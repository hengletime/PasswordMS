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
    /// 【D-108 新增】附件在父条目内不存在（豆包审查 3.1-10）。
    pub const ERR_ATTACHMENT_NOT_FOUND: &str = "ERR_ATTACHMENT_NOT_FOUND";
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
    /// 【D-108 新增】同步互斥（豆包审查 3.4-7：原由 sync_cmd 硬编码构造）。
    pub const ERR_SYNC_BUSY: &str = "ERR_SYNC_BUSY";
    pub const ERR_CONFLICT_EXISTS: &str = "ERR_CONFLICT_EXISTS";
    /// 【D-53 新增】附件条目存在但本地无密文 blob（未同步）。
    pub const ERR_BLOB_MISSING: &str = "ERR_BLOB_MISSING";
    /// 【D-53 新增】blob 组帧失败（分片缺失/错序/tag 不符；内部，转 missing 上报）。
    pub const ERR_BLOB_INCOMPLETE: &str = "ERR_BLOB_INCOMPLETE";
    /// 【D-53 新增】接收端容量预检超限（Meta 阶段拒绝，不落半个分片）。
    pub const ERR_BLOB_CAPACITY: &str = "ERR_BLOB_CAPACITY";
    /// 【D-60 新增】单条目附件数超上限（MAX_ATTACHMENTS_PER_ITEM）。
    pub const ERR_ATTACHMENT_LIMIT: &str = "ERR_ATTACHMENT_LIMIT";
    /// 【主理人裁定 U1/U2 新增】Joiner 本地库非空时禁止静默采用 Host 的 VK。
    pub const ERR_VK_ADOPT_REQUIRES_EMPTY: &str = "ERR_VK_ADOPT_REQUIRES_EMPTY";
    /// 【D-91 新增】回收站还原被拒：同 item_id 已有活跃条目（多设备同步复活了新版本），
    /// 旧快照还原会顶掉同步来的新数据（两端丢数据）。
    pub const ERR_RESTORE_CONFLICT: &str = "ERR_RESTORE_CONFLICT";
    /// 【1.3.0 生物识别新增】平台不可用 / 本机未开启快速解锁。
    pub const ERR_BIOMETRIC_UNAVAILABLE: &str = "ERR_BIOMETRIC_UNAVAILABLE";
    /// 【1.3.0 生物识别新增】用户在平台弹窗中取消。
    pub const ERR_BIOMETRIC_CANCELED: &str = "ERR_BIOMETRIC_CANCELED";
    /// 【1.3.0 生物识别新增】封印已失效（改主密码 / 生物注册变更 / 篡改，见 `docs/08` §10）。
    pub const ERR_BIOMETRIC_INVALIDATED: &str = "ERR_BIOMETRIC_INVALIDATED";
    /// 【1.3.0 生物识别新增】该操作需要主密码（生物解锁会话不持有 KEK）。
    pub const ERR_NEED_MASTER_PASSWORD: &str = "ERR_NEED_MASTER_PASSWORD";
    /// 【D-115 新增】已解锁会话内复核主密码失败（开启快速解锁 / 改主密码的旧密码）。
    ///
    /// 与 `ERR_UNLOCK_FAILED` **分工明确**：后者只属于锁屏入口（未认证调用者，A-05 不可区分），
    /// 本码只属于已认证会话。两者混用会让已解锁的用户看到「解锁失败，请重试」——
    /// 明明在设置页里，却被告知「解锁失败」（主人真机走查 D-115）。
    pub const ERR_WRONG_MASTER_PASSWORD: &str = "ERR_WRONG_MASTER_PASSWORD";
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
    /// 【D-108 新增】附件在父条目内不存在（remove_attachment / export_attachment
    /// 的附件定位失败）。原实现借用 `ItemNotFound`，用户与日志都分不清
    /// 「条目没了」还是「附件没了」（豆包审查 3.1-10）。
    #[error("attachment not found")]
    AttachmentNotFound,
    #[error("folder not found")]
    FolderNotFound,
    #[error("invalid folder name")]
    FolderNameInvalid,
    #[error("validation failed")]
    Validation(String),
    #[error("attachment exceeds the per-file size limit")]
    AttachmentTooLarge,
    /// 【D-60】单条目附件数超上限（§3.14.5）。
    #[error("attachment count exceeds the per-item limit")]
    AttachmentLimit,
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
    /// 【D-108 新增】同一时刻已有一场同步在途（O2 互斥）。原由 `sync_cmd`
    /// 硬编码构造 `ERR_SYNC_BUSY`（豆包审查 3.4-7：缺少编译期约束、手工同步点）。
    #[error("another sync is already in flight")]
    SyncBusy,
    #[error("unresolved conflict exists")]
    ConflictExists,
    /// 【D-53】附件条目存在但本地无密文 blob（未同步），导出/查看时触发。
    #[error("attachment blob not present locally")]
    BlobMissing,
    /// 【D-53】blob 组帧失败（分片缺失/错序/tag 不符）。
    #[error("blob assembly incomplete")]
    BlobIncomplete,
    /// 【D-53】接收端容量预检超限。
    #[error("blob exceeds receiver vault capacity")]
    BlobCapacity,
    /// 【主理人裁定 U1/U2】Joiner 本地库非空，需前端确认后清空或拒绝。
    #[error("vault key adoption requires an empty local vault")]
    VkAdoptRequiresEmpty { record_count: usize },
    /// 【D-91】回收站还原被拒：同 item_id 已有活跃条目（典型：对端删除后又编辑、
    /// 同步复活了新版本）。旧快照 rev+1 + updated_at=now 会把同步来的新数据顶掉，
    /// 并经 LWW 把覆盖传播回对端（两端丢数据）。
    #[error("restore conflicts with an existing live item")]
    RestoreConflict,
    /// 【1.3.0 生物识别】平台不可用，或本机未开启快速解锁。
    ///
    /// **不得**与 [`CoreError::UnlockFailed`] 混用：生物路径的失败原因对用户是可行动的
    /// （去系统设置配 Hello、重新开启开关），折叠成「解锁失败」等于把用户带偏（`docs/08` §10）。
    #[error("biometric unlock unavailable")]
    BiometricUnavailable,
    /// 【1.3.0 生物识别】用户在平台弹窗中取消——前端应**静默**回主密码输入框。
    #[error("biometric prompt canceled")]
    BiometricCanceled,
    /// 【1.3.0 生物识别】封印已失效，携带可区分的失效原因。
    #[error("biometric seal invalidated")]
    BiometricInvalidated(crate::biometric::InvalidatedReason),
    /// 【1.3.0 生物识别】该操作需要主密码：生物解锁会话不持有 KEK。
    #[error("master password required for this operation")]
    NeedMasterPassword,
    /// 【D-115 新增】已解锁会话内复核主密码失败。
    ///
    /// 与 [`CoreError::UnlockFailed`] 的差别**在于调用位置而非泄露多少**：
    /// `UnlockFailed` 是锁屏入口（未认证调用者）的答案，A-05 要求它对「密码错 / 头部坏」
    /// 不可区分；而本变体只可能由 [`crate::session::Session::verify_master_password`] 与
    /// [`crate::session::Session::change_master_password`] 在**已解锁会话**里返回——
    /// 调用者已经过认证、手里还握着 VK 与容器文件，密码对不对对其不构成任何新信息
    /// （它本来就能离线逐条试）。这种场合继续回「解锁失败，请重试」纯属把用户带偏。
    #[error("master password does not match")]
    WrongMasterPassword,
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
            CoreError::AttachmentNotFound => codes::ERR_ATTACHMENT_NOT_FOUND,
            CoreError::FolderNotFound => codes::ERR_FOLDER_NOT_FOUND,
            CoreError::FolderNameInvalid => codes::ERR_FOLDER_NAME_INVALID,
            CoreError::Validation(_) => codes::ERR_VALIDATION,
            CoreError::AttachmentTooLarge => codes::ERR_ATTACHMENT_TOO_LARGE,
            CoreError::AttachmentLimit => codes::ERR_ATTACHMENT_LIMIT,
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
            CoreError::SyncBusy => codes::ERR_SYNC_BUSY,
            CoreError::ConflictExists => codes::ERR_CONFLICT_EXISTS,
            CoreError::BlobMissing => codes::ERR_BLOB_MISSING,
            CoreError::BlobIncomplete => codes::ERR_BLOB_INCOMPLETE,
            CoreError::BlobCapacity => codes::ERR_BLOB_CAPACITY,
            CoreError::VkAdoptRequiresEmpty { .. } => codes::ERR_VK_ADOPT_REQUIRES_EMPTY,
            CoreError::RestoreConflict => codes::ERR_RESTORE_CONFLICT,
            CoreError::BiometricUnavailable => codes::ERR_BIOMETRIC_UNAVAILABLE,
            CoreError::BiometricCanceled => codes::ERR_BIOMETRIC_CANCELED,
            CoreError::BiometricInvalidated(_) => codes::ERR_BIOMETRIC_INVALIDATED,
            CoreError::NeedMasterPassword => codes::ERR_NEED_MASTER_PASSWORD,
            CoreError::WrongMasterPassword => codes::ERR_WRONG_MASTER_PASSWORD,
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
            CoreError::AttachmentNotFound => "err.attachment_not_found",
            CoreError::FolderNotFound => "err.folder_not_found",
            CoreError::FolderNameInvalid => "err.folder_name",
            CoreError::Validation(_) => "err.validation",
            CoreError::AttachmentTooLarge => "err.attachment_too_large",
            CoreError::AttachmentLimit => "err.attachment_limit",
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
            CoreError::SyncBusy => "err.sync_busy",
            CoreError::ConflictExists => "err.conflict",
            CoreError::BlobMissing => "err.blob_missing",
            CoreError::BlobIncomplete => "err.blob_incomplete",
            CoreError::BlobCapacity => "err.blob_capacity",
            CoreError::VkAdoptRequiresEmpty { .. } => "err.vk_adopt_requires_empty",
            CoreError::RestoreConflict => "err.restore_conflict",
            // 1.3.0 生物识别：四类失败各给独立 key —— 用户能据此决定「去配 Hello」
            // 「重新开启开关」还是「改用主密码」，笼统一句「解锁失败」反而误导（`docs/08` §10）。
            CoreError::BiometricUnavailable => "err.biometric_unavailable",
            CoreError::BiometricCanceled => "err.biometric_canceled",
            CoreError::BiometricInvalidated(reason) => match reason {
                crate::biometric::InvalidatedReason::MasterPasswordChanged => {
                    "err.bio_invalidated_password"
                }
                crate::biometric::InvalidatedReason::VaultKeyChanged => {
                    "err.bio_invalidated_vault_key"
                }
                crate::biometric::InvalidatedReason::BiometricEnrollmentChanged => {
                    "err.bio_invalidated_enrollment"
                }
                // D-127：平台保护密钥被重置（TPM 清理 / 系统更新 / 重装），与篡改分开说——
                // 前者是「重新开启一次就好」，后者可能是攻击痕迹，混在一起会误导用户。
                crate::biometric::InvalidatedReason::PlatformKeyReset => {
                    "err.bio_invalidated_platform_key_reset"
                }
                crate::biometric::InvalidatedReason::Tampered => "err.bio_invalidated_tampered",
            },
            CoreError::NeedMasterPassword => "err.need_master_password",
            CoreError::WrongMasterPassword => "err.wrong_master_password",
            // io::Error 有两类来源：本机文件读写、TCP 同步链路（transport 的 read/write 超时、
            // 对端拒绝/断开都走 From<std::io::Error>）。按 kind 细分文案——
            // 把「网络问题」从笼统的「文件读写错误」里拆出来，避免把用户排查方向带偏（D-40 跟进）。
            // 注意 UnexpectedEof 文件截断/网络断开两可，保守归 err.io。
            CoreError::Io(kind) => {
                // 载荷格式："io:Kind" 或带阶段前缀的 "stage|io:Kind"（server.rs 标注失败阶段，D-44）。
                let (stage, k) = match kind.split_once('|') {
                    Some((s, rest)) => (Some(s), rest),
                    None => (None, kind.as_str()),
                };
                // D-57：connect 阶段失败**无论 kind 为何**（TimedOut/Other/NotConnected/
                // Uncategorized/…）一律映射 `err.network_unreachable`——连接失败只有一种
                // 用户可行动的含义（网络不通/不在同一网段）；「对方仍停在配对等待页」的
                // TimedOut 提示只适用于配对/交换阶段，对重同步的连接阶段是误导（曾致
                // 用户往「文件读写」方向排查）。
                if stage == Some("connect") {
                    return "err.network_unreachable";
                }
                // 归一化匹配（D-57 补充根因）：kind 载荷来自 `format!("io:{}", err.kind())`
                // 的 Debug 输出，跨平台形态不一——Linux 类平台多为 PascalCase（"TimedOut"），
                // Windows 实测为小写带空格（"timed out"/"connection refused"）。
                // D-44 的 PascalCase 字面表在本平台全部落空（一律坠入 err.io），正是客户
                // 看到「文件读写出错」的第二个根因。统一去前缀、小写、仅留字母数字后匹配。
                let norm: String = k
                    .trim_start_matches("io:")
                    .to_ascii_lowercase()
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .collect();
                match norm.as_str() {
                    "timedout" => "err.network_timeout",
                    "connectionrefused" => "err.network_refused",
                    "connectionreset" | "connectionaborted" | "brokenpipe" => "err.network_broken",
                    // Android 上无路由/不可达（EHOSTUNREACH/ENETUNREACH）在 stable Rust
                    // 折叠为 Uncategorized——配对场景几乎必然是「两台设备不通」，单独给文案（D-44）。
                    "uncategorized" | "networkunreachable" | "hostunreachable" => {
                        "err.network_unreachable"
                    }
                    _ => "err.io",
                }
            }
            CoreError::Crypto(_) => "err.crypto",
            CoreError::Internal(_) => "err.internal",
        }
    }
}

/// 手写 `Debug`：**不输出**内部诊断字符串，避免意外把上下文写进日志。
///
/// 例外（D-44）：`Io` 的载荷**只含** `stage|io:Kind`（阶段名 + io kind，均无敏感信息），
/// 把它打出来——否则真机日志只剩 `ERR_IO`，网络故障无从定位。
impl fmt::Debug for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Io(kind) => f
                .debug_tuple("CoreError")
                .field(&self.code())
                .field(kind)
                .finish(),
            _ => f.debug_tuple("CoreError").field(&self.code()).finish(),
        }
    }
}

/// 供 server.rs 给 io 错误标注失败阶段（`stage|io:Kind`，D-44）。
///
/// 阶段名是静态字符串（hello_recv / accept_send / …），不携带任何用户数据。
pub fn stage_io(err: CoreError, stage: &'static str) -> CoreError {
    match err {
        CoreError::Io(kind) => CoreError::Io(format!("{stage}|{kind}")),
        other => other,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_message_keys_by_kind() {
        assert_eq!(
            CoreError::Io("io:TimedOut".into()).message_key(),
            "err.network_timeout"
        );
        assert_eq!(
            CoreError::Io("io:ConnectionRefused".into()).message_key(),
            "err.network_refused"
        );
        assert_eq!(
            CoreError::Io("io:ConnectionReset".into()).message_key(),
            "err.network_broken"
        );
        assert_eq!(
            CoreError::Io("io:Uncategorized".into()).message_key(),
            "err.network_unreachable"
        );
        assert_eq!(
            CoreError::Io("io:OutOfMemory".into()).message_key(),
            "err.io"
        );
    }

    #[test]
    fn staged_io_message_key_ignores_prefix() {
        assert_eq!(
            CoreError::Io("hello_recv|io:TimedOut".into()).message_key(),
            "err.network_timeout"
        );
        assert_eq!(
            CoreError::Io("exchange_done_recv|io:Uncategorized".into()).message_key(),
            "err.network_unreachable"
        );
    }

    #[test]
    fn connect_stage_always_maps_to_network_unreachable() {
        // D-57：connect 阶段失败无论 kind 为何 → err.network_unreachable。
        for kind in [
            "io:TimedOut",
            "io:timed out",
            "io:Other",
            "io:NotConnected",
            "io:Uncategorized",
            "io:uncategorized",
            "io:ConnectionRefused",
            "io:connection refused",
            "io:OutOfMemory",
        ] {
            let staged = format!("connect|{kind}");
            assert_eq!(
                CoreError::Io(staged).message_key(),
                "err.network_unreachable",
                "connect 阶段的 {kind} 应映射 network_unreachable"
            );
        }
        // 回归：非 connect 阶段的既有映射不变（PascalCase 与 Windows 小写形态均覆盖）。
        assert_eq!(
            CoreError::Io("hello_recv|io:TimedOut".into()).message_key(),
            "err.network_timeout"
        );
        assert_eq!(
            CoreError::Io("hello_recv|io:timed out".into()).message_key(),
            "err.network_timeout"
        );
        assert_eq!(
            CoreError::Io("push_recv|io:ConnectionReset".into()).message_key(),
            "err.network_broken"
        );
        assert_eq!(
            CoreError::Io("io:connection refused".into()).message_key(),
            "err.network_refused"
        );
        assert_eq!(
            CoreError::Io("io:OutOfMemory".into()).message_key(),
            "err.io"
        );
    }

    #[test]
    fn stage_io_wraps_only_io_variant() {
        let staged = stage_io(CoreError::Io("io:TimedOut".into()), "hello_recv");
        assert!(matches!(staged, CoreError::Io(ref k) if k == "hello_recv|io:TimedOut"));
        // 非 io 变体原样穿透。
        assert!(matches!(
            stage_io(CoreError::SyncAborted, "hello_recv"),
            CoreError::SyncAborted
        ));
    }

    #[test]
    fn debug_prints_io_kind_but_not_other_payloads() {
        // Io：kind 可见（无隐私）。
        assert!(
            format!("{:?}", CoreError::Io("hello_recv|io:TimedOut".into()))
                .contains("hello_recv|io:TimedOut")
        );
        // 其它带载荷变体：仅错误码，载荷不出现在 Debug 里。
        assert_eq!(
            format!("{:?}", CoreError::Validation("secret-ish".into())),
            r#"CoreError("ERR_VALIDATION")"#
        );
    }
}
