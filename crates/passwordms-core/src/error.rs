//! 统一错误类型与错误码。
//!
//! 设计要点（架构文档 §9.2）：
//! - 错误码字符串与前端 `ErrorCode` **同名同值**，`wire` 层只传 `code`。
//! - **统一化报错**：解锁失败无论「主密码错误」还是「数据损坏/被篡改」，一律返回
//!   [`CoreError::UnlockFailed`]（`ERR_UNLOCK_FAILED`），避免侧信道（A-05）。
//! - `Debug` 输出不含任何密钥/明文：本类型只承载非敏感描述。

use std::fmt;

/// 错误码常量表。与前端 `src/lib/errors.ts` 的 `ErrorCode` 严格同名同值。
pub mod codes {
    pub const ERR_UNLOCK_FAILED: &str = "ERR_UNLOCK_FAILED";
    pub const ERR_NO_VAULT: &str = "ERR_NO_VAULT";
    pub const ERR_VAULT_EXISTS: &str = "ERR_VAULT_EXISTS";
    pub const ERR_WEAK_MASTER_PASSWORD: &str = "ERR_WEAK_MASTER_PASSWORD";
    pub const ERR_NOT_UNLOCKED: &str = "ERR_NOT_UNLOCKED";
    pub const ERR_ITEM_NOT_FOUND: &str = "ERR_ITEM_NOT_FOUND";
    pub const ERR_FOLDER_NOT_FOUND: &str = "ERR_FOLDER_NOT_FOUND";
    pub const ERR_FOLDER_NAME_INVALID: &str = "ERR_FOLDER_NAME_INVALID";
    pub const ERR_VALIDATION: &str = "ERR_VALIDATION";
    pub const ERR_ATTACHMENT_TOO_LARGE: &str = "ERR_ATTACHMENT_TOO_LARGE";
    pub const ERR_VAULT_CAPACITY_EXCEEDED: &str = "ERR_VAULT_CAPACITY_EXCEEDED";
    pub const ERR_UNSAFE_URL: &str = "ERR_UNSAFE_URL";
    pub const ERR_IMPORT_PARSE: &str = "ERR_IMPORT_PARSE";
    pub const ERR_IMPORT_ENCRYPTED_EXPORT: &str = "ERR_IMPORT_ENCRYPTED_EXPORT";
    pub const ERR_PAIR_TOKEN_INVALID: &str = "ERR_PAIR_TOKEN_INVALID";
    pub const ERR_PAIR_TOKEN_EXPIRED: &str = "ERR_PAIR_TOKEN_EXPIRED";
    pub const ERR_PAIR_TOKEN_USED: &str = "ERR_PAIR_TOKEN_USED";
    pub const ERR_NOT_PAIRED: &str = "ERR_NOT_PAIRED";
    pub const ERR_REPLAY_DETECTED: &str = "ERR_REPLAY_DETECTED";
    pub const ERR_FINGERPRINT_MISMATCH: &str = "ERR_FINGERPRINT_MISMATCH";
    pub const ERR_SYNC_ABORTED: &str = "ERR_SYNC_ABORTED";
    pub const ERR_CONFLICT_EXISTS: &str = "ERR_CONFLICT_EXISTS";
    /// 【主理人裁定 U1/U2 新增】Joiner 本地库非空时禁止静默采用 Host 的 VK。
    pub const ERR_VK_ADOPT_REQUIRES_EMPTY: &str = "ERR_VK_ADOPT_REQUIRES_EMPTY";
    pub const ERR_IO: &str = "ERR_IO";
    pub const ERR_CRYPTO: &str = "ERR_CRYPTO";
    pub const ERR_INTERNAL: &str = "ERR_INTERNAL";
}

/// 核心错误。
///
/// 注意：`Debug` **不输出**任何密钥、明文或口令；`Validation`/`Crypto` 等携带的字符串
/// 仅用于开发者诊断，构造处严禁写入敏感值。
#[derive(thiserror::Error)]
pub enum CoreError {
    #[error("vault unlock failed")]
    UnlockFailed,
    #[error("no vault present at the given path")]
    NoVault,
    #[error("vault already exists")]
    VaultExists,
    #[error("master password too weak")]
    WeakMasterPassword,
    #[error("vault is locked")]
    NotUnlocked,
    #[error("item not found")]
    ItemNotFound,
    #[error("folder not found")]
    FolderNotFound,
    #[error("invalid folder name")]
    FolderNameInvalid,
    #[error("validation failed")]
    Validation(String),
    #[error("attachment exceeds the per-file size limit")]
    AttachmentTooLarge,
    #[error("vault total capacity exceeded")]
    VaultCapacityExceeded,
    #[error("unsafe url scheme")]
    UnsafeUrl,
    #[error("import file could not be parsed")]
    ImportParse(String),
    #[error("import source is an encrypted export")]
    ImportEncryptedExport,
    #[error("pair token does not match")]
    PairTokenInvalid,
    #[error("pair token expired")]
    PairTokenExpired,
    #[error("pair token already used")]
    PairTokenUsed,
    #[error("peer is not paired")]
    NotPaired,
    #[error("replay detected")]
    ReplayDetected,
    #[error("fingerprint / key confirmation mismatch")]
    FingerprintMismatch,
    #[error("sync aborted")]
    SyncAborted,
    #[error("unresolved conflict exists")]
    ConflictExists,
    /// 【主理人裁定 U1/U2】Joiner 本地库非空，需前端确认后清空或拒绝。
    #[error("vault key adoption requires an empty local vault")]
    VkAdoptRequiresEmpty { record_count: usize },
    #[error("io error")]
    Io(String),
    #[error("crypto operation failed")]
    Crypto(String),
    #[error("internal error")]
    Internal(String),
}

impl CoreError {
    /// 返回稳定的错误码字符串（供 IPC `IpcError.code` 使用）。
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::UnlockFailed => codes::ERR_UNLOCK_FAILED,
            CoreError::NoVault => codes::ERR_NO_VAULT,
            CoreError::VaultExists => codes::ERR_VAULT_EXISTS,
            CoreError::WeakMasterPassword => codes::ERR_WEAK_MASTER_PASSWORD,
            CoreError::NotUnlocked => codes::ERR_NOT_UNLOCKED,
            CoreError::ItemNotFound => codes::ERR_ITEM_NOT_FOUND,
            CoreError::FolderNotFound => codes::ERR_FOLDER_NOT_FOUND,
            CoreError::FolderNameInvalid => codes::ERR_FOLDER_NAME_INVALID,
            CoreError::Validation(_) => codes::ERR_VALIDATION,
            CoreError::AttachmentTooLarge => codes::ERR_ATTACHMENT_TOO_LARGE,
            CoreError::VaultCapacityExceeded => codes::ERR_VAULT_CAPACITY_EXCEEDED,
            CoreError::UnsafeUrl => codes::ERR_UNSAFE_URL,
            CoreError::ImportParse(_) => codes::ERR_IMPORT_PARSE,
            CoreError::ImportEncryptedExport => codes::ERR_IMPORT_ENCRYPTED_EXPORT,
            CoreError::PairTokenInvalid => codes::ERR_PAIR_TOKEN_INVALID,
            CoreError::PairTokenExpired => codes::ERR_PAIR_TOKEN_EXPIRED,
            CoreError::PairTokenUsed => codes::ERR_PAIR_TOKEN_USED,
            CoreError::NotPaired => codes::ERR_NOT_PAIRED,
            CoreError::ReplayDetected => codes::ERR_REPLAY_DETECTED,
            CoreError::FingerprintMismatch => codes::ERR_FINGERPRINT_MISMATCH,
            CoreError::SyncAborted => codes::ERR_SYNC_ABORTED,
            CoreError::ConflictExists => codes::ERR_CONFLICT_EXISTS,
            CoreError::VkAdoptRequiresEmpty { .. } => codes::ERR_VK_ADOPT_REQUIRES_EMPTY,
            CoreError::Io(_) => codes::ERR_IO,
            CoreError::Crypto(_) => codes::ERR_CRYPTO,
            CoreError::Internal(_) => codes::ERR_INTERNAL,
        }
    }

    /// 返回前端文案 key（`err.*`）。与架构文档 §9.2 一致。
    pub fn message_key(&self) -> &'static str {
        match self {
            CoreError::UnlockFailed => "err.unlock_failed",
            CoreError::NoVault => "err.no_vault",
            CoreError::VaultExists => "err.vault_exists",
            CoreError::WeakMasterPassword => "err.weak_password",
            CoreError::NotUnlocked => "err.not_unlocked",
            CoreError::ItemNotFound => "err.item_not_found",
            CoreError::FolderNotFound => "err.folder_not_found",
            CoreError::FolderNameInvalid => "err.folder_name",
            CoreError::Validation(_) => "err.validation",
            CoreError::AttachmentTooLarge => "err.attachment_too_large",
            CoreError::VaultCapacityExceeded => "err.vault_capacity",
            CoreError::UnsafeUrl => "err.unsafe_url",
            CoreError::ImportParse(_) => "err.import_parse",
            CoreError::ImportEncryptedExport => "err.import_encrypted",
            CoreError::PairTokenInvalid => "err.pair_token_invalid",
            CoreError::PairTokenExpired => "err.pair_token_expired",
            CoreError::PairTokenUsed => "err.pair_token_used",
            CoreError::NotPaired => "err.not_paired",
            CoreError::ReplayDetected => "err.replay",
            CoreError::FingerprintMismatch => "err.fingerprint",
            CoreError::SyncAborted => "err.sync_aborted",
            CoreError::ConflictExists => "err.conflict",
            CoreError::VkAdoptRequiresEmpty { .. } => "err.vk_adopt_requires_empty",
            CoreError::Io(_) => "err.io",
            CoreError::Crypto(_) => "err.crypto",
            CoreError::Internal(_) => "err.internal",
        }
    }
}

/// 手写 `Debug`：**不输出**内部诊断字符串，避免意外把上下文写进日志。
impl fmt::Debug for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 仅输出错误码，绝不输出可能包含上下文的字符串载荷。
        f.debug_tuple("CoreError").field(&self.code()).finish()
    }
}

impl From<std::io::Error> for CoreError {
    fn from(err: std::io::Error) -> Self {
        // 仅保留 io 错误的 kind，不携带路径等可能含隐私的信息。
        CoreError::Io(format!("io:{}", err.kind()))
    }
}

impl From<serde_json::Error> for CoreError {
    fn from(_err: serde_json::Error) -> Self {
        CoreError::Internal("json".to_string())
    }
}

impl From<uuid::Error> for CoreError {
    fn from(_err: uuid::Error) -> Self {
        CoreError::Validation("uuid".to_string())
    }
}

/// 便捷别名。
pub type CoreResult<T> = Result<T, CoreError>;
