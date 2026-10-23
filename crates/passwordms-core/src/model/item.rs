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

/// 附件元数据（D-60 §3.14.1：内嵌于父条目；`attachment_id` 同时是 blob 帧 record_id）。
///
/// 设计要点（§3.14.1）：
/// - `attachment_id`（UUID）**≠ 父 `item_id`**——同一 blob 帧可被多条目引用（KeepBoth
///   仲裁场景），附件删除不立即 tombstone blob，交孤儿 GC（§3.14.4）；
/// - `filename` 属用户私密（与原 `AttachmentData.original_filename` 同级），纳入 wipe；
/// - `mime_type` / 尺寸字段不含凭据，不擦（与原裁定一致）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentMeta {
    /// UUID；同时是 blob 帧的 record_id（16B）。
    pub attachment_id: String,
    /// 原始文件名（wipe_plaintext 覆盖）。
    pub filename: String,
    pub mime_type: String,
    /// 明文大小（字节）。
    pub size_bytes: u64,
    /// 密文占用（含 AEAD tag；容量统计口径）。
    pub blob_size_bytes: u64,
}

impl AttachmentMeta {
    /// 擦除原始文件名（A-04：与原 AttachmentData 同级私密）。
    pub fn wipe_plaintext(&mut self) {
        use zeroize::Zeroize;
        self.filename.zeroize();
    }
}

/// 条目类型枚举。内部标签 `type`，取值 `login` / `secure_note` / `attachment`。
///
/// **D-60 §3.14.1**：`Attachment` 变体**废弃**——附件改为父条目内嵌
/// `attachments` 数组（见 [`Item::attachments`]）。本变体仅保留**反序列化**能力
/// 供存量库迁移读取（`migrate_legacy_attachments`，§3.14.3），不得再新建。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemKind {
    Login(LoginData),
    SecureNote(SecureNoteData),
    #[deprecated(since = "0.1.0", note = "D-60: attachments moved to Item.attachments")]
    Attachment(AttachmentData),
}

// 下述四个方法需匹配 `Attachment` 遗留变体（迁移读取兼容，D-60）——整块豁免弃用告警。
#[allow(deprecated)]
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
    /// 附属附件元数据（D-60 §3.14.1；随父条目密文帧过线，自动获得 E2E/rev/墓碑语义）。
    /// `#[serde(default)]` 兼容旧帧（无此字段反序列化为空数组）。
    #[serde(default)]
    pub attachments: Vec<AttachmentMeta>,
    /// 是否钉住（#531：列表排序的第一段 = 钉住块；拖拽置顶即钉住）。
    #[serde(default)]
    pub pinned: bool,
    /// 钉住块内的顺序（小在前）。未钉住条目恒为 0，不参与排序。
    #[serde(default)]
    pub pin_order: i64,
    /// 使用频度（#531：仅「复制」动作计数，同条目 60s 节流，见
    /// [`crate::constants::USAGE_THROTTLE_MS`]）。未钉住段的主排序键（DESC）。
    #[serde(default)]
    pub use_count: u64,
    /// 最近一次计频时间（节流判定与频度段的次级排序键；`None` 视为 0）。
    #[serde(default)]
    pub last_used_at: Option<i64>,
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
