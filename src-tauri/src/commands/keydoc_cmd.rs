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
///
/// D-53：附件条目存在但 blob 未同步到本机时，`repo.export_attachment` 报
/// `ERR_BLOB_MISSING`，经 `From<CoreError>` 原样透传给前端。
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

/// D-53 附件 blob 是否已同步到本机（AttachmentView「附件内容待同步」态的判定来源）。
#[tauri::command(rename_all = "snake_case")]
pub fn attachment_blob_ready(
    state: tauri::State<'_, AppState>,
    item_id: String,
) -> IpcResult<bool> {
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    let item = repo.get_item(&item_id)?;
    let blob_ref = match &item.kind {
        passwordms_core::model::item::ItemKind::Attachment(a) => a.blob_ref.clone(),
        _ => return Ok(true), // 非 attachment 条目无此概念，视为就绪
    };
    Ok(repo.has_blob(&blob_ref))
}
