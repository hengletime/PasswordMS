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

    /// 构造**外壳层**错误（错误码不属于密码库核心的错误，如热键/自启）。
    ///
    /// 为什么这些码不进 `CoreError`：核心库不该知道什么叫"快捷键"或"开机自启"——
    /// 那是本机外壳的 UI 概念。但错误码字符串仍是**同一张表**（见 [`shell_codes`]），
    /// 前端 `ErrorCode` 与之逐字对齐，契约测试守着。
    pub fn shell(code: &str, message_key: &str) -> Self {
        Self {
            code: code.to_string(),
            message_key: message_key.to_string(),
            record_count: None,
        }
    }
}

/// 外壳层错误码（非密码库核心语义）。
pub mod shell_codes {
    /// 热键键位串非法（缺修饰键、键名不认识、同一修饰键写两遍）。
    pub const ERR_HOTKEY_INVALID: &str = "ERR_HOTKEY_INVALID";
    /// 热键合法但注册不上：被其它程序占用或被系统策略拦下。
    pub const ERR_HOTKEY_UNAVAILABLE: &str = "ERR_HOTKEY_UNAVAILABLE";
    /// 开机自启开关写入失败（注册表被策略锁、UUID 缺失等）。
    pub const ERR_AUTOSTART_FAILED: &str = "ERR_AUTOSTART_FAILED";
    /// 该系统集成能力在当前平台不可用（移动端调用桌面专属命令）。
    ///
    /// 正常路径到不了：设置卡按 `PrefsDto.supported` 整块不渲染。纯防御。
    pub const ERR_NOT_SUPPORTED: &str = "ERR_NOT_SUPPORTED";
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
