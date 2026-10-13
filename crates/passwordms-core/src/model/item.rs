//! 条目模型（架构文档 §5.1）。全部敏感字段处于 AEAD 密文内。

use serde::{Deserialize, Serialize};

/// 自定义键值对。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomField {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub hidden: bool,
}

/// 登录型条目数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginData {
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    /// Base32 TOTP 密钥（F-11）。
    #[serde(default)]
    pub totp_secret: Option<String>,
    #[serde(default)]
    pub custom_fields: Vec<CustomField>,
}

/// 安全笔记数据（F-13）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecureNoteData {
    pub title: String,
    pub body: String,
}

/// 加密附件数据（F-12）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentData {
    pub title: String,
    pub original_filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    /// 密文 blob 引用（指向库内 blob 帧的 record_id）。
    pub blob_ref: String,
    pub blob_size_bytes: u64,
}

/// 条目类型枚举。内部标签 `type`，取值 `login` / `secure_note` / `attachment`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemKind {
    Login(LoginData),
    SecureNote(SecureNoteData),
    Attachment(AttachmentData),
}

impl ItemKind {
    /// 条目标题（各子类型均有）。
    pub fn title(&self) -> &str {
        match self {
            ItemKind::Login(d) => &d.title,
            ItemKind::SecureNote(d) => &d.title,
            ItemKind::Attachment(d) => &d.title,
        }
    }

    /// 覆盖标题。
    pub fn set_title(&mut self, title: String) {
        match self {
            ItemKind::Login(d) => d.title = title,
            ItemKind::SecureNote(d) => d.title = title,
            ItemKind::Attachment(d) => d.title = title,
        }
    }

    /// 类型标识（与 IPC `type` 字段一致）。
    pub fn type_name(&self) -> &'static str {
        match self {
            ItemKind::Login(_) => "login",
            ItemKind::SecureNote(_) => "secure_note",
            ItemKind::Attachment(_) => "attachment",
        }
    }

    /// 显式擦除该条目内的全部用户明文字段（A-04 / 安全需求 §3.2）。
    ///
    /// 由 [`crate::store::repository::Repository::wipe_plaintext`] 在锁定时调用。
    pub fn wipe_plaintext(&mut self) {
        match self {
            ItemKind::Login(d) => d.wipe_plaintext(),
            ItemKind::SecureNote(d) => d.wipe_plaintext(),
            ItemKind::Attachment(d) => d.wipe_plaintext(),
        }
    }
}

/// 条目基类。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub item_id: String,
    pub kind: ItemKind,
    #[serde(default)]
    pub folder_id: Option<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub rev: u64,
    #[serde(default)]
    pub modified_by: String,
}

impl Item {
    /// 便捷取标题。
    pub fn title(&self) -> &str {
        self.kind.title()
    }
}

// ===== 明文擦除（A-04）：`Drop` 只释放不覆写，需显式 `zeroize`。=====

impl CustomField {
    /// 擦除自定义字段的键与值（键可能暗示用途，值即凭据）。
    pub fn wipe_plaintext(&mut self) {
        use zeroize::Zeroize;
        self.key.zeroize();
        self.value.zeroize();
    }
}

impl LoginData {
    /// 擦除登录条目的全部用户明文字段。
    ///
    /// 覆盖 `title` / `url` / `username` / `password` / `notes` / `totp_secret` 及自定义字段：
    /// 其中 `title`/`url`/`username` 虽非「密码本体」，但「访问了哪些站点、使用了哪个账号」
    /// 同属用户私密信息，A-04 要求锁定时清空内存中的敏感数据，故一并擦除。
    pub fn wipe_plaintext(&mut self) {
        use zeroize::Zeroize;
        self.title.zeroize();
        self.url.zeroize();
        self.username.zeroize();
        self.password.zeroize();
        self.notes.zeroize();
        self.totp_secret.zeroize();
        for field in self.custom_fields.iter_mut() {
            field.wipe_plaintext();
        }
    }
}

impl SecureNoteData {
    /// 擦除笔记条目的标题与正文。
    pub fn wipe_plaintext(&mut self) {
        use zeroize::Zeroize;
        self.title.zeroize();
        self.body.zeroize();
    }
}

impl AttachmentData {
    /// 擦除附件条目的标题与原始文件名。
    ///
    /// `mime_type`（类型推断结果）、`blob_ref`（随机内部句柄）与尺寸字段不含凭据、
    /// 无法反推敏感内容，故保留不擦，以免影响解密后的正常渲染与引用。
    pub fn wipe_plaintext(&mut self) {
        use zeroize::Zeroize;
        self.title.zeroize();
        self.original_filename.zeroize();
    }
}
