//! 冲突箱模型（架构文档 §5.1、§3.9）。

use serde::{Deserialize, Serialize};

use crate::model::folder::Folder;
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

/// 分组冲突条目【D-171，Kimi 审计 F-01】。
///
/// 与条目冲突同构，但**只有两项仲裁**：分组没有「两者都留」的语义——复制一个分组
/// 壳（名字加后缀）既不能让用户分辨哪份是真的，也不会带上其下条目（条目按
/// `folder_id` 各自有记录），凭空多一个空分组只是噪音。`KeepBoth` 由
/// `Repository::resolve_conflict` 显式拒绝（`folder_conflict_no_keep_both`），
/// UI 侧按 `record_type` 只渲染两个按钮。
///
/// `remote_deleted = true` 时 `remote_snapshot` 是**删除前的死亡快照**（与条目同一
/// 约定），「采用远端」= 接受对端的删除。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderConflictEntry {
    pub conflict_id: String,
    pub folder_id: String,
    pub local_snapshot: Folder,
    pub remote_snapshot: Folder,
    pub local_updated_at: i64,
    pub remote_updated_at: i64,
    pub detected_at: i64,
    #[serde(default)]
    pub resolved: bool,
    #[serde(default)]
    pub remote_deleted: bool,
}

/// 冲突箱的统一视图（条目 + 分组），供 UI / DTO 按同一时间线展示。
///
/// 仓储里两类冲突分两张表存（记录类型不同 → 载荷结构不同，见
/// `RECORD_TYPE_FOLDER_CONFLICT` 的注释），但**对用户只有一个冲突箱**：
/// 同一屏、按 `detected_at` 倒序、每条标明是条目还是分组。
///
/// 两侧都用 `Box` 装：`ConflictEntry` 内嵌两份 `Item` 快照（≥700 字节），直接做变体
/// 会让枚举按最大变体定尺寸，分组冲突也得背这份体积（clippy::large_enum_variant）。
/// 冲突箱本来就是一屏几十条的量级，多一次堆分配换「尺寸不随最大变体膨胀」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictView {
    Item(Box<ConflictEntry>),
    Folder(Box<FolderConflictEntry>),
}

impl ConflictView {
    pub fn conflict_id(&self) -> &str {
        match self {
            Self::Item(e) => &e.conflict_id,
            Self::Folder(e) => &e.conflict_id,
        }
    }

    pub fn detected_at(&self) -> i64 {
        match self {
            Self::Item(e) => e.detected_at,
            Self::Folder(e) => e.detected_at,
        }
    }
}

/// 冲突三选一仲裁选项（分组冲突只接受前两项，见 [`FolderConflictEntry`]）。
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
