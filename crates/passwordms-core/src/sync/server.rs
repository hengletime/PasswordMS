//! Host 侧会话状态机（架构文档 §3.1、§3.2、§6.④）。
//!
//! 职责：生成一次性令牌 + 临时 X25519、绑定端口、展示二维码；接受 Joiner 连接，
//! 完成「令牌校验 → ECDH → 密钥确认 → 下发 VK」并执行一次同步交换。

use std::collections::HashMap;
use std::io::Write;
use std::net::TcpListener;
use std::time::Duration;

use zeroize::Zeroizing;

use crate::constants::{PAIR_TOKEN_TTL_SECS, QR_PROTOCOL_VERSION, SYNC_TIME_WINDOW_SECS};
use crate::crypto::keys::{self, VaultKey};
use crate::crypto::rng;
use crate::error::{stage_io, CoreError, CoreResult};
use crate::model::now_ms;
use crate::model::sync_state::{DeviceSyncState, PairedDevice};
use crate::store::repository::Repository;
use crate::sync::blob_transfer;
use crate::sync::handshake::{self, EphemeralKeypair, HostIdentity, SessionKeys};
use crate::sync::merge;
use crate::sync::protocol::{
    open_envelope, seal_envelope, Accept, Confirm, HandshakeMessage, QRPayload, Reject,
    SyncEnvelope, SyncKind, SyncPayload,
};
use crate::sync::replay::ReplayGuard;
use crate::sync::transport;

/// 设备同步密钥包裹的 AAD 前缀。
const DEVICE_KEY_AAD_PREFIX: &[u8] = b"passwordms/v1/device-key/";

/// 一次配对 + 同步的结果。
#[derive(Debug, Clone)]
pub struct HostSyncResult {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub applied: usize,
    pub conflicts: Vec<String>,
    /// D-53：blob 阶段实际新入库的附件数（能力协商通过时才有意义）。
    pub blobs: usize,
}

/// Host 监听会话。
pub struct SyncServer {
    listener: TcpListener,
    token: [u8; 32],
    exp: i64,
    used: bool,
    session_salt: [u8; 32],
    session_id: [u8; 16],
    identity: HostIdentity,
    host_id: String,
    host_name: String,
    /// 本机持久重同步监听端口（随二维码通告给 Joiner，D-11）。
    sync_port: u16,
}

impl SyncServer {
    /// 启动监听并签发一次性令牌。
    ///
    /// `sync_port` 为本机持久重同步监听端口（0 表示无），仅随二维码/配对记录传递地址信息，
    /// **不携带任何密钥材料**。
    pub fn start(
        port: u16,
        host_id: String,
        host_name: String,
        identity: HostIdentity,
        sync_port: u16,
        now: i64,
    ) -> CoreResult<Self> {
        let listener = transport::listen(port)?;
        let token = rng::random_token()?;
        let session_salt = to_arr32(&rng::random_bytes(32)?)?;
        let session_id = to_arr16(&rng::random_bytes(16)?)?;
        Ok(Self {
            listener,
            token,
            exp: now + (PAIR_TOKEN_TTL_SECS as i64) * 1000,
            used: false,
            session_salt,
            session_id,
            identity,
            host_id,
            host_name,
            sync_port,
        })
    }

    /// 本机持久重同步监听端口（D-11）。
    pub fn sync_port(&self) -> u16 {
        self.sync_port
    }

    /// 实际监听端口。
    pub fn local_port(&self) -> CoreResult<u16> {
        Ok(self.listener.local_addr()?.port())
    }

    /// 令牌是否已过期。
    pub fn is_expired(&self, now: i64) -> bool {
        now > self.exp
    }

    /// 令牌是否已被使用。
    pub fn is_used(&self) -> bool {
        self.used
    }

    /// 取消配对（旧令牌立即作废）。
    pub fn cancel(&mut self) {
        self.used = true;
    }

    /// 当前令牌（供 QR 展示）。
    pub fn token(&self) -> [u8; 32] {
        self.token
    }

    /// 构造二维码载荷（D-44：携带全部候选 IP，主选排最前）。
    pub fn qr_payload(&self, mut ips: Vec<String>) -> CoreResult<QRPayload> {
        if ips.is_empty() {
            ips.push("127.0.0.1".to_string());
        }
        Ok(QRPayload {
            v: QR_PROTOCOL_VERSION,
            ip: ips[0].clone(),
            port: self.local_port()?,
            token: self.token.to_vec(),
            host_id: self.host_id.clone(),
            host_name: self.host_name.clone(),
            fp: self.identity.fingerprint(),
            exp: self.exp,
            sync_port: self.sync_port,
            ips,
        })
    }

    /// 接受一次连接：完成握手 + 双向同步交换（阻塞式）。
    pub fn accept_and_sync(&mut self, repo: &mut Repository, now: i64) -> CoreResult<HostSyncResult> {
        let (stream, peer) = self.listener.accept().map_err(|_| CoreError::SyncAborted)?;
        let peer_ip = peer.ip().to_string();
        self.handle_stream(stream, &peer_ip, repo, now)
    }

    /// 非阻塞轮询一次连接；无连接返回 `Ok(None)`。
    ///
    /// 供 UI 线程/工作线程以轮询方式等待连接，并可被 [`SyncServer::cancel`] 中断。
    pub fn poll_accept_and_sync(
        &mut self,
        repo: &mut Repository,
        now: i64,
    ) -> CoreResult<Option<HostSyncResult>> {
        self.listener
            .set_nonblocking(true)
            .map_err(|_| CoreError::SyncAborted)?;
        let accepted = self.listener.accept();
        let _ = self.listener.set_nonblocking(false);
        match accepted {
            Ok((stream, peer)) => {
                let peer_ip = peer.ip().to_string();
                // D-46（关键平台差异）：**Windows 上 accept 出来的 socket 会继承监听
                // socket 的非阻塞模式**（Unix 不会）。本方法刚把 listener 短暂设为
                // 非阻塞做轮询，若不显式改回阻塞，后续 recv 在对端数据尚未到达时
                // 立即返回 WouldBlock——真机配对必炸（日志实证：
                // `hello_recv|io:operation would block`）；而 127.0.0.1 回环投递
                // 零延迟、数据总先于 recv 到达，竞态从不触发，测试因此漏过。
                stream
                    .set_nonblocking(false)
                    .map_err(|_| CoreError::SyncAborted)?;
                Ok(Some(self.handle_stream(stream, &peer_ip, repo, now)?))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(_) => Err(CoreError::SyncAborted),
        }
    }

    /// 处理一个已建立的连接：令牌校验 → ECDH → 密钥确认 → 下发 VK → 同步交换。
    fn handle_stream(
        &mut self,
        mut stream: std::net::TcpStream,
        peer_ip: &str,
        repo: &mut Repository,
        now: i64,
    ) -> CoreResult<HostSyncResult> {
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|_| CoreError::SyncAborted)?;
        // 1) HELLO + 令牌校验。
        let hello_bytes = transport::recv_frame(&mut stream).map_err(|e| stage_io(e, "hello_recv"))?;
        let hello = match serde_json::from_slice::<HandshakeMessage>(&hello_bytes)? {
            HandshakeMessage::Hello(h) => h,
            _ => return Err(CoreError::FingerprintMismatch),
        };

        if self.used {
            send_reject(&mut stream, crate::error::codes::ERR_PAIR_TOKEN_USED)?;
            return Err(CoreError::PairTokenUsed);
        }
        if self.is_expired(now) {
            send_reject(&mut stream, crate::error::codes::ERR_PAIR_TOKEN_EXPIRED)?;
            return Err(CoreError::PairTokenExpired);
        }
        if !handshake::token_matches(&self.token, &hello.token) {
            send_reject(&mut stream, crate::error::codes::ERR_PAIR_TOKEN_INVALID)?;
            return Err(CoreError::PairTokenInvalid);
        }

        // 2) ECDH + 派生会话密钥。
        let host_eph = EphemeralKeypair::generate()?;
        let joiner_pub = to_arr32(&hello.joiner_pub)?;
        let shared = host_eph.shared_secret(&joiner_pub)?;
        let keys = handshake::derive_session_keys(&shared, &self.session_salt)?;

        // 3) ACCEPT（含 sync_key 包裹的 VK 信封）。
        let (vk_nonce, vk_ct) =
            handshake::seal_vault_key(&keys.sync_key[..], &repo.vault_key()[..], &self.session_id)?;
        let accept = Accept {
            v: QR_PROTOCOL_VERSION,
            host_id: self.host_id.clone(),
            host_name: self.host_name.clone(),
            host_pub: host_eph.public_bytes().to_vec(),
            session_id: self.session_id.to_vec(),
            session_salt: self.session_salt.to_vec(),
            vk_nonce,
            vk_ct,
        };
        let accept_frame = serde_json::to_vec(&HandshakeMessage::Accept(accept))?;
        transport::send_frame(&mut stream, &accept_frame).map_err(|e| stage_io(e, "accept_send"))?;

        // 4) 双向密钥确认。
        let transcript = handshake::transcript_hash(&[&hello_bytes[..], &accept_frame[..]]);
        let mac_h = handshake::confirm_mac(&keys.confirm_key, handshake::ROLE_SERVER, &transcript)?;
        transport::send_frame(
            &mut stream,
            &serde_json::to_vec(&HandshakeMessage::Confirm(Confirm { mac: mac_h }))?,
        )
        .map_err(|e| stage_io(e, "confirm_send"))?;

        let joiner_confirm = transport::recv_frame(&mut stream).map_err(|e| stage_io(e, "confirm_recv"))?;
        let mac_j = match serde_json::from_slice::<HandshakeMessage>(&joiner_confirm)? {
            HandshakeMessage::Confirm(c) => c.mac,
            _ => return Err(CoreError::FingerprintMismatch),
        };
        let expected_j =
            handshake::confirm_mac(&keys.confirm_key, handshake::ROLE_CLIENT, &transcript)?;
        if !handshake::verify_mac(&expected_j, &mac_j) {
            return Err(CoreError::FingerprintMismatch);
        }

        // 5) 令牌用后即焚；持久化配对设备与同步状态。
        self.used = true;
        let fingerprint = handshake::fingerprint_of(&joiner_pub);
        let peer_port = if hello.sync_port == 0 {
            None
        } else {
            Some(hello.sync_port)
        };
        persist_device(
            repo,
            &keys,
            &hello.joiner_id,
            &hello.joiner_name,
            &fingerprint,
            Some(peer_ip.to_string()),
            peer_port,
        )?;

        // 6) 双向同步交换。
        let mut state = repo.sync_state(&hello.joiner_id);
        let mut replay = ReplayGuard::with_state(state.last_recv_seq, SYNC_TIME_WINDOW_SECS);
        let (applied, conflicts, blobs) =
            run_exchange(&mut stream, &keys, &self.session_id, &mut replay, &mut state, repo, now)?;
        state.last_recv_seq = replay.last_recv_seq();
        repo.save_sync_state(state)?;
        repo.touch_device_sync(&hello.joiner_id, now)?;

        Ok(HostSyncResult {
            device_id: hello.joiner_id,
            device_name: hello.joiner_name,
            fingerprint,
            applied,
            conflicts,
            blobs,
        })
    }
}

/// 发送 REJECT 帧。
fn send_reject<W: Write>(writer: &mut W, code: &str) -> CoreResult<()> {
    let msg = HandshakeMessage::Reject(Reject {
        code: code.to_string(),
    });
    transport::send_frame(writer, &serde_json::to_vec(&msg)?)
}

/// 持久化已配对设备（`sync_key` 由 VK 包裹；地址仅记录元信息，不含密钥）。
fn persist_device(
    repo: &mut Repository,
    keys: &SessionKeys,
    device_id: &str,
    device_name: &str,
    fingerprint: &str,
    peer_ip: Option<String>,
    peer_port: Option<u16>,
) -> CoreResult<()> {
    let aad = device_key_aad(device_id);
    let wrapped = keys::wrap_item_key(repo.vault_key(), &keys.sync_key[..], &aad)?;
    let device = PairedDevice {
        device_id: device_id.to_string(),
        device_name: device_name.to_string(),
        fingerprint: fingerprint.to_string(),
        sync_key_wrapped: wrapped,
        paired_at: now_ms(),
        last_sync_at: None,
        peer_ip,
        peer_port,
    };
    repo.upsert_paired_device(device)
}

/// Host 侧一次同步交换：收 Push → 合并 → 回 Push → 收 Done →（blob 阶段）。
///
/// 序号单调递增（从持久化的 `last_sent_seq` 起），以支持重复同步（D-11）。
/// 返回 `(applied, conflicts, blobs_appended)`。
fn run_exchange(
    stream: &mut std::net::TcpStream,
    keys: &SessionKeys,
    session_id: &[u8],
    replay: &mut ReplayGuard,
    state: &mut DeviceSyncState,
    repo: &mut Repository,
    now: i64,
) -> CoreResult<(usize, Vec<String>, usize)> {
    // 收 Joiner 的 Push。
    let push_bytes = transport::recv_frame(stream).map_err(|e| stage_io(e, "push_recv"))?;
    let push_env: SyncEnvelope = serde_json::from_slice(&push_bytes)?;
    replay.accept(push_env.seq, push_env.ts, now)?;
    let push_payload = open_envelope(&keys.sync_key[..], session_id, &push_env)?;
    let report = merge::apply_changes(repo, state, &push_payload.changes)?;

    // 回送本端变更（序号在持久化基准上递增，避免与上一轮重放；D-53：随报文通告本端 blob 能力；
    // D-56：载荷含墓碑，按对端 base 过滤）。
    let out_seq = state.last_sent_seq + 1;
    let reply = SyncPayload {
        kind: SyncKind::Push,
        device_id: repo.device_id().to_string(),
        cursor: 0,
        changes: repo.export_changes(state),
        conflicts: report.conflicts.clone(),
        blob_support: true,
    };
    let reply_env = seal_envelope(&keys.sync_key[..], session_id, out_seq, now, &reply)?;
    transport::send_frame(stream, &serde_json::to_vec(&reply_env)?).map_err(|e| stage_io(e, "reply_send"))?;
    // D-56：发送成功后推进已导出墓碑的 base（下一轮免重发）。
    merge::note_sent_tombstones(state, &reply.changes);

    // 收 Done。
    let done_bytes = transport::recv_frame(stream).map_err(|e| stage_io(e, "done_recv"))?;
    let done_env: SyncEnvelope = serde_json::from_slice(&done_bytes)?;
    replay.accept(done_env.seq, done_env.ts, now)?;
    let _ = open_envelope(&keys.sync_key[..], session_id, &done_env)?;

    // D-53 blob 阶段：双方都支持才进入；seq 从 Done 之后单调延续（过 ReplayGuard）。
    // Host 次序：先供 Joiner 拉的（收 Pull、发 Meta/Data/End），再拉 Joiner 有的（发 Pull、收 Meta/Data/End）——
    // 与 Joiner 侧「先拉后供」严格镜像，避免半双工交织。
    // 注意 send_blob 是「先用后增」：reply Push 已占用 out_seq，故 blob 报文须从 out_seq+1 起。
    let mut sent = out_seq;
    let mut blobs = 0;
    if blob_transfer::blob_phase_enabled(true, push_payload.blob_support) {
        sent = out_seq + 1;
        blob_transfer::serve_pull(
            stream,
            &keys.sync_key[..],
            session_id,
            replay,
            repo,
            &mut sent,
            now,
        )?;
        let phase = blob_transfer::pull_missing(
            stream,
            &keys.sync_key[..],
            session_id,
            replay,
            repo,
            &mut sent,
            now,
        )?;
        blobs = phase.appended;
    }

    state.last_sent_seq = sent;
    Ok((report.applied, report.conflicts, blobs))
}

/// 设备密钥包裹 AAD。
pub fn device_key_aad(device_id: &str) -> Vec<u8> {
    let mut aad = DEVICE_KEY_AAD_PREFIX.to_vec();
    aad.extend_from_slice(device_id.as_bytes());
    aad
}

/// 切片 → 定长 32 字节（内部工具）。
pub(crate) fn to_arr32(bytes: &[u8]) -> CoreResult<[u8; 32]> {
    if bytes.len() != 32 {
        return Err(CoreError::Crypto("expected 32 bytes".to_string()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    Ok(out)
}

/// 切片 → 定长 16 字节（内部工具）。
pub(crate) fn to_arr16(bytes: &[u8]) -> CoreResult<[u8; 16]> {
    if bytes.len() != 16 {
        return Err(CoreError::Crypto("expected 16 bytes".to_string()));
    }
    let mut out = [0u8; 16];
    out.copy_from_slice(bytes);
    Ok(out)
}

/// 未使用的占位保留：`HashMap` 供后续多设备并发状态扩展。
#[allow(dead_code)]
type DeviceStateMap = HashMap<String, DeviceSyncState>;

/// 供外部构造空密钥（避免 `Zeroizing` 未使用告警）。
#[allow(dead_code)]
fn _zero_key() -> Zeroizing<[u8; 32]> {
    Zeroizing::new([0u8; 32])
}

/// 供外部引用 VaultKey 类型（保持导入稳定）。
#[allow(dead_code)]
fn _vault_key_marker(_k: &VaultKey) {}
