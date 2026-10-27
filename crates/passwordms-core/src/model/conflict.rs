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

/// D-109/D-111：仲裁产生的同步状态修正清单项。
///
/// per-peer 的 `DeviceSyncState` 归同步会话层持有（O2 三段式内存副本），repo 层
/// 不持有权威副本，故仲裁对状态的影响以返回值上交，调用方（命令层/测试）对
/// **每个对端**的 state 应用：
///
/// - **水位回落**（TakeRemote，D-109）：落地 rev 可能低于本端曾发出过的版本 rev，
///   高挂的发送水位会把本端后续修改永久吞掉（rev ≤ 水位免重发）——
///   `sent_watermark[record_id] = min(现值, new_rev)`。
/// - **base 推进**（KeepLocal，D-111）：胜出版本 rev 抬升落地后，各对端 base 必须
///   推进到 `{new_rev, new_hash}`——否则双 KeepLocal 场景下两端互为「同 rev 异
///   内容」且各自偏离陈旧 base，每轮同步重新报冲突（新形态振荡）。
///   TakeRemote 的 base 在冲突发生时已推进为落地版本（同值），insert 幂等无害。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatermarkReset {
    pub record_id: String,
    pub new_rev: u64,
    /// D-111：落地/胜出版本的业务内容摘要（base 推进用）。
    pub new_hash: [u8; 32],
}
