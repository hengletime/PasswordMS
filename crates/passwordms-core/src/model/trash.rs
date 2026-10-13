//! 回收站模型（架构文档 §5.1）。快照整体加密。

use serde::{Deserialize, Serialize};

use crate::model::item::Item;

/// 回收站条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashEntry {
    pub trash_id: String,
    pub item_snapshot: Item,
    #[serde(default)]
    pub original_folder_id: Option<String>,
    pub deleted_at: i64,
    pub expire_at: i64,
}
