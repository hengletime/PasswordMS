//! 冲突检测与合并（架构文档 §3.9，Q3【已裁定】）。
//!
//! 条目级 LWW 推进；**双方自上次同步后都改过同一条目且内容不同**时保留冲突副本进「冲突箱」，
//! 不静默覆盖，由 UI 三选一仲裁。

use crate::constants::{RECORD_TYPE_FOLDER, RECORD_TYPE_ITEM};
use crate::error::CoreResult;
use crate::model::conflict::ConflictEntry;
use crate::model::item::Item;
use crate::model::now_ms;
use crate::model::sync_state::{BaseRev, DeviceSyncState};
use crate::store::repository::{folder_hash, hash32, item_hash, Repository};
use crate::sync::protocol::ChangeRecord;

/// 合并结果。
#[derive(Debug, Clone, Default)]
pub struct MergeReport {
    pub applied: usize,
    pub conflicts: Vec<String>,
    pub skipped: usize,
}

/// 「双改」冲突探测：基准存在且两端 `rev` 均大于基准，且内容摘要不同。
pub fn detect_conflict(local: &Item, remote: &Item, base: Option<&BaseRev>) -> CoreResult<bool> {
    let base = match base {
        Some(b) => b,
        // 无基准：首次见到该条目，不构成冲突。
        None => return Ok(false),
    };
    let local_changed = local.rev > base.rev;
    let remote_changed = remote.rev > base.rev;
    if !(local_changed && remote_changed) {
        return Ok(false);
    }
    let local_hash = item_hash(local)?;
    let remote_hash = item_hash(remote)?;
    // 双端都改但内容一致 → 视为无冲突。
    Ok(local_hash != remote_hash)
}

/// 应用一批远端变更（含条目与分组），并按需写入冲突箱。
///
/// D-56 删除传播语义（§3.13.2，含实现补注）：
/// - **幂等判重**：`base.rev > change.rev`，或 `base.rev == change.rev` 且 base 为墓碑
///   标记（`hash == [0;32]`）→ 已应用过，跳过。设计伪码的 `base.rev >= change.rev`
///   须按 hash 细化：base 与墓碑同 rev 但 hash 非零 = 本端持有同 rev **活跃条目**，
///   该墓碑是**首次到达**，不得跳过（否则常规流「同步 rev1 → 删 rev1」永远不过线）；
/// - **删除-修改拉锯**：本端条目 rev 更高 → 墓碑密文解出死亡前快照入冲突箱
///   （`remote_deleted = true`，仲裁「采用远端」= 接受删除走软删）；
/// - **常规远端删除**：软删（`trash_item_direct`，blob 保留）；
/// - **D-98 purge 传播（C 案）**：本端无活跃条目且持有 rev ≤ 墓碑的回收站快照 →
///   顺带彻底删除该快照——对端 `purge_item` 以死亡快照 rev+1 重封 ITEM 墓碑过线
///   （见 `Repository::purge_item` / `purge_trash_snapshots`），快照 rev 更新者保留；
/// - **复活抑制**：本端无活跃条目但持有 `rev >= change.rev` 的墓碑 → 对端 live 变更
///   已陈旧，不得复活（否则「采用远端」删除仲裁会被对端陈旧 live 变更回滚）；
/// - **base 推进**：无论何分支，base 推进到本变更 rev。
pub fn apply_changes(
    repo: &mut Repository,
    state: &mut DeviceSyncState,
    changes: &[ChangeRecord],
) -> CoreResult<MergeReport> {
    let mut report = MergeReport::default();

    for change in changes {
        match change.record_type {
            RECORD_TYPE_ITEM => {
                if change.deleted {
                    // D-56：幂等判重（墓碑标记 hash=0 才代表「该墓碑已应用」）。
                    if let Some(base) = state.base.get(&change.record_id) {
                        if base.rev > change.rev
                            || (base.rev == change.rev && base.hash == [0u8; 32])
                        {
                            report.skipped += 1;
                            continue;
                        }
                    }
                    match repo.get_item_opt(&change.record_id) {
                        // 删除-修改拉锯：本端更新 → 死亡快照入冲突箱（UI 三选一）。
                        Some(local) if local.rev > change.rev => {
                            let remote_snapshot = repo.change_to_item(change)?;
                            let entry = ConflictEntry {
                                conflict_id: uuid::Uuid::new_v4().to_string(),
                                item_id: change.record_id.clone(),
                                local_updated_at: local.updated_at,
                                remote_updated_at: remote_snapshot.updated_at,
                                local_snapshot: local,
                                remote_snapshot,
                                detected_at: now_ms(),
                                resolved: false,
                                remote_deleted: true,
                            };
                            repo.add_conflict(entry)?;
                            report.conflicts.push(change.record_id.clone());
                        }
                        // 常规远端删除 → 软删（blob 保留，回收站可还原）。
                        Some(local) => {
                            repo.trash_item_direct(&local)?;
                            report.applied += 1;
                        }
                        // 本端无活跃条目：
                        // - 持有回收站快照（rev ≤ 墓碑 rev）+ 对端更高 rev 墓碑 =
                        //   **D-98（C 案）对端已彻底删除** → 顺带清理本端回收站快照
                        //   （删除意图跨端收敛；blob 引用收窄交孤儿 GC）；
                        // - 快照 rev 更高（本端删了更新版本）→ 保留，待自身墓碑过线；
                        // - 两者皆无（从未见过）→ 仅推进 base。
                        None => {
                            let _purged =
                                repo.purge_trash_snapshots(&change.record_id, change.rev)?;
                            report.applied += 1;
                        }
                    }
                    state.base.insert(
                        change.record_id.clone(),
                        BaseRev {
                            rev: change.rev,
                            hash: [0u8; 32],
                        },
                    );
                    continue;
                }
                let remote = repo.change_to_item(change)?;
                match repo.get_item_opt(&change.record_id) {
                    Some(local) => {
                        let base = state.base.get(&change.record_id).cloned();
                        if detect_conflict(&local, &remote, base.as_ref())? {
                            let entry = ConflictEntry {
                                conflict_id: uuid::Uuid::new_v4().to_string(),
                                item_id: change.record_id.clone(),
                                local_snapshot: local.clone(),
                                remote_snapshot: remote.clone(),
                                local_updated_at: local.updated_at,
                                remote_updated_at: remote.updated_at,
                                detected_at: now_ms(),
                                resolved: false,
                                remote_deleted: false,
                            };
                            repo.add_conflict(entry)?;
                            report.conflicts.push(change.record_id.clone());
                            // 保留本机为正式条目，基准推进到远端 rev（后续仲裁按 UI 决定）。
                            state.base.insert(
                                change.record_id.clone(),
                                BaseRev {
                                    rev: remote.rev,
                                    hash: item_hash(&remote)?,
                                },
                            );
                            continue;
                        }
                        // 无冲突：LWW 应用。
                        let take_remote = remote.updated_at > local.updated_at
                            || (remote.updated_at == local.updated_at && remote.rev > local.rev);
                        if take_remote {
                            repo.put_item_direct(remote.clone())?;
                            report.applied += 1;
                        } else {
                            report.skipped += 1;
                        }
                        state.base.insert(
                            change.record_id.clone(),
                            BaseRev {
                                rev: remote.rev.max(local.rev),
                                hash: item_hash(&remote)?,
                            },
                        );
                    }
                    None => {
                        // D-56 复活抑制：本端已删（持有 rev ≥ 变更的墓碑）→ 陈旧 live
                        // 变更不得复活条目；否则正常 put。
                        let suppressed = matches!(
                            repo.tombstone_rev(RECORD_TYPE_ITEM, &change.record_id),
                            Some(tomb_rev) if tomb_rev >= change.rev
                        );
                        if suppressed {
                            report.skipped += 1;
                        } else {
                            repo.put_item_direct(remote.clone())?;
                            // D-91（审查 R2 根因修除）：复活成功即清除同 item_id 的
                            // 旧回收站快照——否则旧快照与 live 新版本并存，用户「还原」
                            // 会用旧数据覆盖刚同步来的新版本并传播回对端（两端丢数据）。
                            repo.drop_trash_snapshots(&change.record_id)?;
                            report.applied += 1;
                        }
                        state.base.insert(
                            change.record_id.clone(),
                            BaseRev {
                                rev: remote.rev,
                                hash: item_hash(&remote)?,
                            },
                        );
                    }
                }
            }
            RECORD_TYPE_FOLDER => {
                if change.deleted {
                    // D-56：幂等判重同 ITEM；只删分组壳**无级联**（其下条目的删除由
                    // 对端逐条 item 墓碑自然过线）。
                    if let Some(base) = state.base.get(&change.record_id) {
                        if base.rev > change.rev
                            || (base.rev == change.rev && base.hash == [0u8; 32])
                        {
                            report.skipped += 1;
                            continue;
                        }
                    }
                    repo.remove_folder_direct(&change.record_id)?;
                    state.base.insert(
                        change.record_id.clone(),
                        BaseRev {
                            rev: change.rev,
                            hash: [0u8; 32],
                        },
                    );
                    report.applied += 1;
                    continue;
                }
                let remote = repo.change_to_folder(change)?;
                // D-56 复活抑制（分组壳同条目语义）。
                let suppressed = matches!(
                    repo.tombstone_rev(RECORD_TYPE_FOLDER, &change.record_id),
                    Some(tomb_rev) if tomb_rev >= change.rev
                );
                if suppressed {
                    report.skipped += 1;
                } else {
                    repo.put_folder_direct(remote.clone())?;
                    report.applied += 1;
                }
                state.base.insert(
                    change.record_id.clone(),
                    BaseRev {
                        rev: remote.rev,
                        hash: folder_hash(&remote)?,
                    },
                );
            }
            _ => {
                report.skipped += 1;
            }
        }
    }

    Ok(report)
}

/// 发送侧墓碑基准推进（D-56 §3.13.3「免重发」的落点）。
///
/// `export_changes` 导出的墓碑在**发送成功后**由各调用点调用本函数推进本端对该对端的
/// base —— 下一轮导出的 peer 过滤（`base.rev >= 墓碑 rev` 不导出）即生效，实现
/// 「连续两次同步第二次墓碑零重发」。仅推进 `deleted` 变更（live 幂等由对端 LWW 保证）。
///
/// 失败安全：若本轮同步中途失败，`save_sync_state` 不会执行，base 推进随会话丢弃，
/// 下一轮自动重发墓碑（宁多勿丢；对端按判重规则幂等跳过）。
pub fn note_sent_tombstones(state: &mut DeviceSyncState, changes: &[ChangeRecord]) {
    for change in changes.iter().filter(|c| c.deleted) {
        state.base.insert(
            change.record_id.clone(),
            BaseRev {
                rev: change.rev,
                hash: [0u8; 32],
            },
        );
    }
}

/// O1：发送侧 live 水位推进。
///
/// 与 [`note_sent_tombstones`] 同一失败安全语义：各交换点在 Push 页**发送成功后**
/// 调用本函数推进 `sent_watermark`，下一轮 `export_changes` 的水位过滤即生效。
/// 仅推进 `!deleted` 变更（墓碑走 base 过滤）。失败路径水位随会话丢弃，下一轮
/// 自动重发（宁多勿丢；对端 merge 按 rev/判重规则幂等跳过）。
pub fn note_sent_changes(state: &mut DeviceSyncState, changes: &[ChangeRecord]) {
    for change in changes.iter().filter(|c| !c.deleted) {
        let entry = state
            .sent_watermark
            .entry(change.record_id.clone())
            .or_insert(0);
        if change.rev > *entry {
            *entry = change.rev;
        }
    }
}

/// O1：接收侧水位推进——**回声消除**。
///
/// 对端主动发来的 live 变更本身即证明「对端已持有该 rev」，本端对它的回发水位
/// 可同步推进：否则「刚从对端收到的条目」每轮都会被原样回发（全量导出时代的
/// 固有回声流量，库越大越浪费）。墓碑无需此处理——`apply_changes` 收到删除时
/// 已把 base 推进为墓碑标记（hash=0），导出侧 base 过滤天然消除回声。
///
/// 失败安全与 [`note_sent_changes`] 相同：仅随本轮交换成功（`save_sync_state`）落盘。
pub fn note_received_changes(state: &mut DeviceSyncState, changes: &[ChangeRecord]) {
    note_sent_changes(state, changes);
}

/// 推进某条记录的同步基准。
pub fn advance_state(state: &mut DeviceSyncState, record_id: &str, rev: u64, hash: [u8; 32]) {
    state
        .base
        .insert(record_id.to_string(), BaseRev { rev, hash });
}

/// 由原始密文计算基准摘要（用于墓碑等无明文场景）。
pub fn base_hash_of(bytes: &[u8]) -> [u8; 32] {
    hash32(bytes)
}
