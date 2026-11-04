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
    /// 自动输入（1.5.39）：目标窗口焦点丢失——保存的句柄失效 / `SetForegroundWindow`
    /// 失败 / 切回后前台不是目标 / **注入中途前台变了**（立即中止，剩余序列不发）。
    ///
    /// 与前端 `autotype.ts` 的 `AUTOTYPE_FAILURE_KEY_BY_CODE` 逐字对齐：前端只认
    /// 这一个码（统一显示「目标窗口已切换，已停止输入」），不区分失焦发生在
    /// 注入前还是注入中——对用户那是同一句话。
    pub const ERR_AUTOTYPE_FOCUS_LOST: &str = "ERR_AUTOTYPE_FOCUS_LOST";
    /// 自动输入（1.5.39 增强）：`username` 动作要求条目有非空用户名。
    pub const ERR_AUTOTYPE_NO_USERNAME: &str = "ERR_AUTOTYPE_NO_USERNAME";
    /// 自动输入（1.5.39 增强）：`url` 动作要求条目有非空网址。
    pub const ERR_AUTOTYPE_NO_URL: &str = "ERR_AUTOTYPE_NO_URL";
    /// 自动输入（1.5.39 增强）：`totp` 动作要求条目配置了 TOTP 密钥。
    pub const ERR_AUTOTYPE_NO_TOTP: &str = "ERR_AUTOTYPE_NO_TOTP";
    /// 自动输入（1.5.41）：**目标进程提权**——注入前就能确定打不进去。
    ///
    /// 与 [`ERR_AUTOTYPE_BLOCKED`] 分开，理由有两条：
    /// 1. **给用户的建议相反**。这条可以照做（以管理员身份运行 PasswordMS，或者手动
    ///    复制粘贴）；"被拦"那条是注入途中的意外，用户无从下手。
    /// 2. **时序不同**。这条在 `hide()` **之前**就返回，所以错误能显示在还看得见的
    ///    浮层里；`BLOCKED` 只能发生在 hide 之后。
    ///
    /// 为什么非得"注入前"判：跨完整性注入时 `SendInput` **返回成功**（事件进了系统
    /// 队列，被 UIPI 在更底层丢弃），返回值根本不指示有没有真打进去。主人 2026-11-04
    /// 在管理员 PowerShell 里实测：一个字没打进去，日志里连一条失败都没有——
    /// 1.5.39 的"检查 SendInput 返回值"因此形同虚设。
    pub const ERR_AUTOTYPE_TARGET_ELEVATED: &str = "ERR_AUTOTYPE_TARGET_ELEVATED";
    /// 自动输入（1.5.39 增强 / 安全审查 M2）：**按键被系统拦下**——`SendInput`
    /// 返回 0（不足请求的事件数）。
    ///
    /// 最常见的原因是 UIPI：目标窗口属于**更高完整性级别**的进程（以管理员身份
    /// 运行的程序），低完整性进程的输入注入会被系统静默丢弃。桌面已锁定/输入桌面
    /// 被切换时也是这个返回。
    ///
    /// 为什么**不复用** [`ERR_AUTOTYPE_FOCUS_LOST`]：两者要用户做的事完全不同——
    /// 焦点丢了他点回去重试就行；被拦下他得关掉目标程序的管理员权限（或改用复制
    /// 粘贴）。合成一个码等于给"重试一次"这条错误建议。
    pub const ERR_AUTOTYPE_BLOCKED: &str = "ERR_AUTOTYPE_BLOCKED";
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
