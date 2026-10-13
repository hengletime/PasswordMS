//! Bitwarden 导入命令：预览 / 执行（架构文档 §5.2、F-14）。

use passwordms_core::import::{self, ImportFormat};

use crate::dto::{ImportPreviewDto, ImportResultDto};
use crate::error::{lock_err, IpcResult};
use crate::state::AppState;

/// F-14 解析并预览（结果缓存于状态，返回 `preview_token`）。
#[tauri::command(rename_all = "snake_case")]
pub fn import_preview(
    state: tauri::State<'_, AppState>,
    source_path: String,
    format: String,
) -> IpcResult<ImportPreviewDto> {
    let fmt = ImportFormat::parse(&format)?;
    let preview = import::preview_from_file(std::path::Path::new(&source_path), fmt)?;
    let token = uuid::Uuid::new_v4().to_string();
    state
        .pending_imports
        .lock()
        .map_err(crate::error::lock_err)?
        .insert(token.clone(), preview.clone());
    Ok(ImportPreviewDto::from_preview(token, &preview))
}

/// F-14 执行导入（`source_path` / `format` 仅用于契约对齐；实际使用缓存的预览）。
#[tauri::command(rename_all = "snake_case")]
pub fn import_apply(
    state: tauri::State<'_, AppState>,
    source_path: String,
    format: String,
    preview_token: String,
) -> IpcResult<ImportResultDto> {
    let _ = source_path;
    let _ = format;

    let preview = state
        .pending_imports
        .lock()
        .map_err(crate::error::lock_err)?
        .remove(&preview_token)
        .ok_or_else(|| {
            crate::error::IpcError::from(passwordms_core::CoreError::Validation(
                "preview_token".to_string(),
            ))
        })?;

    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    let result = import::apply(&preview, repo)?;
    Ok(ImportResultDto::from(result))
}
