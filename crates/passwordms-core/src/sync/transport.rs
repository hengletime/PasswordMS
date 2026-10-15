//! 传输层：原生 TCP + 长度前缀帧（架构文档 §3.6）。
//!
//! LAN 直连 + 应用层 E2E 加密，**无需 TLS/HTTP 栈**（省体积）。

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crate::constants::SYNC_MAX_FRAME_BYTES;
use crate::error::{CoreError, CoreResult};

/// 监听端口（绑定 0.0.0.0）。
pub fn listen(port: u16) -> CoreResult<TcpListener> {
    TcpListener::bind(("0.0.0.0", port)).map_err(|_| CoreError::SyncAborted)
}

/// 连接对端。
pub fn connect(ip: &str, port: u16) -> CoreResult<TcpStream> {
    connect_with_timeout(ip, port, std::time::Duration::from_secs(30))
}

/// 带显式超时的连接（D-44）。
///
/// 修复两点：
/// 1. `TcpStream::connect` 对「SYN 被静默丢弃」的地址要挂约 21 秒才报错——
///    多候选 IP 逐个尝试时必须用短超时，否则用户面对长时间无响应。
/// 2. 旧实现把 connect 失败一律折叠成 `SyncAborted`（「同步已中止」），错误码失真；
///    现在保留 io kind（TimedOut/NetworkUnreachable → 对应网络文案）。
pub fn connect_with_timeout(ip: &str, port: u16, timeout: std::time::Duration) -> CoreResult<TcpStream> {
    let addr: std::net::IpAddr = ip
        .parse()
        .map_err(|_| CoreError::Validation("ip".to_string()))?;
    let stream = TcpStream::connect_timeout(
        &std::net::SocketAddr::new(addr, port),
        timeout,
    )?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|_| CoreError::SyncAborted)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(|_| CoreError::SyncAborted)?;
    Ok(stream)
}

/// 发送一个长度前缀帧（`u32 LE` 长度 + 帧体）。
pub fn send_frame<W: Write>(writer: &mut W, bytes: &[u8]) -> CoreResult<()> {
    if bytes.len() > SYNC_MAX_FRAME_BYTES as usize {
        return Err(CoreError::Validation("frame_too_large".to_string()));
    }
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()?;
    Ok(())
}

/// 接收一个长度前缀帧。
pub fn recv_frame<R: Read>(reader: &mut R) -> CoreResult<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf);
    if len > SYNC_MAX_FRAME_BYTES {
        return Err(CoreError::Validation("frame_too_large".to_string()));
    }
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

/// 尽力探测本机局域网 IPv4（UDP `connect` 不发送任何数据包，仅让内核选路）。
pub fn local_ipv4() -> CoreResult<String> {
    let socket = std::net::UdpSocket::bind(("0.0.0.0", 0)).map_err(|_| CoreError::SyncAborted)?;
    // 连接到公网地址仅为选路；不会产生任何流量（无 send 调用）。
    socket
        .connect(("203.0.113.1", 9))
        .map_err(|_| CoreError::SyncAborted)?;
    let addr = socket.local_addr().map_err(|_| CoreError::SyncAborted)?;
    Ok(addr.ip().to_string())
}

/// 枚举本机**全部**非回环 IPv4 候选（D-44）。
///
/// 多网卡机器（WSL2/Hyper-V 虚拟网卡、有线+无线并存）上，UDP 选路探测只给一个答案，
/// 一旦选中手机路由不到的虚拟网卡，配对必败。二维码改为携带全部候选地址，
/// Joiner 逐个尝试：主选地址（默认路由出口）排最前，其余保持枚举顺序。
pub fn local_ipv4s() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Ok(addrs) = if_addrs::get_if_addrs() {
        for a in addrs {
            if let std::net::IpAddr::V4(ip) = a.ip() {
                if !ip.is_loopback() && !ip.is_link_local() {
                    out.push(ip.to_string());
                }
            }
        }
    }
    if let Ok(primary) = local_ipv4() {
        if let Some(pos) = out.iter().position(|x| *x == primary) {
            let p = out.remove(pos);
            out.insert(0, p);
        } else {
            out.insert(0, primary);
        }
    }
    // 去重（保持顺序）。
    let mut seen = std::collections::HashSet::new();
    out.retain(|x| seen.insert(x.clone()));
    out
}
