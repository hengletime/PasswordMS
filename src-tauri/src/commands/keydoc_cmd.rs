//! 密钥文档命令：附件导入 / 导出（架构文档 §5.2、F-12）。

use std::path::Path;

use crate::dto::ItemDto;
use crate::error::{lock_err, IpcResult};
use crate::state::AppState;

/// F-12 导入附件（≤ 20 MB；超限报 `ERR_ATTACHMENT_TOO_LARGE`）。
#[tauri::command(rename_all = "snake_case")]
pub fn import_attachment(
    state: tauri::State<'_, AppState>,
    source_path: String,
    title: String,
) -> IpcResult<ItemDto> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    Ok(repo.import_attachment(Path::new(&source_path), title)?)
}

/// F-12 导出附件（明文风险提示与二次确认由前端负责；导出后不自动打开）。
#[tauri::command(rename_all = "snake_case")]
pub fn export_attachment(
    state: tauri::State<'_, AppState>,
    item_id: String,
    dest_path: String,
) -> IpcResult<()> {
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    repo.export_attachment(&item_id, Path::new(&dest_path))?;
    Ok(())
}
