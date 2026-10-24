//! Joiner 侧会话（架构文档 §3.2、§6.④）。
//!
//! 流程：扫码解析 → 校验 exp → 连接 Host → HELLO → 收 ACCEPT（含 VK 信封）+ 收 CONFIRM
//! → 双向密钥确认 → 解出 Host 的 VK（**不自动采用**，交由会话层按 U1/U2 裁定处理）
//! → 发起一次双向同步交换。

use std::net::TcpStream;
use std::time::Duration;

use zeroize::Zeroizing;

use crate::constants::SYNC_TIME_WINDOW_SECS;
use crate::crypto::keys;
use crate::error::{CoreError, CoreResult};
use crate::model::now_ms;
use crate::model::sync_state::{DeviceSyncState, PairedDevice};
use crate::store::repository::Repository;
use crate::sync::blob_transfer;
use crate::sync::handshake::{self, EphemeralKeypair, SessionKeys};
use crate::sync::merge;
use crate::sync::protocol::{
    open_envelope, seal_envelope, Confirm, HandshakeMessage, Hello, QRPayload, SyncEnvelope,
    SyncKind, SyncPayload,
};
use crate::sync::replay::ReplayGuard;
use crate::sync::server::{device_key_aad, to_arr32};
use crate::sync::transport;

/// 配对完成后的会话上下文。
pub struct JoinerSession {
    stream: TcpStream,
    keys: SessionKeys,
    session_id: Vec<u8>,
    host_id: String,
    host_name: String,
    /// 本次配对的**会话码**（`sha256(Accept.host_pub)`，D-69）：只用于两屏人工核对。
    ///
    /// 每次配对都变——它绑的是本次握手的临时密钥，不是设备身份。
    session_code: String,
    /// Host 的**设备指纹**（长期身份密钥，取自二维码 `fp`，D-67/D-69）。
    ///
    /// 稳定不变，写进 `PairedDevice.fingerprint` 供设备列表辨识；**不参与**本次核对。
    device_fingerprint: String,
    /// Host 的局域网地址与其持久重同步端口（供 `sync_now` 记录，D-11）。
    host_ip: String,
    host_sync_port: u16,
    /// Host 下发的保险库密钥（**尚未采用**；采用需满足「Joiner 本地库为空」裁定）。
    host_vault_key: Zeroizing<[u8; 32]>,
}

/// 一次同步交换结果。
#[derive(Debug, Clone, Default)]
pub struct JoinerSyncResult {
    pub applied: usize,
    pub conflicts: Vec<String>,
    /// D-53：blob 阶段实际新入库的附件数（能力协商通过时才有意义）。
    pub blobs: usize,
}

impl JoinerSession {
    /// Host 设备 ID。
    pub fn host_id(&self) -> &str {
        &self.host_id
    }

    /// Host 展示名。
    pub fn host_name(&self) -> &str {
        &self.host_name
    }

    /// 本次配对的会话码（供两屏核对，D-69）。
    pub fn session_code(&self) -> &str {
        &self.session_code
    }

    /// Host 的长期设备指纹（稳定，供设备列表辨识）。
    pub fn device_fingerprint(&self) -> &str {
        &self.device_fingerprint
    }

    /// Host 下发的 VK（供会话层决定是否采用）。
    pub fn host_vault_key(&self) -> &[u8; 32] {
        &self.host_vault_key
    }

    /// 持久化本端对 Host 的配对记录（`sync_key` 由本端 VK 包裹）。
    ///
    /// 前提：`repo` 已采用 Host 的 VK（否则包裹出的密钥无法被对端解出）。
    pub fn persist_host_device(&self, repo: &mut Repository) -> CoreResult<()> {
        let aad = device_key_aad(&self.host_id);
        let wrapped = keys::wrap_item_key(repo.vault_key(), &self.keys.sync_key[..], &aad)?;
        let peer_port = if self.host_sync_port == 0 {
            None
        } else {
            Some(self.host_sync_port)
        };
        let device = PairedDevice {
            device_id: self.host_id.clone(),
            device_name: self.host_name.clone(),
            fingerprint: self.device_fingerprint.clone(),
            sync_key_wrapped: wrapped,
            paired_at: now_ms(),
            last_sync_at: None,
            peer_ip: Some(self.host_ip.clone()),
            peer_port,
        };
        repo.upsert_paired_device(device)
    }

    /// 执行一次双向同步交换：发 Push → 收 Host 变更并合并 → 发 Done。
    ///
    /// 序号在持久化的 `last_sent_seq` 基础上单调递增，以支持「配对后再同步」（D-11）。
    pub fn push_and_sync(
        &mut self,
        repo: &mut Repository,
        now: i64,
    ) -> CoreResult<JoinerSyncResult> {
        let mut state: DeviceSyncState = repo.sync_state(&self.host_id);
        let mut replay = ReplayGuard::with_state(state.last_recv_seq, SYNC_TIME_WINDOW_SECS);

        // 单调推送序号。
        let c1 = state.last_sent_seq + 1;
        let c2 = c1 + 1;

        // seq=c1：推送本端变更（D-53：随报文通告本端 blob 能力；D-56：载荷含墓碑，
        // 按对端 base 过滤）。
        let push = SyncPayload {
            kind: SyncKind::Push,
            device_id: repo.device_id().to_string(),
            cursor: 0,
            changes: repo.export_changes(&state),
            conflicts: Vec::new(),
            blob_support: true,
        };
        let push_env = seal_envelope(&self.keys.sync_key[..], &self.session_id, c1, now, &push)?;
        transport::send_frame(&mut self.stream, &serde_json::to_vec(&push_env)?)?;
        // D-56：发送成功后推进已导出墓碑的 base（下一轮免重发；失败路径不落盘自动重发）。
        merge::note_sent_tombstones(&mut state, &push.changes);

        // 接收 Host 的 Push 并合并。
        let reply_bytes = transport::recv_frame(&mut self.stream)?;
        let reply_env: SyncEnvelope = serde_json::from_slice(&reply_bytes)?;
        replay.accept(reply_env.seq, reply_env.ts, now)?;
        let reply = open_envelope(&self.keys.sync_key[..], &self.session_id, &reply_env)?;
        let report = merge::apply_changes(repo, &mut state, &reply.changes)?;

        // seq=c2：发送 Done。
        let done = SyncPayload {
            kind: SyncKind::Done,
            device_id: repo.device_id().to_string(),
            cursor: state.last_recv_seq,
            changes: Vec::new(),
            conflicts: report.conflicts.clone(),
            blob_support: true,
        };
        let done_env = seal_envelope(&self.keys.sync_key[..], &self.session_id, c2, now, &done)?;
        transport::send_frame(&mut self.stream, &serde_json::to_vec(&done_env)?)?;

        // D-53 blob 阶段：双方都支持才进入；seq 从 Done 之后单调延续（过 ReplayGuard）。
        // 先我拉对端缺的，再我供对端拉的（串行，与 Host 侧次序镜像）。
        // 注意 send_blob 是「先用后增」：Done 已占用 c2，故 blob 报文须从 c2+1 起，
        // 否则第一条 Pull 与 Done 同 seq 会被对端 ReplayGuard 拒绝。
        let mut sent = c2;
        let mut blobs = 0;
        if blob_transfer::blob_phase_enabled(true, reply.blob_support) {
            sent = c2 + 1;
            let phase = blob_transfer::pull_missing(
                &mut self.stream,
                &self.keys.sync_key[..],
                &self.session_id,
                &mut replay,
                repo,
                &mut sent,
                now,
            )?;
            blobs = phase.appended;
            blob_transfer::serve_pull(
                &mut self.stream,
                &self.keys.sync_key[..],
                &self.session_id,
                &mut replay,
                repo,
                &mut sent,
                now,
            )?;
        }

        state.last_recv_seq = replay.last_recv_seq();
        state.last_sent_seq = sent;
        repo.save_sync_state(state)?;
        repo.touch_device_sync(&self.host_id, now)?;

        Ok(JoinerSyncResult {
            applied: report.applied,
            conflicts: report.conflicts,
            blobs,
        })
    }
}

/// 与 Host 完成配对握手，返回会话上下文（含 Host 的 VK，但**尚未采用**）。
///
/// `joiner_sync_port` 为本机持久重同步监听端口（0 = 未提供），随 HELLO 通告给 Host。
pub fn pair(
    qr: &QRPayload,
    joiner_id: &str,
    joiner_name: &str,
    joiner_sync_port: u16,
    now: i64,
) -> CoreResult<JoinerSession> {
    if now > qr.exp {
        return Err(CoreError::PairTokenExpired);
    }
    // D-44：多候选 IP 逐个尝试（主选在前）。多网卡 Host 广播的第一个地址可能
    // 手机根本路由不到（如 WSL/Hyper-V 虚拟网卡）；每个候选 4 秒短超时，
    // 全部失败时抛出**最后一个**连接错误（保留 io kind → 对应网络文案），
    // 不再折叠成「同步已中止」。
    let mut candidates: Vec<String> = Vec::new();
    for ip in qr.ips.iter().chain(std::iter::once(&qr.ip)) {
        if !candidates.contains(ip) {
            candidates.push(ip.clone());
        }
    }
    let mut connected: Option<(TcpStream, String)> = None;
    let mut last_err: Option<CoreError> = None;
    for ip in &candidates {
        match transport::connect_with_timeout(
            ip,
            qr.port,
            std::time::Duration::from_secs(crate::constants::SYNC_CONNECT_TIMEOUT_SECS),
        ) {
            Ok(stream) => {
                // D-91（审查 C1，D-44 回归）：记录**实际连通**的候选 IP——
                // 此前持久化的一直是主选 qr.ip，多网卡 Host 在 fallback 候选上
                // 握手成功时，配对后首次 sync_now 必连错地址失败一次。
                connected = Some((stream, ip.clone()));
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }
    let (mut stream, host_ip) = match connected {
        Some((s, ip)) => (s, ip),
        None => return Err(last_err.unwrap_or(CoreError::SyncAborted)),
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|_| CoreError::SyncAborted)?;

    let joiner_eph = EphemeralKeypair::generate()?;
    let hello = Hello {
        v: crate::constants::QR_PROTOCOL_VERSION,
        token: qr.token.clone(),
        joiner_id: joiner_id.to_string(),
        joiner_name: joiner_name.to_string(),
        joiner_pub: joiner_eph.public_bytes().to_vec(),
        sync_port: joiner_sync_port,
    };
    let hello_frame = serde_json::to_vec(&HandshakeMessage::Hello(hello))?;
    transport::send_frame(&mut stream, &hello_frame)?;

    // 收 ACCEPT。
    let accept_bytes = transport::recv_frame(&mut stream)?;
    let accept = match serde_json::from_slice::<HandshakeMessage>(&accept_bytes)? {
        HandshakeMessage::Accept(a) => a,
        HandshakeMessage::Reject(r) => return Err(reject_to_error(&r.code)),
        _ => return Err(CoreError::FingerprintMismatch),
    };

    // 收 Host 的 CONFIRM。
    let confirm_bytes = transport::recv_frame(&mut stream)?;
    let confirm_h = match serde_json::from_slice::<HandshakeMessage>(&confirm_bytes)? {
        HandshakeMessage::Confirm(c) => c,
        _ => return Err(CoreError::FingerprintMismatch),
    };

    // 派生会话密钥。
    let host_pub = to_arr32(&accept.host_pub)?;
    let shared = joiner_eph.shared_secret(&host_pub)?;
    if accept.session_salt.len() != 32 {
        return Err(CoreError::FingerprintMismatch);
    }
    let keys = handshake::derive_session_keys(&shared, &accept.session_salt)?;

    // 双向密钥确认（拦截未持有 shared 的中间人）。
    let transcript = handshake::transcript_hash(&[&hello_frame[..], &accept_bytes[..]]);
    let expected_h =
        handshake::confirm_mac(&keys.confirm_key, handshake::ROLE_SERVER, &transcript)?;
    if !handshake::verify_mac(&expected_h, &confirm_h.mac) {
        return Err(CoreError::FingerprintMismatch);
    }
    let mac_j = handshake::confirm_mac(&keys.confirm_key, handshake::ROLE_CLIENT, &transcript)?;
    transport::send_frame(
        &mut stream,
        &serde_json::to_vec(&HandshakeMessage::Confirm(Confirm { mac: mac_j }))?,
    )?;

    // 解出 Host 的 VK。
    let host_vault_key = handshake::open_vault_key(
        &keys.sync_key[..],
        &accept.vk_nonce,
        &accept.vk_ct,
        &accept.session_id,
    )?;

    Ok(JoinerSession {
        stream,
        keys,
        session_id: accept.session_id.clone(),
        host_id: accept.host_id,
        host_name: accept.host_name,
        session_code: handshake::fingerprint_of(&host_pub),
        device_fingerprint: qr.fp.clone(),
        // D-91（审查 C1）：持久化实际连通的 IP（而非主选 qr.ip），配对地址自愈（D-74）
        // 之前的首连成功率由此保证。
        host_ip,
        host_sync_port: qr.sync_port,
        host_vault_key,
    })
}

/// REJECT code → CoreError。
fn reject_to_error(code: &str) -> CoreError {
    use crate::error::codes::*;
    match code {
        ERR_PAIR_TOKEN_USED => CoreError::PairTokenUsed,
        ERR_PAIR_TOKEN_EXPIRED => CoreError::PairTokenExpired,
        ERR_PAIR_TOKEN_INVALID => CoreError::PairTokenInvalid,
        _ => CoreError::SyncAborted,
    }
}
