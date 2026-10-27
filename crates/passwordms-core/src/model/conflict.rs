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
    /// D-56：远端版本是一条**删除墓碑**（remote_snapshot 为死亡前快照）。
    /// 「采用远端」仲裁时按接受删除处理（本端软删），而非复活死亡快照。
    #[serde(default)]
    pub remote_deleted: bool,
}

/// 冲突三选一仲裁选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictChoice {
    KeepLocal,
    TakeRemote,
    KeepBoth,
}

/// D-109：仲裁产生的水位回落清单项。
///
/// TakeRemote **原样落地**对方快照后，落地 rev 可能低于本端曾向某对端发出过的
/// 版本 rev——per-peer 的发送水位若高挂不落，本端后续修改会被「rev ≤ 水位免重发」
/// 永久吞掉。调用方（命令层/同步会话层）须对**每个对端**的
/// `DeviceSyncState.sent_watermark[record_id]` 做 `min(现值, new_rev)` 应用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatermarkReset {
    pub record_id: String,
    pub new_rev: u64,
}
