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
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::constants::{DISCOVERY_WAIT_MS, QR_PROTOCOL_VERSION, SYNC_TIME_WINDOW_SECS};
use crate::crypto::keys::{self, ItemKey};
use crate::crypto::rng;
use crate::error::{stage_io, CoreError, CoreResult};
use crate::model::sync_state::DeviceSyncState;
use crate::store::repository::Repository;
use crate::sync::blob_transfer;
use crate::sync::discovery::{self, DiscoveryResponder};
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
    /// v1.2.4（根因 B）：发起方**自己的持久重同步监听端口**，供响应方刷新配对地址。
    ///
    /// 协议向后兼容：老客户端不发该字段 → `#[serde(default)]` 取 0 → 响应方保留原值
    /// （**不得**把 0 写进配对记录，否则老客户端会把可用的 peer_port 清空）。
    #[serde(default)]
    pub sync_port: u16,
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
    /// D-53：blob 阶段实际新入库的附件数（能力协商通过时才有意义）。
    pub blobs: usize,
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

/// 当前运行平台的展示标签（D-66：让设备列表能区分「手机 / 电脑」）。
///
/// 纯编译期常量，不含主机名、用户名等任何隐私信息。
fn platform_label() -> &'static str {
    if cfg!(target_os = "android") {
        "Android"
    } else if cfg!(target_os = "ios") {
        "iOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    }
}

/// 由设备 ID 生成便于识别的展示名（D-66：`<平台>-<8 位短 ID>`，不含任何隐私信息）。
///
/// 旧实现固定 `PasswordMS-<8 位短 ID>`：两台同型号手机短 ID 撞车时（或用户只看到
/// 前缀）在设备列表里完全无法分辨。加入平台前缀后，至少「手机 vs 电脑」一眼可分；
/// 剩余歧义由 `Repository::rename_paired_device` 的手动改名兜底。
///
/// wire 校验**一律不受影响**：本名只作为 `client_name` 之类的展示字段传输，
/// 设备身份始终是 `device_id`（见 `resync_with_peer` 的 `server_id` 校验）。
pub fn device_display_name(device_id: &str) -> String {
    let short: String = device_id.chars().take(8).collect();
    format!("{}-{short}", platform_label())
}

/// 发现目标策略（D-74）。
///
/// 生产走 [`DiscoveryMode::Broadcast`]；测试注入 loopback 单播目标——CI 上真实广播
/// 不可靠（容器无广播路由、防火墙、虚拟网卡），若把广播写死在函数体里，用例就只能
/// 靠"运气"绿。
#[derive(Debug, Clone, Default)]
pub enum DiscoveryMode {
    /// 真实广播：各网卡广播地址 + `255.255.255.255`。
    #[default]
    Broadcast,
    /// 只发这些单播目标（测试 / 诊断）。
    Unicast(Vec<SocketAddr>),
    /// 完全不发现（保持 D-74 之前的行为）。
    Disabled,
}

/// 发起一次直连重同步（客户端侧）。
///
/// 读取已配对设备 → 连接其持久监听端口 → 会话密钥派生 + 双向确认 → `Push/Push/Done`
/// 交换 → 合并（含冲突箱）→ 更新游标 → 返回摘要。
///
/// `local_port`（v1.2.4 根因 B）：**本机**持久重同步监听端口，随 HELLO 通告给对端。
/// 对端据此刷新它记录的「我这台设备怎么连」的地址（IP 由 accept 出的 socket 得到）。
/// 传 0 表示本机暂无监听器 —— 对端会保留其原有记录，不清空。
///
/// D-74：静态地址连接失败时会做一次**按需局域网发现**（见 [`DiscoveryMode`]）。
pub fn resync_with_peer(
    repo: &mut Repository,
    peer_device_id: &str,
    now: i64,
    local_port: u16,
) -> CoreResult<ResyncOutcome> {
    resync_with_peer_opts(
        repo,
        peer_device_id,
        now,
        local_port,
        &DiscoveryMode::Broadcast,
    )
}

/// [`resync_with_peer`] 的可注入发现目标版本（生产入口的薄包装）。
pub fn resync_with_peer_opts(
    repo: &mut Repository,
    peer_device_id: &str,
    now: i64,
    local_port: u16,
    discovery_mode: &DiscoveryMode,
) -> CoreResult<ResyncOutcome> {
    // 1) 读取已配对设备。
    let peer = repo
        .get_paired_device(peer_device_id)
        .ok_or(CoreError::NotPaired)?;

    // 2) 解出共享同步密钥（仅解锁态可解；AAD 绑定设备身份）。
    //    D-74：提前到连接之前——发现报文的 tag 也要用它算，且**只解这一次**。
    let sync_key = keys::unwrap_item_key(
        repo.vault_key(),
        &peer.sync_key_wrapped,
        &device_key_aad(peer_device_id),
    )?;

    // 3) 连接对端持久监听端口。
    // D-57：改用与配对一致的短超时（裸 connect 对不可路由地址挂 ~21s+，用户干等），
    // 并以 stage_io 标注 connect 阶段——message_key 对该阶段统一映射
    // `err.network_unreachable`（网络不通/不在同一网段），不再误报「文件读写出错」。
    //
    // D-74：这是死锁的突破口——换网络后静态地址**必然**失败，而地址自愈只发生在
    // 「连接已成功」的接收方。故失败后先做一次按需发现，找到新地址再连一次。
    //
    // **不新增错误码**（规格 §8 已撤销）：所有失败出口都复用 D-57 建立的
    // `stage_io(e, "connect")` → `err.network_unreachable` 映射（该文案本身已含
    // "仍失败请重新配对"）。多一个变体只会重复造文案，还要动 `code()`/`message_key()`
    // 两个穷举 match 与前端 i18n。
    let connect_timeout = Duration::from_secs(crate::constants::SYNC_CONNECT_TIMEOUT_SECS);
    let static_addr = match (&peer.peer_ip, peer.peer_port) {
        (Some(ip), Some(port)) => Some((ip.clone(), port)),
        _ => None,
    };
    let used_addr: (String, u16);
    let mut stream = match static_addr {
        Some((ip, port)) => match transport::connect_with_timeout(&ip, port, connect_timeout) {
            Ok(s) => {
                used_addr = (ip, port);
                s
            }
            Err(first_err) => {
                // 记下**静态那次**的错误（已按 connect 阶段标注），发现无果时原样返回。
                let first_err = stage_io(first_err, "connect");
                match discover_addr(&sync_key, discovery_mode, local_port)? {
                    Some((ip, port)) => {
                        let s = transport::connect_with_timeout(&ip, port, connect_timeout)
                            .map_err(|e| stage_io(e, "connect"))?;
                        used_addr = (ip, port);
                        s
                    }
                    None => return Err(first_err),
                }
            }
        },
        // 地址缺失（配对记录不完整，历史上不该发生）：走同一个语义出口——合成一条
        // "没有地址"的 IO 错误并按 connect 阶段标注，落到现成的 `err.network_unreachable`。
        None => match discover_addr(&sync_key, discovery_mode, local_port)? {
            Some((ip, port)) => {
                let s = transport::connect_with_timeout(&ip, port, connect_timeout)
                    .map_err(|e| stage_io(e, "connect"))?;
                used_addr = (ip, port);
                s
            }
            None => {
                let err = CoreError::from(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no peer address",
                ));
                return Err(stage_io(err, "connect"));
            }
        },
    };
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
        sync_port: local_port,
    });
    transport::send_frame(&mut stream, &serde_json::to_vec(&hello)?)
        .map_err(|e| stage_io(e, "hello_send"))?;

    // 5) CHALLENGE。
    let challenge_bytes =
        transport::recv_frame(&mut stream).map_err(|e| stage_io(e, "challenge_recv"))?;
    let challenge = match serde_json::from_slice::<ResyncMessage>(&challenge_bytes)? {
        ResyncMessage::Challenge(c) => c,
        ResyncMessage::Reject(r) => return Err(reject_to_error(&r.code)),
        _ => return Err(CoreError::FingerprintMismatch),
    };
    if challenge.server_id != peer_device_id || challenge.nonce_s.len() != 32 {
        return Err(CoreError::FingerprintMismatch);
    }

    // 6) 派生会话密钥 + 校验响应方确认。
    let transcript = transcript_of(
        &client_id,
        &challenge.server_id,
        &nonce_c,
        &challenge.nonce_s,
    );
    let sk = handshake::derive_session_sk(&sync_key, &nonce_c, &challenge.nonce_s)?;
    let expected_s = handshake::confirm_mac(&sk, ROLE_SERVER, &transcript)?;
    if !handshake::verify_mac(&expected_s, &challenge.mac) {
        return Err(CoreError::FingerprintMismatch);
    }

    // 7) 回送本端确认。
    let mac_c = handshake::confirm_mac(&sk, ROLE_CLIENT, &transcript)?;
    let confirm = ResyncMessage::Confirm(ResyncConfirm { mac: mac_c });
    transport::send_frame(&mut stream, &serde_json::to_vec(&confirm)?)
        .map_err(|e| stage_io(e, "confirm_send"))?;

    let session_id = session_id_of(&transcript);
    let mut state: DeviceSyncState = repo.sync_state(peer_device_id);
    let mut replay = ReplayGuard::with_state(state.last_recv_seq, SYNC_TIME_WINDOW_SECS);
    let c1 = state.last_sent_seq + 1;
    let c2 = c1 + 1;

    // 8) 发本端 Push（D-53：随报文通告本端 blob 能力；D-56：载荷含墓碑，按对端 base 过滤）。
    let push = SyncPayload {
        kind: SyncKind::Push,
        device_id: client_id.clone(),
        cursor: 0,
        changes: repo.export_changes(&state),
        conflicts: Vec::new(),
        blob_support: true,
    };
    let push_env = seal_envelope(&sk[..], &session_id, c1, now, &push)?;
    transport::send_frame(&mut stream, &serde_json::to_vec(&push_env)?)
        .map_err(|e| stage_io(e, "push_send"))?;
    // D-56：发送成功后推进已导出墓碑的 base（下一轮免重发；失败路径不落盘自动重发）。
    merge::note_sent_tombstones(&mut state, &push.changes);

    // 9) 收对端 Push 并合并。
    let reply_bytes = transport::recv_frame(&mut stream).map_err(|e| stage_io(e, "reply_recv"))?;
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
        blob_support: true,
    };
    let done_env = seal_envelope(&sk[..], &session_id, c2, now, &done)?;
    transport::send_frame(&mut stream, &serde_json::to_vec(&done_env)?)
        .map_err(|e| stage_io(e, "done_send"))?;

    // D-53 blob 阶段：双方都支持才进入；seq 从 Done 之后单调延续（过 ReplayGuard）。
    // 发起方次序：先拉后供（与响应方镜像）。
    // 注意 send_blob 是「先用后增」：Done 已占用 c2，故 blob 报文须从 c2+1 起。
    let mut sent = c2;
    let mut blobs = 0;
    if blob_transfer::blob_phase_enabled(true, reply.blob_support) {
        sent = c2 + 1;
        let phase = blob_transfer::pull_missing(
            &mut stream,
            &sk[..],
            &session_id,
            &mut replay,
            repo,
            &mut sent,
            now,
        )?;
        blobs = phase.appended;
        blob_transfer::serve_pull(
            &mut stream,
            &sk[..],
            &session_id,
            &mut replay,
            repo,
            &mut sent,
            now,
        )?;
    }

    // 11) 持久化游标。
    state.last_recv_seq = replay.last_recv_seq();
    state.last_sent_seq = sent;
    repo.save_sync_state(state)?;
    repo.touch_device_sync(peer_device_id, now)?;

    // D-74：靠发现连上后**必须把新地址写回**，否则下次同步还得再发现一次。
    // 与入站自愈（`ResyncServer::handle` 末尾）同一规则——只在**校验通过且同步成功后**写，
    // 任何失败路径都不得篡改配对记录（防投毒）。
    let addr_changed = peer.peer_ip.as_deref() != Some(used_addr.0.as_str())
        || peer.peer_port != Some(used_addr.1);
    if addr_changed {
        repo.update_paired_addr(peer_device_id, &used_addr.0, used_addr.1)?;
    }

    Ok(ResyncOutcome {
        applied: report.applied,
        conflicts: report.conflicts,
        blobs,
    })
}

/// 做一次按需发现（D-74），把 [`DiscoveryMode`] 翻译成具体的 socket 行为。
///
/// `local_port` 用于识别"自己应答自己"（见 [`discovery::discover_peer`]）。
fn discover_addr(
    sync_key: &ItemKey,
    mode: &DiscoveryMode,
    local_port: u16,
) -> CoreResult<Option<(String, u16)>> {
    let self_port = if local_port == 0 {
        None
    } else {
        Some(local_port)
    };
    let wait = Duration::from_millis(DISCOVERY_WAIT_MS);
    match mode {
        DiscoveryMode::Disabled => Ok(None),
        DiscoveryMode::Broadcast => discovery::discover_peer(sync_key, wait, self_port),
        DiscoveryMode::Unicast(targets) => {
            discovery::discover_peer_with(sync_key, targets, wait, self_port)
        }
    }
}

/// 持久重同步监听器（响应方侧）。
///
/// 在保险库解锁期间持续运行，非阻塞轮询接受已配对设备的直连重同步请求。
pub struct ResyncServer {
    listener: TcpListener,
    port: u16,
    local_name: String,
    /// D-74：UDP 发现应答器。bind 失败（端口被占 / 双开 / 权限）时为 `None`，
    /// **仅降级不做发现应答**——绝不能让"发现功能"把整个重同步服务拖死。
    discovery: Option<DiscoveryResponder>,
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
            discovery: DiscoveryResponder::bind().ok(),
        })
    }

    /// 实际监听端口（供配对时通告）。
    pub fn port(&self) -> u16 {
        self.port
    }

    /// D-74：本进程是否成功启用了 UDP 发现应答（供上层记录一条日志）。
    ///
    /// 核心层没有日志设施，`bind` 失败的事实通过本方法暴露给 `src-tauri` 侧落盘。
    pub fn discovery_enabled(&self) -> bool {
        self.discovery.is_some()
    }

    /// D-74：替换发现应答 socket（**测试专用**：注入任意端口的 loopback socket）。
    ///
    /// 生产路径永远走 [`ResyncServer::start`] 里的固定端口绑定；这个方法存在只是为了让
    /// 用例不必抢 `47823`（并行跑会互相冲突）。
    pub fn attach_discovery_socket(&mut self, socket: UdpSocket) {
        self.discovery = Some(DiscoveryResponder::from_socket(socket));
    }

    /// D-74：非阻塞轮询一次发现请求，返回本次应答数。
    ///
    /// **任何失败都不上抛**：调用点（200ms 轮询线程）用它做"顺手应答"，发现出问题
    /// 绝不能让重同步循环 `break`。
    pub fn poll_discovery_once(&mut self, repo: &Repository) -> usize {
        let port = self.port;
        match self.discovery.as_mut() {
            Some(responder) => responder.poll_once(repo, port),
            None => 0,
        }
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
            Ok((stream, peer)) => {
                // D-46：Windows 上 accept 出的 socket 继承 listener 的非阻塞模式
                //（Unix 不会），必须显式改回阻塞，否则 recv 立即 WouldBlock。
                stream
                    .set_nonblocking(false)
                    .map_err(|_| CoreError::SyncAborted)?;
                // v1.2.4（根因 B）：对端 IP 取自 accept 出的 socket——这是**本次连接的真实来源**，
                // 比配对时记录的陈旧地址可靠。以前这里丢弃了 `_peer`，导致地址永不自愈。
                let peer_ip = peer.ip().to_string();
                Ok(Some(self.handle(stream, &peer_ip, repo, now)?))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(_) => Err(CoreError::SyncAborted),
        }
    }

    /// 处理一个入站重同步连接。
    fn handle(
        &mut self,
        mut stream: TcpStream,
        peer_ip: &str,
        repo: &mut Repository,
        now: i64,
    ) -> CoreResult<ResyncOutcome> {
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|_| CoreError::SyncAborted)?;

        // 1) HELLO。
        let hello_bytes =
            transport::recv_frame(&mut stream).map_err(|e| stage_io(e, "hello_recv"))?;
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
        transport::send_frame(&mut stream, &serde_json::to_vec(&challenge)?)
            .map_err(|e| stage_io(e, "challenge_send"))?;

        // 5) 校验发起方确认。
        let confirm_bytes =
            transport::recv_frame(&mut stream).map_err(|e| stage_io(e, "confirm_recv"))?;
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
        let push_bytes =
            transport::recv_frame(&mut stream).map_err(|e| stage_io(e, "push_recv"))?;
        let push_env: SyncEnvelope = serde_json::from_slice(&push_bytes)?;
        replay.accept(push_env.seq, push_env.ts, now)?;
        let push = open_envelope(&sk[..], &session_id, &push_env)?;
        let report = merge::apply_changes(repo, &mut state, &push.changes)?;

        // 7) 回送本端 Push（D-53：随报文通告本端 blob 能力；D-56：载荷含墓碑，按对端 base 过滤）。
        let s1 = state.last_sent_seq + 1;
        let reply = SyncPayload {
            kind: SyncKind::Push,
            device_id: server_id.clone(),
            cursor: 0,
            changes: repo.export_changes(&state),
            conflicts: report.conflicts.clone(),
            blob_support: true,
        };
        let reply_env = seal_envelope(&sk[..], &session_id, s1, now, &reply)?;
        transport::send_frame(&mut stream, &serde_json::to_vec(&reply_env)?)
            .map_err(|e| stage_io(e, "reply_send"))?;
        // D-56：发送成功后推进已导出墓碑的 base（下一轮免重发）。
        merge::note_sent_tombstones(&mut state, &reply.changes);

        // 8) 收 Done。
        let done_bytes =
            transport::recv_frame(&mut stream).map_err(|e| stage_io(e, "done_recv"))?;
        let done_env: SyncEnvelope = serde_json::from_slice(&done_bytes)?;
        replay.accept(done_env.seq, done_env.ts, now)?;
        let done_payload = open_envelope(&sk[..], &session_id, &done_env)?;

        // D-53 blob 阶段：双方都支持才进入；seq 从 Done 之后单调延续（过 ReplayGuard）。
        // 响应方次序：先供（收发起方 Pull、发 Meta/Data/End），再拉（发 Pull、收 Meta/Data/End）——
        // 与发起方「先拉后供」严格镜像。
        // 注意 send_blob 是「先用后增」：reply Push 已占用 s1，故 blob 报文须从 s1+1 起。
        let mut sent = s1;
        let mut blobs = 0;
        if blob_transfer::blob_phase_enabled(true, done_payload.blob_support) {
            sent = s1 + 1;
            blob_transfer::serve_pull(
                &mut stream,
                &sk[..],
                &session_id,
                &mut replay,
                repo,
                &mut sent,
                now,
            )?;
            let phase = blob_transfer::pull_missing(
                &mut stream,
                &sk[..],
                &session_id,
                &mut replay,
                repo,
                &mut sent,
                now,
            )?;
            blobs = phase.appended;
        }

        // 9) 持久化游标。
        state.last_recv_seq = replay.last_recv_seq();
        state.last_sent_seq = sent;
        repo.save_sync_state(state)?;
        // v1.2.4（根因 B）：**同步成功后刷新对端地址**。
        // 以前只在配对那一刻记录一次 peer_ip/port，手机换 IP（DHCP）或监听端口 rebind
        // （resync.port 被占用后 fallback）之后，本机重同步永远连不上、只能重新配对。
        // 现在每次成功入站都把「这次真实来源 IP + 对端自报的监听端口」写回配对记录。
        // 只在**校验通过且同步成功**之后调用：失败/被拒的连接不得篡改地址（防投毒）。
        repo.update_paired_addr(&hello.client_id, peer_ip, hello.sync_port)?;
        repo.touch_device_sync(&hello.client_id, now)?;

        Ok(ResyncOutcome {
            applied: report.applied,
            conflicts: report.conflicts,
            blobs,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// D-66：默认展示名 = `<当前平台>-<8 位短 ID>`。
    #[test]
    fn device_display_name_uses_platform_prefix() {
        let id = "3dd4390a-1111-2222-3333-444455556666";
        let name = device_display_name(id);
        assert_eq!(
            name,
            format!("{}-3dd4390a", platform_label()),
            "默认名必须带当前平台前缀"
        );
        assert!(name.starts_with(platform_label()));
        assert!(!name.contains("PasswordMS"), "旧固定前缀不得残留");
    }

    /// 短 ID 恰为 8 位（不足 8 位时按实际长度截断，不补位）。
    #[test]
    fn device_display_name_short_id_is_eight_chars() {
        let name = device_display_name("abcdef0123456789");
        let short = name.rsplit('-').next().unwrap();
        assert_eq!(short, "abcdef01");
        assert_eq!(short.chars().count(), 8);

        // 极短 ID：不 panic、不补位。
        assert_eq!(
            device_display_name("ab"),
            format!("{}-ab", platform_label())
        );
        // 空 ID 也安全。
        assert_eq!(device_display_name(""), format!("{}-", platform_label()));
    }

    /// 平台标签取值封闭（避免以后误写出自由文本）。
    #[test]
    fn platform_label_is_one_of_known_values() {
        assert!(
            ["Android", "iOS", "Windows", "macOS", "Linux"].contains(&platform_label()),
            "未知平台标签: {}",
            platform_label()
        );
        // 本仓库只在 Windows / Android 上出包：桌面 Windows 构建必须命中 Windows。
        #[cfg(target_os = "windows")]
        assert_eq!(platform_label(), "Windows");
        #[cfg(target_os = "android")]
        assert_eq!(platform_label(), "Android");
    }
}
