//! 本机自动备份（`backups/`）的状态、清理与写入前快照。
//!
//! ## 名词约定：两种「备份」，界面上必须能分开
//!
//! 本产品有两种东西，日常都叫"备份"，混起来会要命：
//!
//! | 代码里 | 界面上 | 能不能带走 |
//! |---|---|---|
//! | `export_vault`（用户主动导出） | **「导出备份」** | 能，换机可用 |
//! | `backups/`（本模块） | **「本机自动备份」** | 不能，卸载/换机一起没 |
//!
//! 代码里继续用 `restore_point` / `old-key` / `batch` 这些精确的词（它们描述的是
//! 磁盘结构，改了就没人看得懂）；**界面上一个都不出现**——「回滚点」「旧代」「批次」
//! 「归档」这些词在 1.5.29 发布后被用户明确指出看不懂（2026-11-03）。
//!
//! 两者不能合并称呼的理由，是那个会把用户推进坑里的推论：
//! 一旦自动备份也叫「备份」，用户会以为「有备份了就不用导出了」——
//! 那正好抵消掉唯一能救换机的提示。
//!
//! ## 为什么写入前快照不回头改各命令的返回值
//!
//! 备份是旁路：写不进去也要让用户改得动数据。失败会落进 `.state`
//! （`last_error_ms` / `last_error_kind`），由 [`backup_status`] 读出、设置页常驻行显示。
//! 这样十来个写命令只需要在开头一行调用，不必各改一遍 DTO——而"各改一遍"正是
//! 将来漏一个就静默的地方。

use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::store::backup;

use crate::dto::{BackupBatchDto, BackupClearDto, BackupPointDto, BackupStatusDto};
use crate::error::{lock_err, IpcResult};
use crate::state::AppState;

/// 在**外层操作边界**拍一次写入前快照（一次用户操作 = 一次）。
///
/// 不要挂在单帧写入上：一次导入 5000 条若逐帧触发，就是 5000 次整库复制。
pub(crate) fn snapshot_before(session: &Session) {
    let Ok(repo) = session.repo() else {
        return;
    };
    let path = repo.path();
    let out = backup::snapshot_before_write(&path, now_ms());
    if let Some(kind) = out.error_kind {
        // 命令层无 AppHandle，走 eprintln（桌面 GUI 会丢弃，但 Android logcat 可见）。
        // 用户可见的部分由 `.state` → `backup_status` → 设置页常驻行承担。
        eprintln!(
            "[backup] 写入前快照失败 kind={kind:?} stage={:?}",
            out.error_stage
        );
    }
}

/// 破坏性操作前的事件快照（恢复 / 换密 / 压缩 / 导入 apply）。
///
/// 与 [`snapshot_before`] 的区别：**不受日备份去重与节流影响**——这几种操作一旦出错
/// 就是整库级损失，哪怕同一秒内已经备份过也要再留一份。
pub(crate) fn snapshot_event(session: &Session, tag: backup::EventTag) {
    let Ok(repo) = session.repo() else {
        return;
    };
    let path = repo.path();
    let out = backup::snapshot_event(&path, tag, now_ms());
    if let Some(kind) = out.error_kind {
        eprintln!(
            "[backup] {} 快照失败 kind={kind:?} stage={:?}",
            tag.prefix(),
            out.error_stage
        );
    }
}

/// 回滚点状态：当前代 / 旧代分两栏 + 上次成功 + 粘性失败 + 新鲜度。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_status(state: tauri::State<'_, AppState>) -> IpcResult<BackupStatusDto> {
    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    let path = repo.path();

    let (current_bytes, current_files) = backup::usage(&path);
    let (old_bytes, old_files) = backup::old_key_usage(&path);
    let batches = backup::old_key_batches(&path)
        .into_iter()
        .map(|b| BackupBatchDto {
            dir_name: b.dir_name,
            batch_ms: b.batch_ms,
            files: b.files,
            bytes: b.bytes,
        })
        .collect();
    let st = backup::state(&path);
    // 「退回去」的候选清单：**全部**能覆盖当前库的整库副本，不是只有 `pre-restore`
    // ——理由见 [`backup::BackupPoint`]（D-178：只认一份会让按钮几乎永不出现）。
    let points = backup::restore_points(&path)
        .into_iter()
        .map(|p| BackupPointDto {
            token: p.token,
            kind: p.kind,
            at_ms: p.created_ms,
            bytes: p.size_bytes,
            batch_ms: p.batch_ms,
            needs_old_password: p.needs_old_password,
        })
        .collect();

    Ok(BackupStatusDto {
        current_bytes,
        current_files,
        old_bytes,
        old_files,
        old_batches: batches,
        unreadable_batches: backup::unreadable_batch_dirs(&path),
        last_ms: st.last_ms,
        last_error_ms: st.last_error_ms,
        last_error_kind: st.last_error_kind,
        staleness: backup::staleness(&path, st.last_ms),
        points,
    })
}

/// 删除**当前代**回滚点（`backups/` 顶层），返回删除份数与残留份数。
///
/// `old-key/` 一份不动——那是另一把密码的退路，走 [`backup_clear_old_batch`]。
/// 回执与界面文案必须说清这个边界，否则用户会以为"删干净了"。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_clear_current(state: tauri::State<'_, AppState>) -> IpcResult<BackupClearDto> {
    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    let out = backup::clear_all(&repo.path())?;
    Ok(BackupClearDto {
        deleted: out.deleted,
        remaining: out.remaining,
    })
}

/// 删除**某一批**旧代回滚点，返回删除份数与残留份数。
///
/// 按批而不是只有"全删"：旧代是单向门，而"我怕旧密码还能开"这个需求往往只针对
/// 其中一批（最原始那把可能早忘了）。整体清空把一个小需求绑成一个大动作。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_clear_old_batch(
    state: tauri::State<'_, AppState>,
    dir_name: String,
) -> IpcResult<BackupClearDto> {
    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    let out = backup::clear_old_key_batch(&repo.path(), &dir_name)?;
    Ok(BackupClearDto {
        deleted: out.deleted,
        remaining: out.remaining,
    })
}
