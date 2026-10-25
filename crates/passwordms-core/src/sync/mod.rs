//! 局域网同步：协议报文、握手、传输、Host/Joiner 状态机、冲突合并、防重放
//! （架构文档 §3）。

pub mod access;
pub mod blob_transfer;
pub mod client;
pub mod discovery;
pub mod handshake;
/// 【D-96】Host 身份密钥文件（VK 包裹格式）。
pub mod identity;
pub mod merge;
pub mod paging;
pub mod protocol;
pub mod replay;
pub mod resync;
pub mod server;
pub mod transport;

pub use handshake::HostIdentity;
pub use protocol::{
    Accept, ChangeRecord, Confirm, Hello, QRPayload, Reject, SyncEnvelope, SyncPayload,
};
pub use resync::{ResyncOutcome, ResyncServer};
