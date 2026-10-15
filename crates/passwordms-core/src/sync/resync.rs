//! 已配对设备的直连重同步（**修复偏差 D-11**）。
//!
//! 与「首次配对」不同，重同步**不涉及一次性令牌，也不重新下发 VK**：两端在配对时已各自
//! 持有同一把 `sync_key`（由各自 VK 包裹后持久化于 `PairedDevice`）。因此重同步流程为：
//!
//! 1. 发起方（客户端）连接对端**持久重同步监听端口**（配对时通告并持久化，D-13）；
//! 2. 交换随机 nonce，基于 `HKDF(sync_key, salt=nonce_c‖nonce_s, info=…/session)` 派生会话密钥
//!    （复用 [`handshake::derive_session_sk`]）；
//! 3. 双方以 HMAC-SHA256 做**双向密钥确认**（角色域分离，复用 [`handshake::confirm_mac`]）；
//! 4. 复用 [`crate::sync::protocol`] 的信封与 [`crate::sync::merge`] 的合并逻辑完成一次
//!    `Push → Push → Done` 交换，冲突写入冲突箱；
//! 5. 同步游标（`last_sent_seq` / `last_recv_seq`）持久化，支持反复同步。
//!
//! **失败路径**：未配对 → [`CoreError::NotPaired`]；密钥确认失败 → [`CoreError::FingerprintMismatch`]；
//! 序号/时间窗异常 → [`CoreError::ReplayDetected`]；协议中断 → [`CoreError::SyncAborted`]。
//! 连接失败自 D-44 起保留 io kind（映射到具体网络文案），不再一律折叠为 SyncAborted。

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::constants::{QR_PROTOCOL_VERSION, SYNC_TIME_WINDOW_SECS};
use crate::crypto::{keys, rng};
use crate::error::{CoreError, CoreResult};
use crate::model::sync_state::DeviceSyncState;
use crate::store::repository::Repository;
use crate::sync::handshake::{self, ROLE_CLIENT, ROLE_SERVER};
use crate::sync::merge;
use crate::sync::protocol::{
    open_envelope, seal_envelope, Reject, SyncEnvelope, SyncKind, SyncPayload,
};
use crate::sync::replay::ReplayGuard;
use crate::sync::server::device_key_aad;
use crate::sync::transport;

/// 重同步 transcript 的域分离标签。
const RESYNC_LABEL: &[u8] = b"passwordms/sync/v1/resync";

/// 重同步握手报文（明文 JSON 外壳；确认通过后进入加密数据阶段）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ResyncMessage {
    Hello(ResyncHello),
    Challenge(ResyncChallenge),
    Confirm(ResyncConfirm),
    Reject(Reject),
}

/// 重同步 HELLO（发起方 → 响应方）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResyncHello {
    pub v: u32,
    pub client_id: String,
    pub client_name: String,
    /// 发起方随机数（32B）。
    #[serde(with = "crate::sync::protocol::b64")]
    pub nonce_c: Vec<u8>,
}

/// 重同步 CHALLENGE（响应方 → 发起方），含响应方密钥确认 MAC。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResyncChallenge {
    pub v: u32,
    pub server_id: String,
    pub server_name: String,
    /// 响应方随机数（32B）。
    #[serde(with = "crate::sync::protocol::b64")]
    pub nonce_s: Vec<u8>,
    /// `HMAC-SHA256(session_key, "server" ‖ transcript)`。
    #[serde(with = "crate::sync::protocol::b64")]
    pub mac: Vec<u8>,
}

/// 重同步 CONFIRM（发起方 → 响应方），发起方密钥确认 MAC。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResyncConfirm {
    /// `HMAC-SHA256(session_key, "client" ‖ transcript)`。
    #[serde(with = "crate::sync::protocol::b64")]
    pub mac: Vec<u8>,
}

/// 重同步结果摘要。
#[derive(Debug, Clone, Default)]
pub struct ResyncOutcome {
    /// 本端应用（写入）的远端变更条数。
    pub applied: usize,
    /// 新产生的冲突条目 ID。
    pub conflicts: Vec<String>,
}

/// 计算 transcript：`sha256(label ‖ client_id ‖ server_id ‖ nonce_c ‖ nonce_s)`。
///
/// 双方均可仅凭各自已知字段复算，无需回放报文字节。
fn transcript_of(client_id: &str, server_id: &str, nonce_c: &[u8], nonce_s: &[u8]) -> [u8; 32] {
    let parts: [&[u8]; 5] = [
        RESYNC_LABEL,
        client_id.as_bytes(),
        server_id.as_bytes(),
        nonce_c,
        nonce_s,
    ];
    handshake::transcript_hash(&parts)
}

/// 由 transcript 派生的会话 ID（前 16 字节，用于信封 AAD）。
fn session_id_of(transcript: &[u8; 32]) -> Vec<u8> {
    transcript[..16].to_vec()
}

/// 由设备 ID 生成便于识别的展示名（不含任何隐私信息）。
pub fn device_display_name(device_id: &str) -> String {
    let short: String = device_id.chars().take(8).collect();
    format!("PasswordMS-{short}")
}

/// 发起一次直连重同步（客户端侧）。
///
/// 读取已配对设备 → 连接其持久监听端口 → 会话密钥派生 + 双向确认 → `Push/Push/Done`
/// 交换 → 合并（含冲突箱）→ 更新游标 → 返回摘要。
pub fn resync_with_peer(
    repo: &mut Repository,
    peer_device_id: &str,
    now: i64,
) -> CoreResult<ResyncOutcome> {
    // 1) 读取已配对设备与地址。
    let peer = repo
        .get_paired_device(peer_device_id)
        .ok_or(CoreError::NotPaired)?;
    let peer_ip = peer.peer_ip.clone().ok_or(CoreError::SyncAborted)?;
    let peer_port = peer.peer_port.ok_or(CoreError::SyncAborted)?;

    // 2) 解出共享同步密钥（仅解锁态可解；AAD 绑定设备身份）。
    let sync_key = keys::unwrap_item_key(
        repo.vault_key(),
        &peer.sync_key_wrapped,
        &device_key_aad(peer_device_id),
    )?;

    // 3) 连接对端持久监听端口。
    let mut stream = transport::connect(&peer_ip, peer_port)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|_| CoreError::SyncAborted)?;

    // 4) HELLO。
    let client_id = repo.device_id().to_string();
    let nonce_c = rng::random_bytes(32)?;
    let hello = ResyncMessage::Hello(ResyncHello {
        v: QR_PROTOCOL_VERSION,
        client_id: client_id.clone(),
        client_name: device_display_name(&client_id),
        nonce_c: nonce_c.clone(),
    });
    transport::send_frame(&mut stream, &serde_json::to_vec(&hello)?)?;

    // 5) CHALLENGE。
    let challenge_bytes = transport::recv_frame(&mut stream)?;
    let challenge = match serde_json::from_slice::<ResyncMessage>(&challenge_bytes)? {
        ResyncMessage::Challenge(c) => c,
        ResyncMessage::Reject(r) => return Err(reject_to_error(&r.code)),
        _ => return Err(CoreError::FingerprintMismatch),
    };
    if challenge.server_id != peer_device_id || challenge.nonce_s.len() != 32 {
        return Err(CoreError::FingerprintMismatch);
    }

    // 6) 派生会话密钥 + 校验响应方确认。
    let transcript = transcript_of(&client_id, &challenge.server_id, &nonce_c, &challenge.nonce_s);
    let sk = handshake::derive_session_sk(&sync_key, &nonce_c, &challenge.nonce_s)?;
    let expected_s = handshake::confirm_mac(&sk, ROLE_SERVER, &transcript)?;
    if !handshake::verify_mac(&expected_s, &challenge.mac) {
        return Err(CoreError::FingerprintMismatch);
    }

    // 7) 回送本端确认。
    let mac_c = handshake::confirm_mac(&sk, ROLE_CLIENT, &transcript)?;
    let confirm = ResyncMessage::Confirm(ResyncConfirm { mac: mac_c });
    transport::send_frame(&mut stream, &serde_json::to_vec(&confirm)?)?;

    let session_id = session_id_of(&transcript);
    let mut state: DeviceSyncState = repo.sync_state(peer_device_id);
    let mut replay = ReplayGuard::with_state(state.last_recv_seq, SYNC_TIME_WINDOW_SECS);
    let c1 = state.last_sent_seq + 1;
    let c2 = c1 + 1;

    // 8) 发本端 Push。
    let push = SyncPayload {
        kind: SyncKind::Push,
        device_id: client_id.clone(),
        cursor: 0,
        changes: repo.export_changes(),
        conflicts: Vec::new(),
    };
    let push_env = seal_envelope(&sk[..], &session_id, c1, now, &push)?;
    transport::send_frame(&mut stream, &serde_json::to_vec(&push_env)?)?;

    // 9) 收对端 Push 并合并。
    let reply_bytes = transport::recv_frame(&mut stream)?;
    let reply_env: SyncEnvelope = serde_json::from_slice(&reply_bytes)?;
    replay.accept(reply_env.seq, reply_env.ts, now)?;
    let reply = open_envelope(&sk[..], &session_id, &reply_env)?;
    let report = merge::apply_changes(repo, &mut state, &reply.changes)?;

    // 10) 发 Done。
    let done = SyncPayload {
        kind: SyncKind::Done,
        device_id: client_id.clone(),
        cursor: state.last_recv_seq,
        changes: Vec::new(),
        conflicts: report.conflicts.clone(),
    };
    let done_env = seal_envelope(&sk[..], &session_id, c2, now, &done)?;
    transport::send_frame(&mut stream, &serde_json::to_vec(&done_env)?)?;

    // 11) 持久化游标。
    state.last_recv_seq = replay.last_recv_seq();
    state.last_sent_seq = c2;
    repo.save_sync_state(state)?;
    repo.touch_device_sync(peer_device_id, now)?;

    Ok(ResyncOutcome {
        applied: report.applied,
        conflicts: report.conflicts,
    })
}

/// 持久重同步监听器（响应方侧）。
///
/// 在保险库解锁期间持续运行，非阻塞轮询接受已配对设备的直连重同步请求。
pub struct ResyncServer {
    listener: TcpListener,
    port: u16,
    local_name: String,
}

impl ResyncServer {
    /// 绑定监听端口（`0` 表示由系统分配临时端口）。
    pub fn start(port: u16, local_name: String) -> CoreResult<Self> {
        let listener = transport::listen(port)?;
        let actual = listener.local_addr()?.port();
        Ok(Self {
            listener,
            port: actual,
            local_name,
        })
    }

    /// 实际监听端口（供配对时通告）。
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 非阻塞轮询一次入站连接；无连接返回 `Ok(None)`。
    pub fn poll_once(
        &mut self,
        repo: &mut Repository,
        now: i64,
    ) -> CoreResult<Option<ResyncOutcome>> {
        self.listener
            .set_nonblocking(true)
            .map_err(|_| CoreError::SyncAborted)?;
        let accepted = self.listener.accept();
        let _ = self.listener.set_nonblocking(false);
        match accepted {
            Ok((stream, _peer)) => Ok(Some(self.handle(stream, repo, now)?)),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(_) => Err(CoreError::SyncAborted),
        }
    }

    /// 处理一个入站重同步连接。
    fn handle(
        &mut self,
        mut stream: TcpStream,
        repo: &mut Repository,
        now: i64,
    ) -> CoreResult<ResyncOutcome> {
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|_| CoreError::SyncAborted)?;

        // 1) HELLO。
        let hello_bytes = transport::recv_frame(&mut stream)?;
        let hello = match serde_json::from_slice::<ResyncMessage>(&hello_bytes)? {
            ResyncMessage::Hello(h) => h,
            _ => return Err(CoreError::FingerprintMismatch),
        };

        // 2) 必须为已配对设备。
        let peer = match repo.get_paired_device(&hello.client_id) {
            Some(p) => p,
            None => {
                send_reject(&mut stream, crate::error::codes::ERR_NOT_PAIRED)?;
                return Err(CoreError::NotPaired);
            }
        };
        if hello.nonce_c.len() != 32 {
            return Err(CoreError::FingerprintMismatch);
        }

        // 3) 解出共享同步密钥 + 派生会话密钥。
        let sync_key = keys::unwrap_item_key(
            repo.vault_key(),
            &peer.sync_key_wrapped,
            &device_key_aad(&hello.client_id),
        )?;
        let server_id = repo.device_id().to_string();
        let nonce_s = rng::random_bytes(32)?;
        let transcript = transcript_of(&hello.client_id, &server_id, &hello.nonce_c, &nonce_s);
        let sk = handshake::derive_session_sk(&sync_key, &hello.nonce_c, &nonce_s)?;

        // 4) CHALLENGE。
        let mac_s = handshake::confirm_mac(&sk, ROLE_SERVER, &transcript)?;
        let challenge = ResyncMessage::Challenge(ResyncChallenge {
            v: QR_PROTOCOL_VERSION,
            server_id: server_id.clone(),
            server_name: self.local_name.clone(),
            nonce_s: nonce_s.clone(),
            mac: mac_s,
        });
        transport::send_frame(&mut stream, &serde_json::to_vec(&challenge)?)?;

        // 5) 校验发起方确认。
        let confirm_bytes = transport::recv_frame(&mut stream)?;
        let confirm = match serde_json::from_slice::<ResyncMessage>(&confirm_bytes)? {
            ResyncMessage::Confirm(c) => c,
            _ => return Err(CoreError::FingerprintMismatch),
        };
        let expected_c = handshake::confirm_mac(&sk, ROLE_CLIENT, &transcript)?;
        if !handshake::verify_mac(&expected_c, &confirm.mac) {
            return Err(CoreError::FingerprintMismatch);
        }

        let session_id = session_id_of(&transcript);
        let mut state: DeviceSyncState = repo.sync_state(&hello.client_id);
        let mut replay = ReplayGuard::with_state(state.last_recv_seq, SYNC_TIME_WINDOW_SECS);

        // 6) 收发起方 Push 并合并。
        let push_bytes = transport::recv_frame(&mut stream)?;
        let push_env: SyncEnvelope = serde_json::from_slice(&push_bytes)?;
        replay.accept(push_env.seq, push_env.ts, now)?;
        let push = open_envelope(&sk[..], &session_id, &push_env)?;
        let report = merge::apply_changes(repo, &mut state, &push.changes)?;

        // 7) 回送本端 Push。
        let s1 = state.last_sent_seq + 1;
        let reply = SyncPayload {
            kind: SyncKind::Push,
            device_id: server_id.clone(),
            cursor: 0,
            changes: repo.export_changes(),
            conflicts: report.conflicts.clone(),
        };
        let reply_env = seal_envelope(&sk[..], &session_id, s1, now, &reply)?;
        transport::send_frame(&mut stream, &serde_json::to_vec(&reply_env)?)?;

        // 8) 收 Done。
        let done_bytes = transport::recv_frame(&mut stream)?;
        let done_env: SyncEnvelope = serde_json::from_slice(&done_bytes)?;
        replay.accept(done_env.seq, done_env.ts, now)?;
        let _ = open_envelope(&sk[..], &session_id, &done_env)?;

        // 9) 持久化游标。
        state.last_recv_seq = replay.last_recv_seq();
        state.last_sent_seq = s1;
        repo.save_sync_state(state)?;
        repo.touch_device_sync(&hello.client_id, now)?;

        Ok(ResyncOutcome {
            applied: report.applied,
            conflicts: report.conflicts,
        })
    }
}

/// 发送 REJECT 帧。
fn send_reject<W: Write>(writer: &mut W, code: &str) -> CoreResult<()> {
    let msg = ResyncMessage::Reject(Reject {
        code: code.to_string(),
    });
    transport::send_frame(writer, &serde_json::to_vec(&msg)?)
}

/// REJECT code → CoreError。
fn reject_to_error(code: &str) -> CoreError {
    use crate::error::codes::*;
    match code {
        ERR_NOT_PAIRED => CoreError::NotPaired,
        ERR_FINGERPRINT_MISMATCH => CoreError::FingerprintMismatch,
        ERR_REPLAY_DETECTED => CoreError::ReplayDetected,
        _ => CoreError::SyncAborted,
    }
}

/// 编译期占位：确保 `Zeroizing` 导入被使用（避免未使用告警）。
#[allow(dead_code)]
fn _zeroizing_marker() -> Zeroizing<[u8; 32]> {
    Zeroizing::new([0u8; 32])
}
