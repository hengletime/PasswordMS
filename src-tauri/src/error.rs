//! IPC 错误：`CoreError` → `IpcError{code,message_key}`（架构文档 §5.2、§9.2）。

use serde::Serialize;

use passwordms_core::CoreError;

/// 回传给前端的错误 JSON。
#[derive(Debug, Clone, Serialize)]
pub struct IpcError {
    pub code: String,
    pub message_key: String,
    /// 附加数值（如 `ERR_VK_ADOPT_REQUIRES_EMPTY` 的本地记录数），仅少量场景使用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_count: Option<usize>,
}

impl IpcError {
    /// 构造内部错误。
    pub fn internal(_detail: &str) -> Self {
        Self {
            code: CoreError::Internal(String::new()).code().to_string(),
            message_key: CoreError::Internal(String::new()).message_key().to_string(),
            record_count: None,
        }
    }

    /// 构造 IO 错误。
    pub fn io(_detail: &str) -> Self {
        Self {
            code: CoreError::Io(String::new()).code().to_string(),
            message_key: CoreError::Io(String::new()).message_key().to_string(),
            record_count: None,
        }
    }
}

impl From<CoreError> for IpcError {
    fn from(err: CoreError) -> Self {
        let record_count = match &err {
            CoreError::VkAdoptRequiresEmpty { record_count } => Some(*record_count),
            _ => None,
        };
        Self {
            code: err.code().to_string(),
            message_key: err.message_key().to_string(),
            record_count,
        }
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(_err: serde_json::Error) -> Self {
        IpcError::from(CoreError::Internal("json".to_string()))
    }
}

impl From<std::io::Error> for IpcError {
    fn from(err: std::io::Error) -> Self {
        IpcError::from(CoreError::from(err))
    }
}

/// 互斥锁中毒 → 内部错误（不在日志中记录任何参数值）。
pub fn lock_err<T>(_err: std::sync::PoisonError<T>) -> IpcError {
    IpcError::internal("lock_poisoned")
}

/// IPC 结果别名。
pub type IpcResult<T> = Result<T, IpcError>;
