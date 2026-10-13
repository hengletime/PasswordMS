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
use crate::store::repository::{folder_hash, item_hash, hash32, Repository};
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
                    repo.remove_item_direct(&change.record_id)?;
                    state
                        .base
                        .insert(change.record_id.clone(), BaseRev { rev: change.rev, hash: [0u8; 32] });
                    report.applied += 1;
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
                            };
                            repo.add_conflict(entry)?;
                            report.conflicts.push(change.record_id.clone());
                            // 保留本机为正式条目，基准推进到远端 rev（后续仲裁按 UI 决定）。
                            state
                                .base
                                .insert(change.record_id.clone(), BaseRev { rev: remote.rev, hash: item_hash(&remote)? });
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
                            BaseRev { rev: remote.rev.max(local.rev), hash: item_hash(&remote)? },
                        );
                    }
                    None => {
                        repo.put_item_direct(remote.clone())?;
                        state
                            .base
                            .insert(change.record_id.clone(), BaseRev { rev: remote.rev, hash: item_hash(&remote)? });
                        report.applied += 1;
                    }
                }
            }
            RECORD_TYPE_FOLDER => {
                if change.deleted {
                    repo.remove_folder_direct(&change.record_id)?;
                    state
                        .base
                        .insert(change.record_id.clone(), BaseRev { rev: change.rev, hash: [0u8; 32] });
                    report.applied += 1;
                    continue;
                }
                let remote = repo.change_to_folder(change)?;
                repo.put_folder_direct(remote.clone())?;
                state.base.insert(
                    change.record_id.clone(),
                    BaseRev { rev: remote.rev, hash: folder_hash(&remote)? },
                );
                report.applied += 1;
            }
            _ => {
                report.skipped += 1;
            }
        }
    }

    Ok(report)
}

/// 推进某条记录的同步基准。
pub fn advance_state(state: &mut DeviceSyncState, record_id: &str, rev: u64, hash: [u8; 32]) {
    state.base.insert(record_id.to_string(), BaseRev { rev, hash });
}

/// 由原始密文计算基准摘要（用于墓碑等无明文场景）。
pub fn base_hash_of(bytes: &[u8]) -> [u8; 32] {
    hash32(bytes)
}
