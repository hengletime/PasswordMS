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

use crate::constants::{
    is_sync_port, DISCOVERY_WAIT_MS, QR_PROTOCOL_VERSION, SYNC_PORT_MAX, SYNC_PORT_MIN,
    SYNC_TIME_WINDOW_SECS,
};
use crate::crypto::keys::{self, ItemKey};
use crate::crypto::rng;
use crate::error::{stage_io, CoreError, CoreResult};
use crate::model::sync_state::DeviceSyncState;
use crate::store::repository::Repository;
use crate::sync::access::DirectAccess;
use crate::sync::blob_transfer;
use crate::sync::discovery::{self, DiscoveryResponder};
use crate::sync::handshake::{self, ROLE_CLIENT, ROLE_SERVER};
use crate::sync::merge;
use crate::sync::paging;
use crate::sync::protocol::{seal_envelope, ChangeRecord, Reject, SyncKind, SyncPayload};
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

/// [`resync_with_peer`] 的可注入发现目标版本（兼容入口：repo 已由调用方持锁 / 既有测试）。
pub fn resync_with_peer_opts(
    repo: &mut Repository,
    peer_device_id: &str,
    now: i64,
    local_port: u16,
    discovery_mode: &DiscoveryMode,
) -> CoreResult<ResyncOutcome> {
    let mut access = DirectAccess(repo);
    resync_with_peer_access(&mut access, peer_device_id, now, local_port, discovery_mode)
}

/// 锁内段①的产出（全部可安全带出锁外：密文载荷 + 密钥封装 + 对端地址元信息）。
struct PreparedResync {
    peer_ip: Option<String>,
    peer_port: Option<u16>,
    sync_key: ItemKey,
    client_id: String,
    state: DeviceSyncState,
    pages: Vec<Vec<ChangeRecord>>,
}

/// [`resync_with_peer`] 的三段式版本（**O2**：网络等待全程不持会话锁）。
///
/// - 锁内段①（毫秒级）：配对信息 + 解 `sync_key` + 状态初值 + 导出待发页（密文）；
/// - 网络段：连接（含按需发现）、握手、发 Push 页、收对端 Push 页；
/// - 锁内段②（毫秒级）：vault 身份校验 → 合并收到的变更（含回声水位推进）；
/// - 网络段：发 Done、blob 阶段（按批进出锁）；
/// - 锁内段③（毫秒级）：持久化游标 + 地址自愈。
///
/// 中途会话被锁（自动锁定竞态）⇒ `access` 返回 `Err(NotUnlocked)` ⇒ 交换整体放弃，
/// 游标不落盘、下一轮自动重发（宁多勿丢；密文暂存随连接丢弃，无明文残留）。
pub fn resync_with_peer_access(
    access: &mut dyn crate::sync::access::RepoAccess,
    peer_device_id: &str,
    now: i64,
    local_port: u16,
    discovery_mode: &DiscoveryMode,
) -> CoreResult<ResyncOutcome> {
    // ── 锁内段①（O2，毫秒级）：配对信息 + 解密钥 + 状态初值 + 导出待发页。
    // 锁定态到达这里 → with_repo 返回 Err → 交换整体放弃。
    let mut prepared: Option<CoreResult<PreparedResync>> = None;
    access.with_repo(Box::new(|repo| {
        // 1) 读取已配对设备。
        let Some(peer) = repo.get_paired_device(peer_device_id) else {
            prepared = Some(Err(CoreError::NotPaired));
            return Ok(());
        };
        // 2) 解出共享同步密钥（仅解锁态可解；AAD 绑定设备身份）。
        //    D-74：提前到连接之前——发现报文的 tag 也要用它算，且**只解这一次**。
        match keys::unwrap_item_key(
            repo.vault_key(),
            &peer.sync_key_wrapped,
            &device_key_aad(peer_device_id),
        ) {
            Ok(sk) => {
                let st = repo.sync_state(peer_device_id);
                match paging::paginate(repo.export_changes(&st)) {
                    Ok(pages) => {
                        prepared = Some(Ok(PreparedResync {
                            peer_ip: peer.peer_ip.clone(),
                            peer_port: peer.peer_port,
                            sync_key: sk,
                            client_id: repo.device_id().to_string(),
                            state: st,
                            pages,
                        }));
                    }
                    Err(e) => prepared = Some(Err(e)),
                }
            }
            Err(_) => prepared = Some(Err(CoreError::FingerprintMismatch)),
        }
        Ok(())
    }))?;
    let prep = prepared.take().ok_or(CoreError::SyncAborted)??;
    let mut state = prep.state;

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
    // D-136 诊断：配对记录里的端口落在**本程序选取区间**外时打一条轨迹。
    // 这里**只标注、不改行为**——曾试过「区间外就跳过静态连接直接进发现」，被既有
    // 测试当场拦下：本机动态端口范围被 WSL2/Hyper-V 改成 1024-65535，测试里
    // `bind(0)` 拿到的 29581 完全可连，却会被那条判定当成非法地址丢掉。
    // 「协议从哪个区间选端口」不等于「对端的端口必须落在该区间」，拿它做连接
    // 决策是假阴性，比原问题更糟。
    // 【2026-10-30 措辞更正】「多半是历史残留」是错的：Windows 临时端口默认 49152
    // 起，但 **Android（Linux 内核）默认是 32768-60999**——区间外的端口完全可以是
    // 对端真实工作过的端口（主人实测 46879 配对成功过）。端口每次启动重选，记录
    // 是否过期无法从数值判定，所以只记录事实、不下结论。
    if let (Some(ip), Some(port)) = (&prep.peer_ip, prep.peer_port) {
        if !is_sync_port(port) {
            discovery::trace(format!(
                "peer: 记录地址 {ip}:{port} 端口不在本程序选取区间 {SYNC_PORT_MIN}-{SYNC_PORT_MAX}（Windows 临时端口 49152 起、Android 32768 起，此值可能是对端真实端口，仅记录不判定）"
            ));
        }
    }
    let static_addr = match (&prep.peer_ip, prep.peer_port) {
        (Some(ip), Some(port)) => Some((ip.clone(), port)),
        _ => None,
    };
    // D-132 诊断：配对记录里的地址是排查第一现场——「连的是哪个 IP:端口」以前只存在于
    // 内存里，日志一个字都没有，真机出问题时只能靠猜。
    match &static_addr {
        Some((ip, port)) => discovery::trace(format!("peer: 配对记录地址 {ip}:{port}")),
        None => discovery::trace("peer: 配对记录无地址，直接走发现"),
    }
    let used_addr: (String, u16);
    let mut stream = match static_addr {
        Some((ip, port)) => match transport::connect_with_timeout(&ip, port, connect_timeout) {
            Ok(s) => {
                discovery::trace(format!("connect: 静态地址 {ip}:{port} 直连成功"));
                used_addr = (ip, port);
                s
            }
            Err(first_err) => {
                // 记下**静态那次**的错误（已按 connect 阶段标注），发现无果时原样返回。
                let first_err = stage_io(first_err, "connect");
                discovery::trace(format!(
                    "connect: 静态地址 {ip}:{port} 失败（{connect_timeout:?} 超时）→ 尝试局域网发现"
                ));
                match discover_addr(&prep.sync_key, discovery_mode, local_port)? {
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
        None => match discover_addr(&prep.sync_key, discovery_mode, local_port)? {
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

    // 4) HELLO（网络段，不持锁）。
    let client_id = prep.client_id.clone();
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
    let sk = handshake::derive_session_sk(&prep.sync_key, &nonce_c, &challenge.nonce_s)?;
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
    // 状态沿用锁内段①载入的副本（含本次发送侧水位推进的落点）；sync_busy 保证本场
    // 交换期间没有并发同步改写同一份 DeviceSyncState。
    let mut replay = ReplayGuard::with_state(state.last_recv_seq, SYNC_TIME_WINDOW_SECS);

    // 8) 发本端 Push（网络段，不持锁；页已在锁内段①导出。D-53：随报文通告本端 blob 能力；
    // D-56：载荷含墓碑，按对端 base 过滤；O1：live 按发送水位过滤、超预算自动分页）。
    let mut seq = state.last_sent_seq + 1;
    paging::send_push_pages(
        &mut stream,
        &sk[..],
        &session_id,
        &mut seq,
        now,
        &client_id,
        &prep.pages,
        Vec::new(),
    )
    .map_err(|e| stage_io(e, "push_send"))?;
    // D-56/O1：发送成功后推进已导出墓碑的 base 与 live 水位（下一轮免重发；失败路径
    // 不落盘自动重发）。
    for page in &prep.pages {
        merge::note_sent_tombstones(&mut state, page);
        merge::note_sent_changes(&mut state, page);
    }

    // 9) 收对端 Push（网络段，不持锁；O1：多页聚合，密文载荷）。
    let (reply_changes, reply_blob_support) =
        paging::recv_push_pages(&mut stream, &sk[..], &session_id, &mut replay, now)
            .map_err(|e| stage_io(e, "reply_recv"))?;

    // ── 锁内段②（O2，毫秒级）：vault 身份校验 → 合并收到的变更。
    let sent_flat: Vec<crate::sync::protocol::ChangeRecord> =
        prep.pages.iter().flatten().cloned().collect();
    let mut merged: Option<CoreResult<(usize, Vec<String>)>> = None;
    access.with_repo(Box::new(|repo| {
        if repo.device_id().as_bytes() != client_id.as_bytes() {
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
    let (applied, conflicts) = merged.take().ok_or(CoreError::SyncAborted)??;

    // 10) 发 Done（网络段，不持锁；seq 延续分页序号）。
    let done = SyncPayload {
        kind: SyncKind::Done,
        device_id: client_id.clone(),
        cursor: 0,
        changes: Vec::new(),
        conflicts: conflicts.clone(),
        blob_support: true,
    };
    let done_env = seal_envelope(&sk[..], &session_id, seq, now, &done)?;
    transport::send_frame(&mut stream, &serde_json::to_vec(&done_env)?)
        .map_err(|e| stage_io(e, "done_send"))?;

    // D-53 blob 阶段（网络段 + 锁内段按批进出）：双方都支持才进入；seq 从 Done 之后
    // 单调延续（过 ReplayGuard）。发起方次序：先拉后供（与响应方镜像）。
    // 注意 send_blob 是「先用后增」：Done 已占用 seq，故 blob 报文须从 seq+1 起。
    let mut sent = seq;
    let mut blobs = 0;
    if blob_transfer::blob_phase_enabled(true, reply_blob_support) {
        sent = seq + 1;
        let phase = blob_transfer::pull_missing(
            &mut stream,
            &sk[..],
            &session_id,
            &mut replay,
            access,
            &mut sent,
            now,
        )?;
        blobs = phase.appended;
        blob_transfer::serve_pull(
            &mut stream,
            &sk[..],
            &session_id,
            &mut replay,
            access,
            &mut sent,
            now,
        )?;
    }

    // ── 锁内段③（O2，毫秒级）：持久化游标 + D-74 地址自愈。
    state.last_recv_seq = replay.last_recv_seq();
    state.last_sent_seq = sent;
    // D-74：靠发现连上后**必须把新地址写回**，否则下次同步还得再发现一次。
    // 与入站自愈（`ResyncServer::handle_connection` 末尾）同一规则——只在**校验通过且
    // 同步成功后**写，任何失败路径都不得篡改配对记录（防投毒）。
    let addr_changed = prep.peer_ip.as_deref() != Some(used_addr.0.as_str())
        || prep.peer_port != Some(used_addr.1);
    let final_state = state;
    access.with_repo(Box::new(|repo| {
        if repo.device_id().as_bytes() != client_id.as_bytes() {
            return Err(CoreError::SyncAborted);
        }
        repo.save_sync_state(final_state)?;
        repo.touch_device_sync(peer_device_id, now)?;
        if addr_changed {
            repo.update_paired_addr(peer_device_id, &used_addr.0, used_addr.1)?;
        }
        Ok(())
    }))?;

    Ok(ResyncOutcome {
        applied,
        conflicts,
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
    let mode_name = match mode {
        DiscoveryMode::Disabled => "disabled",
        DiscoveryMode::Broadcast => "broadcast",
        DiscoveryMode::Unicast(_) => "unicast",
    };
    discovery::trace(format!("discovery: mode={mode_name} 开始查找对端"));
    let out = match mode {
        DiscoveryMode::Disabled => Ok(None),
        DiscoveryMode::Broadcast => discovery::discover_peer(sync_key, wait, self_port),
        DiscoveryMode::Unicast(targets) => {
            discovery::discover_peer_with(sync_key, targets, wait, self_port)
        }
    };
    match &out {
        Ok(Some((ip, port))) => {
            discovery::trace(format!("discovery: 找到对端 {ip}:{port}"));
        }
        Ok(None) => discovery::trace("discovery: 未找到对端（无任何应答）"),
        Err(e) => discovery::trace(format!("discovery: 出错 {e:?}")),
    }
    out
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

    /// 非阻塞轮询一次入站连接（**不持任何会话锁**，O2）；无连接返回 `Ok(None)`。
    pub fn poll_accept(&mut self) -> CoreResult<Option<(TcpStream, String)>> {
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
                Ok(Some((stream, peer.ip().to_string())))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(_) => Err(CoreError::SyncAborted),
        }
    }

    /// 兼容入口（主动路径 / 既有测试）：repo 已由调用方持锁，直连处理一次入站连接。
    pub fn poll_once(
        &mut self,
        repo: &mut Repository,
        now: i64,
    ) -> CoreResult<Option<ResyncOutcome>> {
        let Some((stream, peer_ip)) = self.poll_accept()? else {
            return Ok(None);
        };
        let mut access = crate::sync::access::DirectAccess(repo);
        self.handle_connection(stream, &peer_ip, now, &mut access)
            .map(Some)
    }

    /// 处理一个入站重同步连接（**O2 三段式**：网络等待全程不持会话锁）。
    ///
    /// - 锁内段①（毫秒级）：配对校验 + 解 `sync_key` + 载入对端同步状态初值；
    /// - 网络段：HELLO/CHALLENGE/CONFIRM 握手、收发起方 Push（多页聚合）；
    /// - 锁内段②（毫秒级）：校验 vault 未变 → 合并收到的变更 → 导出回包页（含水位推进）；
    /// - 网络段：回送 Push 页、收 Done、blob 阶段（按批进出锁）；
    /// - 锁内段③（毫秒级）：持久化游标 + 地址自愈。
    ///
    /// 中途会话被锁（自动锁定竞态）⇒ `access` 返回 `Err(NotUnlocked)` ⇒ 整段丢弃，
    /// 对端下轮重发（宁多勿丢；密文暂存随连接丢弃，无明文残留）。
    pub fn handle_connection(
        &mut self,
        mut stream: TcpStream,
        peer_ip: &str,
        now: i64,
        access: &mut dyn crate::sync::access::RepoAccess,
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

        if hello.nonce_c.len() != 32 {
            return Err(CoreError::FingerprintMismatch);
        }

        // ── 锁内段①（O2，毫秒级）：必须是已配对设备 + 解出共享同步密钥 + 状态初值。
        // 锁定态到达这里（自动锁定竞态）→ with_repo 返回 Err → 交换整体放弃。
        let mut reject: Option<(&'static str, CoreError)> = None;
        let mut resolved: Option<(ItemKey, String, u64)> = None; // (sync_key, server_id, last_recv_seq)
        access.with_repo(Box::new(|repo| {
            let Some(peer) = repo.get_paired_device(&hello.client_id) else {
                reject = Some((crate::error::codes::ERR_NOT_PAIRED, CoreError::NotPaired));
                return Ok(());
            };
            match keys::unwrap_item_key(
                repo.vault_key(),
                &peer.sync_key_wrapped,
                &device_key_aad(&hello.client_id),
            ) {
                Ok(sk) => {
                    let st = repo.sync_state(&hello.client_id);
                    resolved = Some((sk, repo.device_id().to_string(), st.last_recv_seq));
                    Ok(())
                }
                Err(_) => {
                    reject = Some((
                        crate::error::codes::ERR_FINGERPRINT_MISMATCH,
                        CoreError::FingerprintMismatch,
                    ));
                    Ok(())
                }
            }
        }))?;
        let (sync_key, server_id, last_recv_seq) = match resolved {
            Some(v) => v,
            None => {
                let (code, err) = reject.take().ok_or(CoreError::SyncAborted)?;
                send_reject(&mut stream, code)?;
                return Err(err);
            }
        };
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
        let mut replay = ReplayGuard::with_state(last_recv_seq, SYNC_TIME_WINDOW_SECS);

        // 6) 收发起方 Push（O1：多页聚合后一次合并——页面间记录互不重叠，等价单帧）。
        let (push_changes, push_blob_support) =
            paging::recv_push_pages(&mut stream, &sk[..], &session_id, &mut replay, now)
                .map_err(|e| stage_io(e, "push_recv"))?;

        // ── 锁内段②（O2，毫秒级）：vault 未变校验 → 合并 → 导出回包页。
        // 水位/墓碑推进在此一并做：发送失败路径 `save_sync_state` 不执行、随会话丢弃，
        // 失败安全语义不变（宁多勿丢）。
        // 锁内段②的密文暂存类型（O2 三段式）：RepoAccess 闭包外只能持此不透明结果。
        type SectionResult =
            CoreResult<(DeviceSyncState, Vec<Vec<ChangeRecord>>, usize, Vec<String>)>;
        let mut section: Option<SectionResult> = None;
        access.with_repo(Box::new(|repo| {
            if repo.device_id() != server_id {
                // 锁定后换库重开（device_id 变化）→ 本场交换作废，密文暂存丢弃。
                return Err(CoreError::SyncAborted);
            }
            let mut st = repo.sync_state(&hello.client_id);
            let rep = merge::apply_changes(repo, &mut st, &push_changes)?;
            // O1 回声消除：对端发来的 live 变更证明对端已持有该 rev → 回发水位推进。
            merge::note_received_changes(repo, &mut st, &push_changes);
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
            section = Some(Ok((st, pgs, rep.applied, rep.conflicts.clone())));
            Ok(())
        }))?;
        let (mut state, pages, applied, conflicts) =
            section.take().ok_or(CoreError::SyncAborted)??;
        let mut seq = state.last_sent_seq + 1;

        // 7) 回送本端 Push（D-53：随报文通告本端 blob 能力；D-56：载荷含墓碑，按对端 base
        // 过滤；O1：live 按发送水位过滤、超预算自动分页）。网络段不持锁。
        paging::send_push_pages(
            &mut stream,
            &sk[..],
            &session_id,
            &mut seq,
            now,
            &server_id,
            &pages,
            conflicts.clone(),
        )
        .map_err(|e| stage_io(e, "reply_send"))?;

        // 8) 收 Done。
        let done_payload = paging::recv_done(&mut stream, &sk[..], &session_id, &mut replay, now)
            .map_err(|e| stage_io(e, "done_recv"))?;

        // D-53 blob 阶段：双方都支持才进入；seq 从 Done 之后单调延续（过 ReplayGuard）。
        // 响应方次序：先供（收发起方 Pull、发 Meta/Data/End），再拉（发 Pull、收 Meta/Data/End）——
        // 与发起方「先拉后供」严格镜像。
        // 注意 send_blob 是「先用后增」：最后一个 reply 页已占用 seq-1，Done 由对端发出
        // 不占本端序号，故 blob 报文须从 seq 起。
        let mut sent = seq - 1;
        let mut blobs = 0;
        if blob_transfer::blob_phase_enabled(true, push_blob_support.min(done_payload.blob_support))
        {
            sent = seq;
            blob_transfer::serve_pull(
                &mut stream,
                &sk[..],
                &session_id,
                &mut replay,
                access,
                &mut sent,
                now,
            )?;
            let phase = blob_transfer::pull_missing(
                &mut stream,
                &sk[..],
                &session_id,
                &mut replay,
                access,
                &mut sent,
                now,
            )?;
            blobs = phase.appended;
        }

        // ── 锁内段③（O2，毫秒级）：持久化游标 + 地址自愈。
        // v1.2.4（根因 B）：**同步成功后刷新对端地址**。
        // 以前只在配对那一刻记录一次 peer_ip/port，手机换 IP（DHCP）或监听端口 rebind
        // （resync.port 被占用后 fallback）之后，本机重同步永远连不上、只能重新配对。
        // 现在每次成功入站都把「这次真实来源 IP + 对端自报的监听端口」写回配对记录。
        // 只在**校验通过且同步成功**之后调用：失败/被拒的连接不得篡改地址（防投毒）。
        access.with_repo(Box::new(move |repo| {
            state.last_recv_seq = replay.last_recv_seq();
            state.last_sent_seq = sent;
            repo.save_sync_state(state)?;
            repo.update_paired_addr(&hello.client_id, peer_ip, hello.sync_port)?;
            repo.touch_device_sync(&hello.client_id, now)?;
            Ok(())
        }))?;

        Ok(ResyncOutcome {
            applied,
            conflicts,
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
