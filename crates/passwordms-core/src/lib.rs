//! # passwordms-core
//!
//! PasswordMS 的纯逻辑核心：不依赖任何 GUI/外壳，可独立单测、可被任意前端外壳复用。
//!
//! 模块划分（架构文档 §4.3）：
//! - [`attachment_name`] 附件展示名解析（路径 / Android `content://` URI，D-66）
//! - [`error`]     统一错误类型与错误码
//! - [`constants`] 全部常量（KDF 参数、大小上限、超时、协议版本）
//! - [`securemem`] 敏感内存类型别名与清零辅助
//! - [`model`]     数据模型（Vault/Folder/Item/Trash/Conflict/SyncState）
//! - [`crypto`]    加密层（KDF / AEAD / 密钥层级 / CSPRNG / 验证）
//! - [`store`]     容器与仓储（单文件格式 + CRUD）
//! - [`session`]   会话生命周期（创建 / 解锁 / 锁定 / 改主密码）
//! - [`generator`] 密码生成器
//! - [`health`]    密码体检
//! - [`totp`]      TOTP（RFC 6238）
//! - [`import`]    Bitwarden 导入
//! - [`sync`]      局域网同步协议
//!
//! **安全基线**：禁用任何自研密码学；密钥与明文一律使用 [`zeroize::Zeroizing`]；
//! 日志与 `Debug` 不输出任何密钥或明文。

#![forbid(unsafe_code)]

pub mod attachment_name;
pub mod biometric;
pub mod constants;
pub mod crypto;
pub mod error;
pub mod generator;
pub mod health;
pub mod import;
pub mod model;
pub mod paths;
pub mod securemem;
pub mod session;
pub mod store;
pub mod sync;
pub mod totp;
pub mod url_safety;

pub use error::CoreError;
