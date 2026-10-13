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
}

// ===== 明文擦除（A-04）：`Drop` 只释放不覆写，需显式 `zeroize`。=====

impl Folder {
    /// 擦除分组的用户可辨识明文字段（名称 / 图标 / 颜色）。
    ///
    /// 依据 `01-security-req.md` §2.3：「分组名称、图标、颜色」应加密，避免元数据
    /// 泄露条目主题（如「银行」「公司 VPN」本身就说明问题）。A-04 要求锁定后内存中
    /// 无残留明文，故一并擦除。
    /// `sort_order` / `rev` / `updated_at` 为不可反推主题的数值元数据，保留不擦。
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
