//! 数据模型（架构文档 §5.1）。
//!
//! 序列化统一 `serde`，wire 格式 = `snake_case`。二进制字段（盐、密钥密文、哈希）
//! 一律 Base64（标准带填充）。

pub mod b64serde;
pub mod conflict;
pub mod folder;
pub mod item;
pub mod sync_state;
pub mod trash;
pub mod vault;

pub use conflict::{ConflictChoice, ConflictEntry};
pub use folder::Folder;
pub use item::{AttachmentData, CustomField, Item, ItemKind, LoginData, SecureNoteData};
pub use sync_state::{BaseRev, DeviceSyncState, PairedDevice};
pub use trash::TrashEntry;
pub use vault::{HealthConfig, KdfParams, Vault, VaultSettings, WrappedKey};

/// 当前 UTC 毫秒时间戳（`i64`）。不使用 `chrono`/`time`，直接由 `SystemTime` 推导。
pub fn now_ms() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        // 时钟早于 epoch 属异常，返回 0 而非 panic。
        Err(_) => 0,
    }
}
