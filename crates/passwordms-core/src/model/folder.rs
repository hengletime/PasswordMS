//! 分组模型（架构文档 §5.1）。整条内容均加密。

use serde::{Deserialize, Serialize};

/// 分组。`folder_id` 位于帧头明文，其余字段处于 AEAD 密文内。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub folder_id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub rev: u64,
    #[serde(default)]
    pub updated_at: i64,
    /// 是否钉住（#531：分组列表排序第一段 = 钉住块；拖拽置顶即钉住）。
    #[serde(default)]
    pub pinned: bool,
    /// 钉住块内的顺序（小在前）。未钉住分组恒为 0。
    ///
    /// 注意：`sort_order` 自 #531 起不再被 `list_folders` 的排序消费（字段与
    /// `reorder_folders` 命令保留仅为兼容），新排序见 `Repository::list_folders`。
    #[serde(default)]
    pub pin_order: i64,
}

// ===== 明文擦除（A-04）：`Drop` 只释放不覆写，需显式 `zeroize`。=====

impl Folder {
    /// 擦除分组的用户可辨识明文字段（名称 / 图标 / 颜色）。
    ///
    /// 依据 `01-security-req.md` §2.3：「分组名称、图标、颜色」应加密，避免元数据
    /// 泄露条目主题（如「银行」「公司 VPN」本身就说明问题）。A-04 要求锁定后内存中
    /// 无残留明文，故一并擦除。
    /// `sort_order` / `rev` / `updated_at` / `pinned` / `pin_order` 为不可反推主题的
    /// 数值元数据，保留不擦。
    pub fn wipe_plaintext(&mut self) {
        use zeroize::Zeroize;
        self.name.zeroize();
        if let Some(icon) = self.icon.as_mut() {
            icon.zeroize();
        }
        if let Some(color) = self.color.as_mut() {
            color.zeroize();
        }
    }
}
