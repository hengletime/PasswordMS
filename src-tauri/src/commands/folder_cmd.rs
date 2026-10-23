//! 分组命令：CRUD / 排序（架构文档 §5.2、F-03；#531 起排序消费 pin/频度字段）。

use passwordms_core::store::repository::FolderDeleteStrategy;

use crate::dto::FolderDto;
use crate::error::{lock_err, IpcError, IpcResult};
use crate::state::AppState;

/// 列出分组（#531：`use_count`/`last_used_at` 为组内聚合值，与排序键同源）。
#[tauri::command(rename_all = "snake_case")]
pub fn list_folders(state: tauri::State<'_, AppState>) -> IpcResult<Vec<FolderDto>> {
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    let usage = repo.folder_usage();
    Ok(repo
        .list_folders()
        .into_iter()
        .map(|f| {
            let u = usage.get(&f.folder_id).copied().unwrap_or_default();
            FolderDto::from_folder(f, u)
        })
        .collect())
}

/// 新建分组。
#[tauri::command(rename_all = "snake_case")]
pub fn create_folder(
    state: tauri::State<'_, AppState>,
    name: String,
    icon: Option<String>,
    color: Option<String>,
) -> IpcResult<FolderDto> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    let folder = repo.create_folder(name, icon, color)?;
    // 新分组还没有条目，聚合值必然是默认值；仍走 folder_usage 保证口径统一。
    let usage = repo
        .folder_usage()
        .get(&folder.folder_id)
        .copied()
        .unwrap_or_default();
    Ok(FolderDto::from_folder(folder, usage))
}

/// 更新分组。
#[tauri::command(rename_all = "snake_case")]
pub fn update_folder(
    state: tauri::State<'_, AppState>,
    folder_id: String,
    name: String,
    icon: Option<String>,
    color: Option<String>,
) -> IpcResult<FolderDto> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    let folder = repo.update_folder(&folder_id, name, icon, color)?;
    let usage = repo
        .folder_usage()
        .get(&folder.folder_id)
        .copied()
        .unwrap_or_default();
    Ok(FolderDto::from_folder(folder, usage))
}

/// 删除分组（`strategy` 为 `to_ungrouped` / `to_trash`；二次确认由前端负责）。
#[tauri::command(rename_all = "snake_case")]
pub fn delete_folder(
    state: tauri::State<'_, AppState>,
    folder_id: String,
    strategy: String,
) -> IpcResult<()> {
    let parsed = match strategy.as_str() {
        "to_trash" => FolderDeleteStrategy::ToTrash,
        "to_ungrouped" => FolderDeleteStrategy::ToUngrouped,
        _ => {
            return Err(IpcError::from(passwordms_core::CoreError::Validation(
                "strategy".to_string(),
            )))
        }
    };
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    repo.delete_folder(&folder_id, parsed)?;
    Ok(())
}

/// 重排分组。
#[tauri::command(rename_all = "snake_case")]
pub fn reorder_folders(
    state: tauri::State<'_, AppState>,
    ordered_ids: Vec<String>,
) -> IpcResult<()> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    repo.reorder_folders(&ordered_ids)?;
    Ok(())
}
