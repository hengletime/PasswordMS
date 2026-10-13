//! 冲突箱模型（架构文档 §5.1、§3.9）。

use serde::{Deserialize, Serialize};

use crate::model::item::Item;

/// 冲突条目。两端版本快照均加密存储。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictEntry {
    pub conflict_id: String,
    /// 冲突对应的本机条目 ID。
    pub item_id: String,
    pub local_snapshot: Item,
    pub remote_snapshot: Item,
    pub local_updated_at: i64,
    pub remote_updated_at: i64,
    pub detected_at: i64,
    #[serde(default)]
    pub resolved: bool,
}

/// 冲突三选一仲裁选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictChoice {
    KeepLocal,
    TakeRemote,
    KeepBoth,
}
