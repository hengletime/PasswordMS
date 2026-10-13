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
    let stream = TcpStream::connect((ip, port)).map_err(|_| CoreError::SyncAborted)?;
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
