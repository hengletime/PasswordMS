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
use crate::error::{stage_io, CoreError, CoreResult};
use crate::model::now_ms;
use crate::model::sync_state::{DeviceSyncState, PairedDevice};
use crate::store::repository::Repository;
use crate::sync::access::{DirectAccess, RepoAccess};
use crate::sync::blob_transfer;
use crate::sync::handshake::{self, EphemeralKeypair, SessionKeys};
use crate::sync::merge;
use crate::sync::paging;
use crate::sync::protocol::{
    seal_envelope, Confirm, HandshakeMessage, Hello, QRPayload, SyncKind, SyncPayload,
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
    /// 本次通道只做配对、不交换数据（D-147）。
    ///
    /// 默认 false = 旧行为（配对握手里夹一次完整双向同步）。配对命令在调用
    /// [`Self::push_and_sync_access`] 之前置 true：那一轮两端都不 merge、不推进水位，
    /// 数据留给用户自己在状态页点「立即同步」。
    pair_only: bool,
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

    /// 把本轮交换标记为「只配对、不交换数据」（D-147）。
    ///
    /// 影响 [`Self::push_and_sync_access`] 的三处：不发本端变更、不合并对端变更、
    /// 不推进任何同步水位。**配对关系本身的持久化不受影响**（那一步在
    /// `persist_host_device`，发生在调用本方法之前）。
    pub fn mark_pair_only(&mut self) {
        self.pair_only = true;
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

    /// 执行一次双向同步交换：发 Push（O1：可多页）→ 收 Host 变更并合并 → 发 Done。
    ///
    /// 序号在持久化的 `last_sent_seq` 基础上单调递增，以支持「配对后再同步」（D-11）。
    ///
    /// 兼容入口（既有测试 / 直连调用方）：repo 已由调用方持锁。
    pub fn push_and_sync(
        &mut self,
        repo: &mut Repository,
        now: i64,
    ) -> CoreResult<JoinerSyncResult> {
        let mut access = DirectAccess(repo);
        self.push_and_sync_access(&mut access, now)
    }

    /// [`Self::push_and_sync`] 的三段式版本（**O2**：网络等待全程不持会话锁）。
    ///
    /// - 锁内段①（毫秒级）：状态初值 + 导出待发页（密文，可安全带出锁外）；
    /// - 网络段：发 Push 页、收 Host Push 页（密文载荷）；
    /// - 锁内段②（毫秒级）：vault 身份校验 → 合并收到的变更（含回声水位推进）；
    /// - 网络段：发 Done、blob 阶段（按批进出锁）；
    /// - 锁内段③（毫秒级）：持久化游标。
    ///
    /// 中途会话被锁（自动锁定竞态）⇒ `access` 返回 `Err(NotUnlocked)` ⇒ 交换整体放弃，
    /// 游标不落盘、下一轮自动重发（宁多勿丢；密文暂存随连接丢弃，无明文残留）。
    pub fn push_and_sync_access(
        &mut self,
        access: &mut dyn RepoAccess,
        now: i64,
    ) -> CoreResult<JoinerSyncResult> {
        // ── 锁内段①（O2，毫秒级）：状态初值 + 导出待发页。
        // 锁内段①的暂存类型（O2 三段式）：RepoAccess 闭包外只能持此不透明结果。
        type PreparedSection = CoreResult<(
            DeviceSyncState,
            Vec<Vec<crate::sync::protocol::ChangeRecord>>,
            String,
        )>;
        let mut prepared: Option<PreparedSection> = None;
        access.with_repo(Box::new(|repo| {
            let st = repo.sync_state(&self.host_id);
            // D-147：只配对时**一条本地变更都不发**。`paginate` 对空集仍返回一页空
            // 载荷（协议要求必有一帧 Push），对端据此知道「这一轮没东西」而不是在等。
            let changes = if self.pair_only {
                Vec::new()
            } else {
                repo.export_changes(&st)
            };
            match paging::paginate(changes) {
                Ok(pgs) => {
                    let my_id = repo.device_id().to_string();
                    prepared = Some(Ok((st, pgs, my_id)));
                }
                Err(e) => prepared = Some(Err(e)),
            }
            Ok(())
        }))?;
        let (mut state, pages, my_device_id) = prepared.take().ok_or(CoreError::SyncAborted)??;
        let mut replay = ReplayGuard::with_state(state.last_recv_seq, SYNC_TIME_WINDOW_SECS);

        // 网络段（O2，不持锁）：seq=c1.. 推送本端变更（D-53：随报文通告本端 blob 能力；
        // D-56：载荷含墓碑，按对端 base 过滤；O1：live 按发送水位过滤、超预算自动分页）。
        let mut seq = state.last_sent_seq + 1;
        paging::send_push_pages(
            &mut self.stream,
            &self.keys.sync_key[..],
            &self.session_id,
            &mut seq,
            now,
            &my_device_id,
            &pages,
            Vec::new(),
            self.pair_only,
        )?;
        // D-56/O1：发送成功后推进已导出墓碑的 base 与 live 水位（下一轮免重发；
        // 失败路径不落盘自动重发）。
        for page in &pages {
            merge::note_sent_tombstones(&mut state, page);
            merge::note_sent_changes(&mut state, page);
        }

        // 网络段（O2）：接收 Host 的 Push（O1：多页聚合，密文载荷）。
        let reply = paging::recv_push_pages(
            &mut self.stream,
            &self.keys.sync_key[..],
            &self.session_id,
            &mut replay,
            now,
        )?;
        // D-147：**以对端的确认为准**。回包带 `pair_only` = 双方都同意「这一轮只配对」，
        // 于是一个字节都不落地、水位也不动；回包不带（老版本 Host 会忽略我们发的标记）
        // 则照常合并——宁可多同步一次，也绝不能让对端已推进的 base 与本端实际持有对不上
        // （那会让这批变更在后续任何一轮都不再重发，等于静默丢数据）。
        //
        // 注意：这只跳过「合并」这一段，**Done 必须照发**——对端正等着它收尾。
        let skip_merge = reply.pair_only;
        let reply_changes = reply.changes;
        let reply_blob_support = reply.blob_support;

        // ── 锁内段②（O2，毫秒级）：vault 身份校验 → 合并。
        // D-106（豆包审查 3.3-1）：直接复用段①导出后在**内存 state** 上推进过发送
        // 水位（W'）的同一份状态——原实现在这里重新 `repo.sync_state()` 读盘（仍是
        // 旧水位 W），W' 整体丢弃未落盘，配对 → 首轮重同步整库全量重发（O1 失效）。
        // 逐行对齐 resync.rs 的正确实现；不丢数据（merge 幂等）但大库性能回退。
        let sent_flat: Vec<crate::sync::protocol::ChangeRecord> =
            pages.iter().flatten().cloned().collect();
        let mut merged: Option<CoreResult<(usize, Vec<String>)>> = None;
        if skip_merge {
            merged = Some(Ok((0, Vec::new())));
        } else {
            access.with_repo(Box::new(|repo| {
                if repo.device_id().as_bytes() != my_device_id.as_bytes() {
                    // 锁定后换库重开（device_id 变化）→ 本场交换作废，密文暂存丢弃。
                    return Err(CoreError::SyncAborted);
                }
                // D-109：发送侧 base 推进——本端已成功发出的版本，对端至少持有。
                merge::note_sent_base(repo, &mut state, &sent_flat);
                let rep = merge::apply_changes(repo, &mut state, &reply_changes)?;
                // O1 回声消除：对端发来的 live 变更证明对端已持有该 rev → 回发水位同步推进。
                merge::note_received_changes(repo, &mut state, &reply_changes);
                merged = Some(Ok((rep.applied, rep.conflicts.clone())));
                Ok(())
            }))?;
        }
        let (applied, conflicts) = merged.take().ok_or(CoreError::SyncAborted)??;

        // 网络段（O2）：Done（seq 延续分页序号）。
        let done = SyncPayload {
            kind: SyncKind::Done,
            device_id: my_device_id.clone(),
            cursor: 0,
            changes: Vec::new(),
            conflicts: conflicts.clone(),
            blob_support: true,
            pair_only: self.pair_only,
        };
        let done_env = seal_envelope(&self.keys.sync_key[..], &self.session_id, seq, now, &done)?;
        transport::send_frame(&mut self.stream, &serde_json::to_vec(&done_env)?)?;

        // 网络段（O2）：D-53 blob 阶段——双方都支持才进入；seq 从 Done 之后单调延续
        // （过 ReplayGuard）。先我拉对端缺的，再我供对端拉的（串行，与 Host 侧次序镜像）。
        // 注意 send_blob 是「先用后增」：Done 已占用 seq，故 blob 报文须从 seq+1 起。
        // blob 内部按「落盘/读帧」粒度进出锁（access），网络等待全程锁外。
        let mut sent = seq;
        let mut blobs = 0;
        // D-147：只配对的一轮不谈附件——对端也没通告 blob 能力（`send_push_pages`
        // 在 pair_only 时置 false），这里再显式短路一次，免得将来有人把上面的
        // `blob_support` 改成恒 true。
        if !skip_merge && blob_transfer::blob_phase_enabled(true, reply_blob_support) {
            sent = seq + 1;
            let phase = blob_transfer::pull_missing(
                &mut self.stream,
                &self.keys.sync_key[..],
                &self.session_id,
                &mut replay,
                access,
                &mut sent,
                now,
            )?;
            blobs = phase.appended;
            blob_transfer::serve_pull(
                &mut self.stream,
                &self.keys.sync_key[..],
                &self.session_id,
                &mut replay,
                access,
                &mut sent,
                now,
            )?;
        }

        // ── 锁内段③（O2，毫秒级）：持久化游标。
        // D-147：只配对的一轮**不落盘任何同步水位**（连 `last_sync_at` 也不动——
        // 配对不是一次同步）。配对关系本身已由 `persist_host_device` 写过。
        if skip_merge {
            return Ok(JoinerSyncResult {
                applied: 0,
                conflicts: Vec::new(),
                blobs: 0,
            });
        }
        state.last_recv_seq = replay.last_recv_seq();
        state.last_sent_seq = sent;
        let final_state = state.clone();
        let host_id = self.host_id.clone();
        access.with_repo(Box::new(|repo| {
            if repo.device_id().as_bytes() != my_device_id.as_bytes() {
                return Err(CoreError::SyncAborted);
            }
            repo.save_sync_state(final_state)?;
            repo.touch_device_sync(&host_id, now)?;
            Ok(())
        }))?;

        Ok(JoinerSyncResult {
            applied,
            conflicts,
            blobs,
        })
    }
}

/// 与 Host 完成配对握手，返回会话上下文（含 Host 的 VK，但**尚未采用**）。
///
/// `joiner_sync_port` 为本机持久重同步监听端口（0 = 未提供），随 HELLO 通告给 Host。
/// `joiner_identity_pub` 为本机**长期**身份公钥（X25519）——【D-70】随 HELLO 发给 Host，
/// 让电脑端能用它算出稳定的设备指纹。传 `None` = 本端没有长期身份（或加载失败），
/// 等于老客户端行为：对端指纹留空、UI 不渲染指纹行。
///
/// 【D-70】绝不能把 `joiner_eph` 的临时公钥当身份发出去。它是每次配对新生成的，
/// 用它算指纹会让同一台手机每次配对都变成「新设备」——正是 D-67 定性过的错误。
pub fn pair(
    qr: &QRPayload,
    joiner_id: &str,
    joiner_name: &str,
    joiner_sync_port: u16,
    joiner_identity_pub: Option<[u8; 32]>,
    now: i64,
) -> CoreResult<JoinerSession> {
    if now > qr.exp {
        return Err(CoreError::PairTokenExpired);
    }
    // D-44：多候选 IP 逐个尝试（主选在前）。多网卡 Host 广播的第一个地址可能
    // 手机根本路由不到（如 WSL/Hyper-V 虚拟网卡）；每个候选 4 秒短超时。
    let mut candidates: Vec<String> = Vec::new();
    for ip in qr.ips.iter().chain(std::iter::once(&qr.ip)) {
        if !candidates.contains(ip) {
            candidates.push(ip.clone());
        }
    }
    let mut connected: Option<(TcpStream, String)> = None;
    // D-138：逐候选记录 connect 阶段错误（统一 stage 标注 → 全部失败时映射
    // err.network_unreachable，与 D-57 对 sync_now 的裁定一致）。
    let mut errs: Vec<CoreError> = Vec::new();
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
            Err(e) => errs.push(stage_io(e, "connect")),
        }
    }
    let (mut stream, host_ip) = match connected {
        Some((s, ip)) => (s, ip),
        // D-138：不再「最后一个候选说了算」——候选表末位常是 WSL/Hyper-V 虚拟网卡
        // 地址，对端永远超时，把真实错误（如「连接被拒 = 对端可达但已离开配对等待页」）
        // 掩盖成笼统的「网络响应超时」（主人实测排查一整天）。选错优先级：
        // 连接被拒 > 其它非超时错误 > 超时（全部 candidates 均试尽后才走到这里）。
        None => {
            let refused = errs.iter().position(|e| io_kind_contains(e, "refused"));
            let other = errs.iter().position(|e| !io_kind_contains(e, "timedout"));
            match refused.or(other) {
                Some(i) => return Err(errs.swap_remove(i)),
                None => return Err(errs.pop().unwrap_or(CoreError::SyncAborted)),
            }
        }
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
        // D-70：长期身份公钥（不是上面那把临时公钥）。缺省空 = 老客户端行为。
        joiner_identity_pub: joiner_identity_pub.map(|k| k.to_vec()).unwrap_or_default(),
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
        pair_only: false,
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

/// D-138：Io 载荷（`io:Kind`，可能带 `stage|` 前缀）归一化后是否含某关键词。
/// 归一化口径与 `error.rs` message_key 的映射一致：去非字母数字、小写——
/// `io:TimedOut` / `io:timed out` / `connect|io:timed out` 都能命中 "timedout"。
fn io_kind_contains(err: &CoreError, needle: &str) -> bool {
    match err {
        CoreError::Io(kind) => kind
            .to_ascii_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .contains(needle),
        _ => false,
    }
}

/// D-138：候选选错策略的归一化判据（Windows「timed out」/ Linux「TimedOut」/
/// 带 `connect|` 阶段前缀三种形态都必须命中）。
#[cfg(test)]
mod io_kind_tests {
    use super::*;

    #[test]
    fn normalizes_platform_and_staged_forms() {
        assert!(io_kind_contains(
            &CoreError::Io("io:TimedOut".into()),
            "timedout"
        ));
        assert!(io_kind_contains(
            &CoreError::Io("io:timed out".into()),
            "timedout"
        ));
        assert!(io_kind_contains(
            &CoreError::Io("connect|io:timed out".into()),
            "timedout"
        ));
        assert!(io_kind_contains(
            &CoreError::Io("io:ConnectionRefused".into()),
            "refused"
        ));
        assert!(io_kind_contains(
            &CoreError::Io("connect|io:connection refused".into()),
            "refused"
        ));
        // 非超时/非拒连的错误不得误判。
        assert!(!io_kind_contains(
            &CoreError::Io("io:TimedOut".into()),
            "refused"
        ));
        assert!(!io_kind_contains(&CoreError::SyncAborted, "timedout"));
    }
}
