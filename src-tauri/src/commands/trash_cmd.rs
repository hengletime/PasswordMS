//! 回收站命令（架构文档 §5.2、F-15）。

use crate::dto::{ItemDto, TrashDto};
use crate::error::{lock_err, IpcResult};
use crate::state::AppState;

/// 列出回收站。
#[tauri::command(rename_all = "snake_case")]
pub fn list_trash(state: tauri::State<'_, AppState>) -> IpcResult<Vec<TrashDto>> {
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    Ok(repo.list_trash().into_iter().map(TrashDto::from).collect())
}

/// 还原条目。
#[tauri::command(rename_all = "snake_case")]
pub fn restore_item(state: tauri::State<'_, AppState>, trash_id: String) -> IpcResult<ItemDto> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    Ok(repo.restore_item(&trash_id)?)
}

/// 彻底删除单条。
#[tauri::command(rename_all = "snake_case")]
pub fn purge_item(state: tauri::State<'_, AppState>, trash_id: String) -> IpcResult<()> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    repo.purge_item(&trash_id)?;
    Ok(())
}

/// 清空回收站（强二次确认由前端负责，不可恢复）。
#[tauri::command(rename_all = "snake_case")]
pub fn empty_trash(state: tauri::State<'_, AppState>) -> IpcResult<()> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    repo.empty_trash()?;
    Ok(())
}
