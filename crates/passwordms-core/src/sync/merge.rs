//! 冲突检测与合并（架构文档 §3.9，Q3【已裁定】）。
//!
//! 条目级 LWW 推进；**双方自上次同步后都改过同一条目且内容不同**时保留冲突副本进「冲突箱」，
//! 不静默覆盖，由 UI 三选一仲裁。

use crate::constants::{RECORD_TYPE_FOLDER, RECORD_TYPE_ITEM};
use crate::error::CoreResult;
use crate::model::conflict::{ConflictEntry, FolderConflictEntry};
use crate::model::folder::Folder;
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
    Ok(detect_change_conflict(
        local.rev,
        remote.rev,
        item_hash(local)?,
        item_hash(remote)?,
        base,
    ))
}

/// 分组版「双改」探测（D-171）。
///
/// 判定逻辑与条目**共用同一个函数**（[`detect_change_conflict`]），只有哈希口径不同——
/// 分组用 [`folder_hash`]、条目用 [`item_hash`]。共用是为了避免「条目修好了、分组还
/// 留着一份旧判定」这类只差一行的漏洞（D-170 的 F-02 就是这么来的）。
pub fn detect_folder_conflict(
    local: &Folder,
    remote: &Folder,
    base: Option<&BaseRev>,
) -> CoreResult<bool> {
    Ok(detect_change_conflict(
        local.rev,
        remote.rev,
        folder_hash(local)?,
        folder_hash(remote)?,
        base,
    ))
}

/// D-205：本端已删 + 远端 live 变更是否构成 delete-vs-update 冲突。
///
/// **为什么不能复用既有的「等 rev 抑制」**：`Repository::tombstone()` 只翻墓碑标志位，
/// **不推进 `rev` / `updated_at`**（payload 原样保留），所以墓碑的 rev 恒等于删除前
/// 最后一版的 rev。抑制条件是 `tomb_rev >= change.rev`；而真冲突的定义恰恰相反——
/// 远端在本端删除之后又改过一轮，`change.rev > tomb_rev`。两者互补：对真冲突，
/// 抑制**永远为假**，于是旧实现直接 `put_folder_direct` 把远端改名当成新版本落地，
/// 本端的删除被静默复活（存活方先发起同步时结构性必现，见
/// `src-tauri/src/sync/server.rs` 先 `apply_changes` 后 `export_changes` 的调用顺序）。
///
/// 返回 `Some(dead)` = 构成冲突，`dead` 是墓碑里的死亡快照（删除前的形态）；
/// `None` = 无墓碑，或墓碑不旧于远端（回声 / 远端变更比删除旧）→ 走原有落地路径。
fn deleted_local_folder_conflict(repo: &Repository, change: &ChangeRecord) -> Option<Folder> {
    let dead = repo.tombstone_folder(&change.record_id)?;
    (change.rev > dead.rev).then_some(dead)
}

/// 「双改」判定的本体：只看 rev 与内容摘要，与记录类型无关。
fn detect_change_conflict(
    local_rev: u64,
    remote_rev: u64,
    local_hash: [u8; 32],
    remote_hash: [u8; 32],
    base: Option<&BaseRev>,
) -> bool {
    match base {
        // D-109：无基准（本端从未收过该对端的此记录）时，同 rev 异内容必为双端
        // 独立修改——单侧修改的传播会随接收建立 base，不会停留在无基准态；同 id
        // 同 rev 异内容不可能是单侧传播（传播版本单调同源）。此时若走无基准 LWW，
        // 后改一端会静默覆盖先改一端（Q3 裁定禁止静默覆盖），故直接判冲突交 UI 仲裁。
        None => local_rev == remote_rev && local_hash != remote_hash,
        Some(b) => {
            // 基准存在：两端都推进过才可能是「双改」；单侧推进是无冲突的常规传播。
            if !(local_rev > b.rev && remote_rev > b.rev) {
                return false;
            }
            // 双端都改但内容一致 → 视为无冲突。
            local_hash != remote_hash
        }
    }
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
/// - **删除-改名拉锯（D-205，分组）**：本端已删（持墓碑）而远端 live 变更 rev 更高
///   → 墓碑里的死亡快照与远端活版本入冲突箱（`local_deleted = true`），**不落地、
///   不推进 base**；`KeepLocal` = 确认删除，`TakeRemote` = 接受复活；
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
                            // D-111：条目被更新版本覆盖后，挂着的旧冲突快照已过时
                            // （继续仲裁只会把过时版本写回去）→ 事实解决收口。
                            // report.conflicts 记录的是本轮新建冲突（本条未冲突才
                            // 走到这），与过期对象不相交，无需回改报告。
                            repo.expire_conflicts_for(&change.record_id)?;
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
                        //
                        // TODO(D-205 边界)：条目侧同型洞见 docs/02 §3.47.5
                        // 条目侧这里也有「本端已彻底删除（无活跃条目、持墓碑）+ 远端
                        // 更晚改名 → 静默复活」的同型洞，但本次**不修**：
                        // 1. 条目删除先入回收站，回收站条目仍算活跃，「本端无活跃条目」
                        //    只发生在用户**彻底清除**之后（触发面窄于分组）；
                        // 2. 条目冲突是三选一（含 KeepBoth），删除-复活要重新裁定
                        //    「保留本机副本」的语义，属于独立的语义设计，不在 D-205
                        //    这个「分组 delete-vs-update」的修复范围内扩围。
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
                            // D-111：复活同样令挂着的旧冲突快照过时（如对端仲裁
                            // KeepLocal 的胜出版本传回），事实解决收口。
                            repo.expire_conflicts_for(&change.record_id)?;
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
                    // D-171 删除-修改拉锯（分组版，镜像条目 D-56）：本端 rev 更高说明
                    // 本端改过（改名 / 换图标 / 钉住）→ 死亡快照入冲突箱交 UI 仲裁。
                    // 旧实现无条件 `remove_folder_direct`：对端删分组时本端刚改的名字
                    // 连痕迹都不留，其下条目全部掉进「未分组」。
                    match repo.get_folder_opt(&change.record_id) {
                        Some(local) if local.rev > change.rev => {
                            let remote_snapshot = repo.change_to_folder(change)?;
                            let entry = FolderConflictEntry {
                                conflict_id: uuid::Uuid::new_v4().to_string(),
                                folder_id: change.record_id.clone(),
                                local_updated_at: local.updated_at,
                                remote_updated_at: remote_snapshot.updated_at,
                                local_snapshot: local,
                                remote_snapshot,
                                detected_at: now_ms(),
                                resolved: false,
                                remote_deleted: true,
                                // D-205：本端是活跃版本，本端侧不是墓碑。
                                local_deleted: false,
                            };
                            repo.add_folder_conflict(entry)?;
                            report.conflicts.push(change.record_id.clone());
                        }
                        Some(local) => {
                            repo.remove_folder_direct(&local.folder_id)?;
                            // D-205：本端原来也挂着一张冲突（旧快照），删除落地即事实解决。
                            repo.expire_folder_conflicts_for(&change.record_id)?;
                            report.applied += 1;
                        }
                        // 本端从未有该分组（或已删）→ 删除意图已达成。
                        None => {
                            // D-205：本端"已删"时可能正挂着一张「本端已删除 ↔ 远端改名」的
                            // 冲突卡。对端现在也删了，两边事实已经一致——不收口的话，用户
                            // 在 A 上会看到一条早就处理完的陈旧冲突，只能自己猜要不要再点
                            // 一遍（走查单 3.81 的真实路径：用户在 B 上点「采用远端」）。
                            repo.expire_folder_conflicts_for(&change.record_id)?;
                            report.skipped += 1;
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
                let remote = repo.change_to_folder(change)?;
                // D-56 复活抑制（分组壳同条目语义）。
                let suppressed = matches!(
                    repo.tombstone_rev(RECORD_TYPE_FOLDER, &change.record_id),
                    Some(tomb_rev) if tomb_rev >= change.rev
                );
                if suppressed {
                    report.skipped += 1;
                    state.base.insert(
                        change.record_id.clone(),
                        BaseRev {
                            rev: change.rev,
                            hash: [0u8; 32],
                        },
                    );
                    continue;
                }
                let remote_rev = remote.rev;
                let remote_hash = folder_hash(&remote)?;
                match repo.get_folder_opt(&change.record_id) {
                    Some(local) => {
                        let base = state.base.get(&change.record_id).cloned();
                        // D-171：分组双改同样不得静默覆盖（Q3 裁定）。旧实现直接
                        // put_folder_direct，等于无条件 LWW——两端各改一次，谁后同步
                        // 谁的名字留下，另一边看不到任何提示。
                        if detect_folder_conflict(&local, &remote, base.as_ref())? {
                            let entry = FolderConflictEntry {
                                conflict_id: uuid::Uuid::new_v4().to_string(),
                                folder_id: change.record_id.clone(),
                                local_updated_at: local.updated_at,
                                remote_updated_at: remote.updated_at,
                                local_snapshot: local.clone(),
                                remote_snapshot: remote.clone(),
                                detected_at: now_ms(),
                                resolved: false,
                                remote_deleted: false,
                                local_deleted: false,
                            };
                            repo.add_folder_conflict(entry)?;
                            report.conflicts.push(change.record_id.clone());
                            // 保留本机为正式分组，基准推进到远端 rev（同条目语义）。
                            state.base.insert(
                                change.record_id.clone(),
                                BaseRev {
                                    rev: remote_rev,
                                    hash: remote_hash,
                                },
                            );
                            continue;
                        }
                        let take_remote = remote.updated_at > local.updated_at
                            || (remote.updated_at == local.updated_at && remote_rev > local.rev);
                        if take_remote {
                            repo.put_folder_direct(remote)?;
                            // D-171：分组本体已换新版本，挂着的旧冲突快照过时（同条目
                            // D-111 的「事实解决」收口）。
                            repo.expire_folder_conflicts_for(&change.record_id)?;
                            report.applied += 1;
                        } else {
                            report.skipped += 1;
                        }
                        state.base.insert(
                            change.record_id.clone(),
                            BaseRev {
                                rev: remote_rev.max(local.rev),
                                hash: remote_hash,
                            },
                        );
                    }
                    None => {
                        // D-205：本端已删（持墓碑）、远端是**更晚的** live 改名 →
                        // delete-vs-update 真冲突，进冲突箱交用户二选一，**不落地**。
                        // 旧实现无条件 `put_folder_direct`：存活方先发起同步时，本端
                        // 的删除被远端的改名静默复活（真机 1.6.6 走查 3.81/3.84）。
                        //
                        // 这里**刻意不推进 base**：墓碑 rev 冻结在删除前那一版（原因见
                        // [`deleted_local_folder_conflict`]），一旦 base 推到远端 rev，
                        // `tombstone_changes` 的 peer 过滤（`base.rev >= 墓碑 rev` 不导出）
                        // 就会把墓碑永久扣下——用户在本端选「保留本端（= 确认删除）」后，
                        // 删除意图再也传不到对端，两侧从此静默分叉。留旧 base，下一轮
                        // 墓碑照常导出，对端会按它自己那侧的 delete-vs-update 冲突收口。
                        if let Some(dead) = deleted_local_folder_conflict(repo, change) {
                            // D-205 去重：该分组已有未解决卡（对端重发 / 重新配对 /
                            // resync 重放）→ 只原地刷新远端侧，不再开一张，也不重复
                            // 计入 report（调用方按 conflicts 计数，重复计会以为发生了
                            // 多起事件）。
                            if repo
                                .refresh_folder_conflict_remote(&change.record_id, &remote)?
                                .is_none()
                            {
                                let entry = FolderConflictEntry {
                                    conflict_id: uuid::Uuid::new_v4().to_string(),
                                    folder_id: change.record_id.clone(),
                                    local_updated_at: dead.updated_at,
                                    remote_updated_at: remote.updated_at,
                                    local_snapshot: dead,
                                    remote_snapshot: remote,
                                    detected_at: now_ms(),
                                    resolved: false,
                                    remote_deleted: false,
                                    local_deleted: true,
                                };
                                repo.add_folder_conflict(entry)?;
                                report.conflicts.push(change.record_id.clone());
                            }
                            continue;
                        }
                        repo.put_folder_direct(remote)?;
                        repo.expire_folder_conflicts_for(&change.record_id)?;
                        report.applied += 1;
                        state.base.insert(
                            change.record_id.clone(),
                            BaseRev {
                                rev: remote_rev,
                                hash: remote_hash,
                            },
                        );
                    }
                }
            }
            _ => {
                report.skipped += 1;
            }
        }
    }

    // 【D-174】整批应用完之后统一收口，不在循环里逐条判。
    //
    // 两条只有"整批看"才能判准的成因：
    // 1. 批次内顺序不保证——条目可能先于它的分组壳到达，逐条判会把它误清成未分组，
    //    而壳随后就到；
    // 2. 分组壳的重建可能被复活抑制挡下（本端墓碑 rev 更高），条目却照常落地，
    //    于是引用指向一个本端永远不会有的分组。
    //
    // 失败直接上抛：本轮同步若不能把引用修干净，就不该对上层报成功
    // （`apply_changes` 本来就没有事务，宁可能让用户重试一次同步）。
    repo.repair_dangling_folder_refs()?;

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

/// O1：接收侧水位推进——**回声消除**（D-109 修订：base 对照判同源）。
///
/// 对端发来的 live 变更若**本端当前版本就来自它**，回发必是原样冗余，回发水位
/// 可同步推进。判据：`apply_changes` 刚把 base 推进为收到的变更（rev + 内容摘要），
/// 本端条目 rev 与 hash **均与 base 一致** = 同源 → 推进；否则不推进。
///
/// D-109 之前无条件推进，在「双端同时各自推进到**同 rev**」（同 rev 双改）时误吞
/// 本端版本的导出——本端 rev2 是独立修改、对端从未见过，却被回声消除当成
/// 「对端已持有 rev2」，对端永远收不到本端修改（冲突只在一侧出现、另一侧静默丢改）。
///
/// 墓碑无需此处理——`apply_changes` 收到删除时已把 base 推进为墓碑标记（hash=0），
/// 导出侧 base 过滤天然消除回声。
///
/// 失败安全与 [`note_sent_changes`] 相同：仅随本轮交换成功（`save_sync_state`）落盘。
pub fn note_received_changes(
    repo: &Repository,
    state: &mut DeviceSyncState,
    changes: &[ChangeRecord],
) {
    for change in changes.iter().filter(|c| !c.deleted) {
        let same_as_base = match change.record_type {
            RECORD_TYPE_ITEM => repo
                .get_item_opt(&change.record_id)
                .zip(state.base.get(&change.record_id))
                .map(|(local, base)| {
                    local.rev == base.rev && item_hash(&local).ok() == Some(base.hash)
                })
                .unwrap_or(false),
            RECORD_TYPE_FOLDER => repo
                .get_folder_opt(&change.record_id)
                .zip(state.base.get(&change.record_id))
                .map(|(local, base)| {
                    local.rev == base.rev && folder_hash(&local).ok() == Some(base.hash)
                })
                .unwrap_or(false),
            _ => false,
        };
        if !same_as_base {
            continue;
        }
        let entry = state
            .sent_watermark
            .entry(change.record_id.clone())
            .or_insert(0);
        if change.rev > *entry {
            *entry = change.rev;
        }
    }
}

/// D-109：发送侧 base 推进。
///
/// 本端把 rev R **成功发出**给对端后，「对端至少持有 R」即成事实——本端对该对端的
/// 共同基准应推进到 R（hash 取发出版本的明文摘要，与接收侧 base 同口径）。仅在
/// `change.rev > 现有 base.rev` 时推进（不倒退）。
///
/// 旧实现 base 仅在接收侧推进：发送方对自己已发出的版本无基准记录，下轮双改判定
/// 缺基准 → 无基准 LWW 会把后改一端**静默覆盖**先改一端（Q3 裁定禁止静默覆盖）。
///
/// 失败安全：与 [`note_sent_changes`] 同语义——在发送成功后的锁内段调用；同步中途
/// 失败则 `save_sync_state` 不执行，base 推进随会话丢弃，下一轮重发（对端 merge 幂等）。
pub fn note_sent_base(repo: &Repository, state: &mut DeviceSyncState, changes: &[ChangeRecord]) {
    for change in changes.iter().filter(|c| !c.deleted) {
        if state
            .base
            .get(&change.record_id)
            .map(|b| b.rev >= change.rev)
            .unwrap_or(false)
        {
            continue; // 已有更高基准，不倒退。
        }
        let base = match change.record_type {
            RECORD_TYPE_ITEM => repo
                .change_to_item(change)
                .ok()
                .and_then(|sent| item_hash(&sent).ok())
                .map(|hash| BaseRev {
                    rev: change.rev,
                    hash,
                }),
            RECORD_TYPE_FOLDER => repo
                .change_to_folder(change)
                .ok()
                .and_then(|sent| folder_hash(&sent).ok())
                .map(|hash| BaseRev {
                    rev: change.rev,
                    hash,
                }),
            _ => None,
        };
        if let Some(base) = base {
            state.base.insert(change.record_id.clone(), base);
        }
    }
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
