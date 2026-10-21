//! Bitwarden 导入命令：预览 / 执行 / 模板导出（架构文档 §5.2、F-14；D-72）。

use passwordms_core::import::{self, ImportFormat};

#[cfg(target_os = "android")]
use passwordms_core::CoreError;

use crate::dto::{ImportPreviewDto, ImportResultDto};
use crate::error::{lock_err, IpcResult};
use crate::state::AppState;

/// F-14 解析并预览（结果缓存于状态，返回 `preview_token`）。
///
/// D-72：Android 上 `source_path` 可能是 `content://…`（文件选择器返回的 SAF URI），
/// 此时不能用 `std::fs::read_to_string`（内核无此路径 → `ERR_IO`），改走
/// [`crate::content_uri::read_to_bytes`] 读字节再按 UTF-8 解析；
/// 非 UTF-8 复用 `CoreError::ImportParse`（**不新增错误码**）。
/// 其余情况（桌面任意路径、Android 普通路径）走原 `preview_from_file`，行为不变。
#[tauri::command(rename_all = "snake_case")]
pub fn import_preview(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    source_path: String,
    format: String,
) -> IpcResult<ImportPreviewDto> {
    // 桌面目标不使用 `app`（content:// 只出现在 Android）；显式忽略避免 unused 告警。
    #[cfg(not(target_os = "android"))]
    let _ = &app;

    let fmt = ImportFormat::parse(&format)?;

    #[cfg(target_os = "android")]
    let preview = if source_path.starts_with("content://") {
        let bytes = crate::content_uri::read_to_bytes(&app, &source_path)?;
        // 导入源必须是 UTF-8 文本（JSON / CSV 皆然）；解码失败归「无法解析」，
        // 与既有 `ERR_IMPORT_PARSE` 同码，前端文案不变。
        let content =
            String::from_utf8(bytes).map_err(|_| CoreError::ImportParse("utf8".to_string()))?;
        import::preview_from_str(&content, fmt)?
    } else {
        import::preview_from_file(std::path::Path::new(&source_path), fmt)?
    };
    #[cfg(not(target_os = "android"))]
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

/// D-72：把内置 CSV 模板写到用户选定位置（Android `content://` / 桌面路径）。
///
/// 内容固定为 `passwordms_core::import::template::CSV_TEMPLATE`（模板与解析器的
/// 一致性由 `tests/import_template.rs` 守着）。与附件导出同一个模式：
/// 桌面 `std::fs::write`，Android 走 provider fd 的 `"wt"` 写入。
#[tauri::command(rename_all = "snake_case")]
pub fn save_import_template(app: tauri::AppHandle, dest_path: String) -> IpcResult<()> {
    // 桌面目标不使用 `app`；显式忽略避免 unused 告警。
    #[cfg(not(target_os = "android"))]
    let _ = &app;

    let template = passwordms_core::import::template::CSV_TEMPLATE;

    #[cfg(target_os = "android")]
    if dest_path.starts_with("content://") {
        return crate::content_uri::write_bytes(&app, &dest_path, template.as_bytes());
    }

    std::fs::write(&dest_path, template)?;
    Ok(())
}
