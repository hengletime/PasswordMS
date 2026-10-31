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
use crate::sync::access::{DirectAccess, RepoAccess};
use crate::sync::blob_transfer;
use crate::sync::handshake::{self, EphemeralKeypair, HostIdentity, SessionKeys};
use crate::sync::merge;
use crate::sync::paging;
use crate::sync::protocol::{Accept, Confirm, HandshakeMessage, QRPayload, Reject};
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
    /// 本次配对会话的临时 X25519 密钥对（D-69）。
    ///
    /// 在 `start()` 里一次生成、整个配对窗口内复用，**不再每连接新生成**：
    /// 因为「会话码」必须在手机连上之前就能显示在电脑屏上（两屏同值的前提）。
    /// 复用是安全的——令牌用后即焚（`used`），一个 `SyncServer` 生命周期内只允许完成一次配对。
    host_eph: EphemeralKeypair,
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
        let host_eph = EphemeralKeypair::generate()?;
        Ok(Self {
            listener,
            token,
            exp: now + (PAIR_TOKEN_TTL_SECS as i64) * 1000,
            used: false,
            session_salt,
            session_id,
            identity,
            host_eph,
            host_id,
            host_name,
            sync_port,
        })
    }

    /// 本次配对的**会话码**：`sha256(host_eph 公钥)` 前 8 字节 hex（D-69）。
    ///
    /// 与手机屏核对的是**同一个值**——手机端由 `Accept.host_pub` 算出
    /// （`client.rs` 的 `JoinerSession::session_code`），两屏构造上必然相等。
    ///
    /// 刻意与「设备指纹」（`HostIdentity::fingerprint`，长期身份密钥、稳定不变）
    /// 区分命名：会话码每次配对都变，只用于**本次**人工核对，不能当设备身份用。
    pub fn session_code(&self) -> String {
        handshake::fingerprint_of(&self.host_eph.public_bytes())
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

    /// 兼容入口（既有测试 / 直连调用方）：repo 已由调用方持锁，阻塞接受一次连接。
    pub fn accept_and_sync(
        &mut self,
        repo: &mut Repository,
        now: i64,
    ) -> CoreResult<HostSyncResult> {
        let (stream, peer) = self.listener.accept().map_err(|_| CoreError::SyncAborted)?;
        let peer_ip = peer.ip().to_string();
        let mut access = DirectAccess(repo);
        self.handle_connection(stream, &peer_ip, now, &mut access)
    }

    /// 兼容入口：repo 已由调用方持锁，非阻塞轮询一次连接；无连接返回 `Ok(None)`。
    ///
    /// 供 UI 线程/工作线程以轮询方式等待连接，并可被 [`SyncServer::cancel`] 中断。
    pub fn poll_accept_and_sync(
        &mut self,
        repo: &mut Repository,
        now: i64,
    ) -> CoreResult<Option<HostSyncResult>> {
        let Some((stream, peer_ip)) = self.poll_accept()? else {
            return Ok(None);
        };
        let mut access = DirectAccess(repo);
        self.handle_connection(stream, &peer_ip, now, &mut access)
            .map(Some)
    }

    /// 非阻塞轮询一次连接（**不持任何会话锁**，O2）；无连接返回 `Ok(None)`。
    pub fn poll_accept(&mut self) -> CoreResult<Option<(std::net::TcpStream, String)>> {
        self.listener
            .set_nonblocking(true)
            .map_err(|_| CoreError::SyncAborted)?;
        let accepted = self.listener.accept();
        let _ = self.listener.set_nonblocking(false);
        match accepted {
            Ok((stream, peer)) => {
                // D-46（关键平台差异）：**Windows 上 accept 出来的 socket 会继承监听
                // socket 的非阻塞模式**（Unix 不会）。本方法刚把 listener 短暂设为
                // 非阻塞做轮询，若不显式改回阻塞，后续 recv 在对端数据尚未到达时
                // 立即返回 WouldBlock——真机配对必炸（日志实证：
                // `hello_recv|io:operation would block`）；而 127.0.0.1 回环投递
                // 零延迟、数据总先于 recv 到达，竞态从不触发，测试因此漏过。
                stream
                    .set_nonblocking(false)
                    .map_err(|_| CoreError::SyncAborted)?;
                Ok(Some((stream, peer.ip().to_string())))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(_) => Err(CoreError::SyncAborted),
        }
    }

    /// 处理一个已建立的连接（**O2 三段式**：令牌校验 → ECDH → 密钥确认 → 下发 VK → 同步交换）。
    ///
    /// - 锁内段①（毫秒级）：取本端 VK（ACCEPT 的 VK 信封需要）；
    /// - 网络段：HELLO 校验、ECDH、ACCEPT、双向密钥确认（不接触 repo）；
    /// - 锁内段②（毫秒级）：令牌用后即焚 + 持久化配对设备 + 同步状态初值；
    /// - 网络段：收 Joiner Push 页（密文载荷）；
    /// - 锁内段③（毫秒级）：vault 身份校验 → 合并 → 导出回包页（含水位推进）；
    /// - 网络段：回送 Push 页、收 Done、blob 阶段（按批进出锁）；
    /// - 锁内段④（毫秒级）：持久化游标。
    ///
    /// 中途会话被锁（自动锁定竞态）⇒ `access` 返回 `Err(NotUnlocked)` ⇒ 配对会话作废，
    /// 对端收到连接中断（宁多勿丢；密文暂存随连接丢弃，无明文残留）。
    pub fn handle_connection(
        &mut self,
        mut stream: std::net::TcpStream,
        peer_ip: &str,
        now: i64,
        access: &mut dyn RepoAccess,
    ) -> CoreResult<HostSyncResult> {
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|_| CoreError::SyncAborted)?;
        // 网络段：1) HELLO + 令牌校验（自持状态，不接触 repo）。
        let hello_bytes =
            transport::recv_frame(&mut stream).map_err(|e| stage_io(e, "hello_recv"))?;
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

        // 网络段：2) ECDH + 派生会话密钥（用 `start()` 生成的那把临时密钥，见 `host_eph` 字段注释）。
        let joiner_pub = to_arr32(&hello.joiner_pub)?;
        let shared = self.host_eph.shared_secret(&joiner_pub)?;
        let keys = handshake::derive_session_keys(&shared, &self.session_salt)?;

        // ── 锁内段①（O2，毫秒级）：取本端 VK（ACCEPT 的 VK 信封需要）。
        let mut vk_out: Option<crate::crypto::keys::VaultKey> = None;
        access.with_repo(Box::new(|repo| {
            vk_out = Some(repo.vault_key().clone());
            Ok(())
        }))?;
        let vk = vk_out.ok_or(CoreError::SyncAborted)?;

        // 网络段：3) ACCEPT（含 sync_key 包裹的 VK 信封）。
        let (vk_nonce, vk_ct) =
            handshake::seal_vault_key(&keys.sync_key[..], &vk[..], &self.session_id)?;
        let accept = Accept {
            v: QR_PROTOCOL_VERSION,
            host_id: self.host_id.clone(),
            host_name: self.host_name.clone(),
            host_pub: self.host_eph.public_bytes().to_vec(),
            session_id: self.session_id.to_vec(),
            session_salt: self.session_salt.to_vec(),
            vk_nonce,
            vk_ct,
        };
        let accept_frame = serde_json::to_vec(&HandshakeMessage::Accept(accept))?;
        transport::send_frame(&mut stream, &accept_frame)
            .map_err(|e| stage_io(e, "accept_send"))?;

        // 网络段：4) 双向密钥确认。
        let transcript = handshake::transcript_hash(&[&hello_bytes[..], &accept_frame[..]]);
        let mac_h = handshake::confirm_mac(&keys.confirm_key, handshake::ROLE_SERVER, &transcript)?;
        transport::send_frame(
            &mut stream,
            &serde_json::to_vec(&HandshakeMessage::Confirm(Confirm { mac: mac_h }))?,
        )
        .map_err(|e| stage_io(e, "confirm_send"))?;

        let joiner_confirm =
            transport::recv_frame(&mut stream).map_err(|e| stage_io(e, "confirm_recv"))?;
        let mac_j = match serde_json::from_slice::<HandshakeMessage>(&joiner_confirm)? {
            HandshakeMessage::Confirm(c) => c.mac,
            _ => return Err(CoreError::FingerprintMismatch),
        };
        let expected_j =
            handshake::confirm_mac(&keys.confirm_key, handshake::ROLE_CLIENT, &transcript)?;
        if !handshake::verify_mac(&expected_j, &mac_j) {
            return Err(CoreError::FingerprintMismatch);
        }

        // ── 锁内段②（O2，毫秒级）：令牌用后即焚；持久化配对设备与同步状态。
        // 【D-70】设备指纹取自 HELLO 里的 `joiner_identity_pub`（**长期**身份公钥）。
        // 老客户端不发该字段 → 空 vec → 指纹留空串（= 「本端不掌握对端长期身份」），
        // UI 据此不渲染指纹行。
        //
        // 红线：**绝不**回退到 `joiner_pub`（临时 ECDH 公钥）。它是每次配对新生成的，
        // 算出来的指纹每次都不一样，写进 `PairedDevice.fingerprint` 就是用一次性值
        // 冒充设备身份 —— D-67 定性过的错误，也是 D-69 把指纹置空的原始动机。
        // 长度不符（非 32B）同样留空：畸形输入不得 panic，也不得猜。
        self.used = true;
        let fingerprint = peer_identity_fingerprint(&hello.joiner_identity_pub);
        let peer_port = if hello.sync_port == 0 {
            None
        } else {
            Some(hello.sync_port)
        };
        let joiner_id = hello.joiner_id.clone();
        let joiner_name = hello.joiner_name.clone();
        let mut st_out: Option<CoreResult<DeviceSyncState>> = None;
        access.with_repo(Box::new(|repo| {
            match persist_device(
                repo,
                &keys,
                &joiner_id,
                &joiner_name,
                &fingerprint,
                Some(peer_ip.to_string()),
                peer_port,
            ) {
                Ok(()) => {
                    let mut st = repo.sync_state(&joiner_id);
                    // D-62：pair 通道 = **全新配对会话**（QR 一次性令牌 + 完整握手才走到这里），
                    // 对端 seq 允许从 1 重新开始——解绑重配对场景（remove_paired_device 清了对端
                    // 侧 sync_state）对端游标必然归零，若沿用旧 last_recv_seq 作基线，合法重配对
                    // 会被误判重放（实测 ERR_REPLAY_DETECTED）。故在此重置接收基线；
                    // 重放防护的实质由「pair token 一次性 + merge 幂等判重（base.hash）」承担：
                    // 即使旧信封被重放通过验收，merge 也只做幂等合并，数据不会回滚。
                    // 增量重同步通道（ResyncServer）的重放红线**不受影响**，维持严格单调。
                    st.last_recv_seq = 0;
                    st_out = Some(Ok(st));
                    Ok(())
                }
                Err(e) => {
                    st_out = Some(Err(e));
                    Ok(())
                }
            }
        }))?;
        let state = st_out.take().ok_or(CoreError::SyncAborted)??;
        let mut replay = ReplayGuard::with_state(state.last_recv_seq, SYNC_TIME_WINDOW_SECS);

        // 网络段：5) 收 Joiner 的 Push（O1：多页聚合，密文载荷）。
        let push = paging::recv_push_pages(
            &mut stream,
            &keys.sync_key[..],
            &self.session_id,
            &mut replay,
            now,
        )
        .map_err(|e| stage_io(e, "push_recv"))?;

        // ── 锁内段③（O2，毫秒级）：vault 身份校验 → 合并 → 导出回包页。
        // 水位/墓碑推进在此一并做：发送失败路径 `save_sync_state` 不执行、随会话丢弃，
        // 失败安全语义不变（宁多勿丢）。
        //
        // 【D-147】对端声明 `pair_only` 时整段跳过：不合并它的变更、不导出本端变更、
        // **不推进任何 base/水位**。这一轮只把配对关系留下来，数据由用户自己点
        // 「立即同步」（走增量重同步通道）。协议仍要求本端回一帧 Push（对端在等），
        // 所以回一页空载荷并同样打上 `pair_only`，让对端知道「这一轮没有任何数据」。
        let host_id = self.host_id.clone();
        let pair_only = push.pair_only;
        if pair_only {
            let (mut state, pages, applied, conflicts) = {
                let mut st_out: Option<CoreResult<DeviceSyncState>> = None;
                access.with_repo(Box::new(|repo| {
                    if repo.device_id().as_bytes() != host_id.as_bytes() {
                        return Err(CoreError::SyncAborted);
                    }
                    st_out = Some(Ok(repo.sync_state(&joiner_id)));
                    Ok(())
                }))?;
                (
                    st_out.take().ok_or(CoreError::SyncAborted)??,
                    vec![Vec::new()],
                    0usize,
                    Vec::<String>::new(),
                )
            };
            let mut seq = state.last_sent_seq + 1;
            paging::send_push_pages(
                &mut stream,
                &keys.sync_key[..],
                &self.session_id,
                &mut seq,
                now,
                &host_id,
                &pages,
                conflicts,
                true,
            )
            .map_err(|e| stage_io(e, "reply_send"))?;
            let _done = paging::recv_done(
                &mut stream,
                &keys.sync_key[..],
                &self.session_id,
                &mut replay,
                now,
            )
            .map_err(|e| stage_io(e, "done_recv"))?;
            // 游标照常落盘：这一帧确实占了一个序号，不记下来的话下一轮会重发同一个
            // seq，对端的重放护栏会直接拒掉它（D-147 的坑，宁记不可漏）。
            state.last_recv_seq = 0;
            state.last_sent_seq = seq;
            access.with_repo(Box::new(|repo| {
                if repo.device_id().as_bytes() != host_id.as_bytes() {
                    return Err(CoreError::SyncAborted);
                }
                repo.save_sync_state(state)?;
                Ok(())
            }))?;
            return Ok(HostSyncResult {
                device_id: joiner_id,
                device_name: joiner_name,
                fingerprint,
                applied,
                conflicts: Vec::new(),
                blobs: 0,
            });
        }
        // 锁内段③的密文暂存类型（O2 三段式）：RepoAccess 闭包外只能持此不透明结果。
        type SectionResult = CoreResult<(
            DeviceSyncState,
            Vec<Vec<crate::sync::protocol::ChangeRecord>>,
            usize,
            Vec<String>,
        )>;
        let mut sec: Option<SectionResult> = None;
        access.with_repo(Box::new(|repo| {
            if repo.device_id().as_bytes() != host_id.as_bytes() {
                // 锁定后换库重开（device_id 变化）→ 本场配对作废，密文暂存丢弃。
                return Err(CoreError::SyncAborted);
            }
            let mut st = repo.sync_state(&joiner_id);
            let rep = merge::apply_changes(repo, &mut st, &push.changes)?;
            // O1 回声消除：对端发来的 live 变更证明对端已持有该 rev → 回发水位推进。
            merge::note_received_changes(repo, &mut st, &push.changes);
            let changes = repo.export_changes(&st);
            let pgs = paging::paginate(changes)?;
            // D-109：发送侧 base 推进——即将回发的版本对端即将持有（本交换内送达；
            // 失败路径 save_sync_state 不执行，base 随会话丢弃）。
            let sent_flat: Vec<crate::sync::protocol::ChangeRecord> =
                pgs.iter().flatten().cloned().collect();
            merge::note_sent_base(repo, &mut st, &sent_flat);
            for page in &pgs {
                merge::note_sent_tombstones(&mut st, page);
                merge::note_sent_changes(&mut st, page);
            }
            sec = Some(Ok((st, pgs, rep.applied, rep.conflicts.clone())));
            Ok(())
        }))?;
        let (mut state, pages, applied, conflicts) = sec.take().ok_or(CoreError::SyncAborted)??;

        // 网络段：6) 回送本端变更页（序号在持久化基准上递增，避免与上一轮重放；D-53：
        // 随报文通告本端 blob 能力；D-56：载荷含墓碑，按对端 base 过滤；O1：live 按发送
        // 水位过滤、超预算自动分页）。
        let mut seq = state.last_sent_seq + 1;
        paging::send_push_pages(
            &mut stream,
            &keys.sync_key[..],
            &self.session_id,
            &mut seq,
            now,
            &host_id,
            &pages,
            conflicts.clone(),
            false,
        )
        .map_err(|e| stage_io(e, "reply_send"))?;

        // 网络段：收 Done。
        let _done = paging::recv_done(
            &mut stream,
            &keys.sync_key[..],
            &self.session_id,
            &mut replay,
            now,
        )
        .map_err(|e| stage_io(e, "done_recv"))?;

        // 网络段：D-53 blob 阶段——双方都支持才进入；seq 从最后一个 reply 页之后单调延续
        // （过 ReplayGuard）。Host 次序：先供 Joiner 拉的（收 Pull、发 Meta/Data/End），再拉
        // Joiner 有的（发 Pull、收 Meta/Data/End）——与 Joiner 侧「先拉后供」严格镜像，
        // 避免半双工交织。注意 send_blob 是「先用后增」：最后一个 reply 页已占用 seq-1，
        // Done 由对端发出不占本端序号，故 blob 报文须从 seq 起。
        // blob 内部按「落盘/读帧」粒度进出锁（access），网络等待全程锁外。
        let mut sent = seq - 1;
        let mut blobs = 0;
        if blob_transfer::blob_phase_enabled(true, push.blob_support) {
            sent = seq;
            blob_transfer::serve_pull(
                &mut stream,
                &keys.sync_key[..],
                &self.session_id,
                &mut replay,
                access,
                &mut sent,
                now,
            )?;
            let phase = blob_transfer::pull_missing(
                &mut stream,
                &keys.sync_key[..],
                &self.session_id,
                &mut replay,
                access,
                &mut sent,
                now,
            )?;
            blobs = phase.appended;
        }

        // ── 锁内段④（O2，毫秒级）：持久化游标。
        state.last_recv_seq = replay.last_recv_seq();
        state.last_sent_seq = sent;
        access.with_repo(Box::new(|repo| {
            if repo.device_id().as_bytes() != host_id.as_bytes() {
                return Err(CoreError::SyncAborted);
            }
            repo.save_sync_state(state)?;
            repo.touch_device_sync(&joiner_id, now)?;
            Ok(())
        }))?;

        Ok(HostSyncResult {
            device_id: joiner_id,
            device_name: joiner_name,
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

// Host 侧一次同步交换的内联实现已并入 [`SyncServer::handle_connection`]（O2 三段式）。

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

/// 由 HELLO 里的**长期**身份公钥算对端设备指纹（【D-70】）。
///
/// 长度不是 32B（空 = 老客户端不发该字段；或畸形输入）一律返回空串：
/// - 空串的语义是「本端不掌握对端长期身份」，UI 据此**不渲染**指纹行；
/// - **绝不回退**到 `joiner_pub`（临时 ECDH 公钥）——那是每次配对新生成的，
///   用它算指纹等于把一个一次性值冒充成设备身份（D-67/D-69）。
fn peer_identity_fingerprint(identity_pub: &[u8]) -> String {
    match <[u8; 32]>::try_from(identity_pub) {
        Ok(key) => handshake::fingerprint_of(&key),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::peer_identity_fingerprint;

    /// 32B 正身 → 16 位小写 hex（与手机端 `HostIdentity::fingerprint` 同算法同长度）。
    #[test]
    fn thirty_two_bytes_yields_hex_fingerprint() {
        let fp = peer_identity_fingerprint(&[9u8; 32]);
        assert_eq!(fp.len(), 16, "指纹是 sha256 前 8 字节 = 16 字符");
        assert!(
            fp.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "必须是小写 hex：{fp}"
        );
        // 同一把公钥恒得同一指纹（跨配对稳定的前提）。
        assert_eq!(fp, peer_identity_fingerprint(&[9u8; 32]));
        // 不同公钥必得不同指纹（否则认不出两台设备）。
        assert_ne!(fp, peer_identity_fingerprint(&[10u8; 32]));
    }

    /// 老客户端不发该字段 → 空 vec → 留空串（UI 不渲染指纹行）。
    #[test]
    fn empty_identity_stays_empty() {
        assert_eq!(peer_identity_fingerprint(&[]), "");
    }

    /// 畸形长度（31 / 33B）→ 留空，不 panic 也不猜。
    #[test]
    fn wrong_length_stays_empty() {
        assert_eq!(peer_identity_fingerprint(&[1u8; 31]), "");
        assert_eq!(peer_identity_fingerprint(&[1u8; 33]), "");
    }
}
