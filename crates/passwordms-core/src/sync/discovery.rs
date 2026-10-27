//! 局域网自动发现（**D-74**）：打破「换网络后只能重新扫码配对」的死锁。
//!
//! # 要解决的死锁
//!
//! 重同步用**配对时静态记录**的 `peer_ip` / `peer_port` 直连
//! （[`crate::sync::resync::resync_with_peer`] 第 3 步），而地址自愈发生在**接收方**且
//! **必须连接已成功**才执行。换网络（DHCP 续租 / 换 Wi-Fi / 端口 rebind）后双方 IP 全变 →
//! 双方都连不上 → 自愈永远触发不了 → 只能重新配对。
//!
//! 本模块给发起方一条兜底：静态地址连不上时，**按需广播一次**，让对端用自己的
//! `sync_key` 认出请求并单播应答；请求方从应答的**源地址**得到对端新 IP，从报文的
//! `port` 字段得到新端口，然后重连一次。
//!
//! # 三条设计取舍（改代码前先读）
//!
//! 1. **不做被动宣告、不周期广播**：只在"用户触发同步 + 静态地址失败"时广播一次。周期
//!    宣告会让同网段的人持续追踪"这台机器装着密码管理器、此刻在活动"。
//! 2. **报文里没有 `device_id` / 设备名 / 指纹**：只有 32B 随机数 `q` 与 32B 不可区分
//!    tag。陌生人对广播包**不应答**（不知道 key 就算不出 tag），所以外人既不能确认这台
//!    机器装了 PasswordMS，也无法放大攻击。
//! 3. **不用 mDNS**：要新增 crate（违反架构 §8.1 的 23 个上限），并在 Android 侧引入
//!    NSD / `MulticastLock` / 权限；而 `if-addrs`（已在依赖树）的
//!    `Ifv4Addr.broadcast` 现成可用。
//!
//! # 报文
//!
//! ```text
//! 请求  { v: 1, q: <32B base64url>, tags: [ b64url(HMAC(sync_key_i, "pmsd1/req"‖q)), … ] }
//! 应答  { v: 1, q: <原样回显>, tag: b64url(HMAC(sync_key_i, "pmsd1/resp"‖q)), port: <TCP 口> }
//! ```
//!
//! 报文格式上 `tags` 是数组（协议预留），但**请求侧当前只携带 1 个 tag**：发起方一次
//! 广播只针对一个已配对对端（手头只有一个 `sync_key`）。响应方仍按多 tag 遍历校验
//! （`DISCOVERY_MAX_TAGS` 上限防灌爆），未来「一次发现多个对端」无需改协议。
//! （D-108 豆包审查 3.3-4：原文档未说明此差异，读代码者会误以为实现漏了多 tag。）
//!
//! 两个 label 必须不同：否则应答可被当作请求回灌（反射/重放面）。

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::constants::{DISCOVERY_MAX_TAGS, DISCOVERY_PORT, DISCOVERY_VERSION};
use crate::crypto::keys::{self, ItemKey};
use crate::crypto::rng;
use crate::error::{CoreError, CoreResult};
use crate::store::repository::Repository;
use crate::sync::server::device_key_aad;

type HmacSha256 = Hmac<Sha256>;

/// 请求侧域分离标签。
pub const LABEL_REQ: &[u8] = b"pmsd1/req";
/// 应答侧域分离标签（必须与 [`LABEL_REQ`] 不同）。
pub const LABEL_RESP: &[u8] = b"pmsd1/resp";

/// `q` 的固定长度（随机数，本次等待窗口内唯一）。
const QUERY_LEN: usize = 32;
/// 一轮轮询最多处理的包数（防被灌爆：单轮有界，剩下的留给下一个 200ms 周期）。
const MAX_PACKETS_PER_POLL: usize = 8;
/// 单个源 IP 每秒最多应答次数。
const RATE_PER_SOURCE_PER_SEC: u32 = 5;
/// 全局每秒最多应答次数。
const RATE_GLOBAL_PER_SEC: u32 = 50;
/// 收包缓冲区（报文最大 ~1.5KB：8 个 43B tag + JSON 外壳）。
const RECV_BUF: usize = 2048;
/// 速率窗口长度。
const RATE_WINDOW: Duration = Duration::from_secs(1);

/// 广播请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryRequest {
    /// 协议版本（[`DISCOVERY_VERSION`]）。
    pub v: u16,
    /// 32B 随机数；应答须原样回显。
    #[serde(with = "crate::sync::protocol::b64url")]
    pub q: Vec<u8>,
    /// 每个元素的含义见模块文档；每项 = `b64url(HMAC(sync_key_i, LABEL_REQ‖q))`。
    #[serde(default)]
    pub tags: Vec<String>,
}

/// 单播应答（源地址即对端本次出口 IP）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryResponse {
    /// 协议版本。
    pub v: u16,
    /// 原样回显请求的 `q`。
    #[serde(with = "crate::sync::protocol::b64url")]
    pub q: Vec<u8>,
    /// `b64url(HMAC(sync_key_i, LABEL_RESP‖q))`。
    pub tag: String,
    /// 本端重同步 **TCP** 监听端口。
    pub port: u16,
}

/// 计算 `b64url(HMAC-SHA256(key, label ‖ q))`。
fn tag_of(key: &[u8], label: &[u8], q: &[u8]) -> CoreResult<String> {
    let mut mac =
        HmacSha256::new_from_slice(key).map_err(|_| CoreError::Crypto("hmac key".to_string()))?;
    mac.update(label);
    mac.update(q);
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

/// 请求 tag（发起方计算、响应方校验）。
pub fn request_tag(sync_key: &ItemKey, q: &[u8]) -> CoreResult<String> {
    tag_of(&sync_key[..], LABEL_REQ, q)
}

/// 应答 tag（响应方计算、发起方校验）。
pub fn response_tag(sync_key: &ItemKey, q: &[u8]) -> CoreResult<String> {
    tag_of(&sync_key[..], LABEL_RESP, q)
}

/// 常量时间比较两个 base64url 字符串（长度不同直接 false）。
fn tag_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

/// 计算本次广播的目标地址：每个网卡的广播地址 + 受限广播 `255.255.255.255`。
///
/// 多网卡机器（有线 + 无线 + 虚拟网卡）逐网卡发一份：只发受限广播时，部分系统/路由器
/// 不会把它转发到所有子网接口。
pub fn broadcast_targets() -> Vec<SocketAddr> {
    let mut out: Vec<SocketAddr> = Vec::new();
    if let Ok(addrs) = if_addrs::get_if_addrs() {
        for a in addrs {
            if let if_addrs::IfAddr::V4(v4) = a.addr {
                if let Some(b) = v4.broadcast {
                    if !b.is_unspecified() {
                        out.push(SocketAddr::new(IpAddr::V4(b), DISCOVERY_PORT));
                    }
                }
            }
        }
    }
    let limited = SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), DISCOVERY_PORT);
    if !out.contains(&limited) {
        out.push(limited);
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|a| seen.insert(*a));
    out
}

/// 生产入口：向本网段广播一次，等待应答。
///
/// `self_port`：本机自己的重同步 TCP 端口。用于丢弃**自己应答自己**的包——本机的
/// 应答 socket 与请求 socket 同机同网段，广播出去的包有可能被自己收到；而两端持有同一把
/// `sync_key`，tag 校验拦不住这种情况。传 `None` 表示本机无监听器（无需过滤）。
pub fn discover_peer(
    sync_key: &ItemKey,
    wait: Duration,
    self_port: Option<u16>,
) -> CoreResult<Option<(String, u16)>> {
    let targets = broadcast_targets();
    discover_peer_with(sync_key, &targets, wait, self_port)
}

/// 可注入目标版本的 [`discover_peer`]（**测试必须用它**）。
///
/// CI 上真实广播不可靠（容器无广播路由、防火墙、虚拟网卡），所以把"socket + 目标地址"
/// 变成参数：测试用 loopback 单播注入，生产用 [`broadcast_targets`]。
///
/// # 错误语义
///
/// 任何 **IO** 失败（bind / send / recv / 超时）都返回 `Ok(None)`——发现只是增强路径，
/// 不能把"同步失败的原因"从"连不上对端"改写成"发现出错"。只有 `rng` / HMAC 这类
/// 内部错误才上抛。
pub fn discover_peer_with(
    sync_key: &ItemKey,
    targets: &[SocketAddr],
    wait: Duration,
    self_port: Option<u16>,
) -> CoreResult<Option<(String, u16)>> {
    let q = rng::random_bytes(QUERY_LEN)?;
    let request = DiscoveryRequest {
        v: DISCOVERY_VERSION,
        q: q.clone(),
        tags: vec![request_tag(sync_key, &q)?],
    };
    let payload = serde_json::to_vec(&request)?;

    let socket = match UdpSocket::bind(("0.0.0.0", 0)) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    let _ = socket.set_broadcast(true);
    for target in targets {
        // 单个目标失败不影响其它网卡（接口已消失 / 无广播权限 / 无路由）。
        let _ = socket.send_to(&payload, target);
    }

    let deadline = Instant::now() + wait;
    let mut buf = [0u8; RECV_BUF];
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        let remaining = deadline.saturating_duration_since(now);
        let slice = remaining.max(Duration::from_millis(1));
        if socket.set_read_timeout(Some(slice)).is_err() {
            return Ok(None);
        }
        let (n, src) = match socket.recv_from(&mut buf) {
            Ok(v) => v,
            // 超时或任何读错误：本轮发现结束（不区分，见方法文档）。
            Err(_) => return Ok(None),
        };
        let Ok(resp) = serde_json::from_slice::<DiscoveryResponse>(&buf[..n]) else {
            continue;
        };
        // `q` 不符 → 同一网段可能有另一台设备正在发现，丢弃继续等。
        if resp.v != DISCOVERY_VERSION || resp.q != q {
            continue;
        }
        let expected = response_tag(sync_key, &q)?;
        if !tag_eq(&expected, &resp.tag) {
            continue;
        }
        // 自己应答自己：端口与本机 TCP 监听口相同且来源是本机地址 → 丢弃。
        if let Some(own) = self_port {
            if resp.port == own && is_local_ip(src.ip()) {
                continue;
            }
        }
        return Ok(Some((src.ip().to_string(), resp.port)));
    }
}

/// 是否为**本机**地址（用于识别"自己应答自己"）。
fn is_local_ip(ip: IpAddr) -> bool {
    if ip.is_loopback() {
        return true;
    }
    match if_addrs::get_if_addrs() {
        Ok(addrs) => addrs.iter().any(|a| a.ip() == ip),
        Err(_) => false,
    }
}

/// 发现请求的应答器（响应方侧）。
///
/// 持有 UDP socket 与限速状态；由 [`crate::sync::resync::ResyncServer`] 在解锁期间
/// 每 200ms 轮询一次。
pub struct DiscoveryResponder {
    socket: UdpSocket,
    /// 源 IP → (窗口起点, 本窗口已应答数)
    per_source: HashMap<IpAddr, (Instant, u32)>,
    /// (窗口起点, 本窗口已应答数)
    global: (Instant, u32),
}

impl DiscoveryResponder {
    /// 绑定固定发现端口（非阻塞）。失败由调用方降级为"不提供发现应答"。
    pub fn bind() -> std::io::Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT))?;
        Ok(Self::from_socket(socket))
    }

    /// 用**已绑定**的 socket 构造（测试注入任意端口）。
    pub fn from_socket(socket: UdpSocket) -> Self {
        let _ = socket.set_nonblocking(true);
        Self {
            socket,
            per_source: HashMap::new(),
            global: (Instant::now(), 0),
        }
    }

    /// 实际绑定的地址（测试用于取得注入端口）。
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.socket.local_addr().ok()
    }

    /// 非阻塞轮询一次：最多处理 [`MAX_PACKETS_PER_POLL`] 个包，返回**应答数**。
    ///
    /// 无包可读时返回 0（`WouldBlock` 不是错误）。
    pub fn poll_once(&mut self, repo: &Repository, tcp_port: u16) -> usize {
        let mut replied = 0usize;
        let mut buf = [0u8; RECV_BUF];
        for _ in 0..MAX_PACKETS_PER_POLL {
            let (n, src) = match self.socket.recv_from(&mut buf) {
                Ok(v) => v,
                // WouldBlock（无包）/ 其它瞬时错误：本轮结束。
                Err(_) => break,
            };
            let Ok(req) = serde_json::from_slice::<DiscoveryRequest>(&buf[..n]) else {
                continue;
            };
            // 版本 / 形状校验：不合格一律静默丢弃（不回错误包，不暴露任何信息）。
            if req.v != DISCOVERY_VERSION || req.q.len() != QUERY_LEN || req.tags.is_empty() {
                continue;
            }
            if !self.allow(src.ip()) {
                continue;
            }
            let Some(tag) = match_paired_device(repo, &req) else {
                // 一个都没命中 → 静默不回包（这就是"不向陌生人暴露存在"的实现）。
                continue;
            };
            let resp = DiscoveryResponse {
                v: DISCOVERY_VERSION,
                q: req.q.clone(),
                tag,
                port: tcp_port,
            };
            let Ok(bytes) = serde_json::to_vec(&resp) else {
                continue;
            };
            if self.socket.send_to(&bytes, src).is_ok() {
                replied += 1;
            }
        }
        replied
    }

    /// 限速：按源 IP 每秒 [`RATE_PER_SOURCE_PER_SEC`]、全局每秒 [`RATE_GLOBAL_PER_SEC`]。
    fn allow(&mut self, ip: IpAddr) -> bool {
        let now = Instant::now();
        if now.duration_since(self.global.0) >= RATE_WINDOW {
            self.global = (now, 0);
        }
        if self.global.1 >= RATE_GLOBAL_PER_SEC {
            return false;
        }
        let (window_start, count) = *self.per_source.entry(ip).or_insert((now, 0));
        let mut count = count;
        let mut window_start = window_start;
        if now.duration_since(window_start) >= RATE_WINDOW {
            window_start = now;
            count = 0;
        }
        if count >= RATE_PER_SOURCE_PER_SEC {
            self.per_source.insert(ip, (window_start, count));
            return false;
        }
        self.per_source.insert(ip, (window_start, count + 1));
        self.global.1 += 1;
        // 防 map 无限增长（伪造源 IP 灌包时）：条目多了就清掉过期窗口。
        if self.per_source.len() > 64 {
            self.per_source
                .retain(|_, (start, _)| now.duration_since(*start) < RATE_WINDOW);
        }
        true
    }
}

/// 在已配对设备里找**持有本次请求 tag 那把 key** 的那一台；命中返回应答 tag。
///
/// 单条记录损坏（解包失败）只跳过该条，不影响其它设备——否则一条坏记录会让整个发现
/// 功能静默失效。
fn match_paired_device(repo: &Repository, req: &DiscoveryRequest) -> Option<String> {
    for device in repo.list_paired_devices() {
        let key = match keys::unwrap_item_key(
            repo.vault_key(),
            &device.sync_key_wrapped,
            &device_key_aad(&device.device_id),
        ) {
            Ok(k) => k,
            Err(_) => continue,
        };
        let Ok(expected) = request_tag(&key, &req.q) else {
            continue;
        };
        let hit = req
            .tags
            .iter()
            .take(DISCOVERY_MAX_TAGS)
            .any(|t| tag_eq(&expected, t));
        if hit {
            return response_tag(&key, &req.q).ok();
        }
    }
    None
}
