//! 分组命令：CRUD / 排序（架构文档 §5.2、F-03）。

use passwordms_core::store::repository::FolderDeleteStrategy;

use crate::dto::FolderDto;
use crate::error::{lock_err, IpcError, IpcResult};
use crate::state::AppState;

/// 列出分组。
#[tauri::command(rename_all = "snake_case")]
pub fn list_folders(state: tauri::State<'_, AppState>) -> IpcResult<Vec<FolderDto>> {
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    Ok(repo
        .list_folders()
        .into_iter()
        .map(FolderDto::from)
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
    Ok(FolderDto::from(repo.create_folder(name, icon, color)?))
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
    Ok(FolderDto::from(
        repo.update_folder(&folder_id, name, icon, color)?,
    ))
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
