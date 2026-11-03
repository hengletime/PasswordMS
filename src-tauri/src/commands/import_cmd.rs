//! Bitwarden 导入命令：预览 / 执行 / 模板导出（架构文档 §5.2、F-14；D-72）。

use passwordms_core::import::{self, ImportFormat};

use crate::commands::backup_cmd;
use crate::dto::{ImportPreviewDto, ImportResultDto};
use crate::error::{lock_err, IpcResult};
use crate::state::AppState;

/// F-14 解析并预览（结果缓存于状态，返回 `preview_token`）。
///
/// D-72：Android 上 `source_path` 可能是 `content://…`（文件选择器返回的 SAF URI），
/// 此时不能用 `std::fs::read_to_string`（内核无此路径 → `ERR_IO`），改走
/// [`crate::content_uri::read_to_bytes`] 读字节再解码；
/// D-87：桌面路径与 Android 字节路径统一走 [`passwordms_core::import::decode_source`]
/// 编码容错（UTF-8 / UTF-16 BOM / GBK），解码失败归 `ERR_IMPORT_PARSE`。
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

    // D-108（3.4-6）：导入向导是用户活动（本命令不走会话锁，需显式触点）。
    state.touch_activity();

    let fmt = ImportFormat::parse(&format)?;

    #[cfg(target_os = "android")]
    let preview = if source_path.starts_with("content://") {
        // 【D-96】SAF 入口按导入上限（64 MB）读取；再按同一常量复查字节长度
        // （超限报 `ERR_VALIDATION`，信息带实际字节数）。
        let bytes = crate::content_uri::read_to_bytes_limited(
            &app,
            &source_path,
            passwordms_core::constants::MAX_IMPORT_FILE_BYTES,
        )?;
        if bytes.len() as u64 > passwordms_core::constants::MAX_IMPORT_FILE_BYTES {
            return Err(crate::error::IpcError::from(
                passwordms_core::CoreError::Validation(format!(
                    "import_file_too_large: {} > {} bytes",
                    bytes.len(),
                    passwordms_core::constants::MAX_IMPORT_FILE_BYTES
                )),
            ));
        }
        let content = passwordms_core::import::decode_source(&bytes)?;
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
///
/// **F-08：预览的消费时机是「成功之后」**。旧实现先 `remove(&preview_token)` 再拿会话锁，
/// 于是库已自动锁定、写盘失败、校验失败三种结局都会把预览一并吃掉——用户只能重新选
/// 文件、重新解析，明明什么都没导进去。现在：
///
/// 1. 先拿会话锁（锁失败时预览**原封不动**）；
/// 2. 再 `remove` 取出（take 语义，防同一预览被并发提交两次）；
/// 3. `apply` 成功才算消费；失败把预览**放回缓存**——core 侧已保证「全成或全无」，
///    所以放回去的预览与库的状态仍然对得上，用户点重试就是真正的重试。
#[tauri::command(rename_all = "snake_case")]
pub fn import_apply(
    state: tauri::State<'_, AppState>,
    source_path: String,
    format: String,
    preview_token: String,
) -> IpcResult<ImportResultDto> {
    let _ = source_path;
    let _ = format;

    // 锁先于取预览：这一句的顺序就是 F-08 的全部修法。
    let mut session = state.lock_session().map_err(lock_err)?;
    // 导入是整库级破坏性操作：走事件快照，**不受日备份去重与节流影响**。
    // 快照拍在锁内、apply 之前——apply 一旦开始就在写帧，那时再拍就晚了。
    backup_cmd::snapshot_event(
        &session,
        passwordms_core::store::backup::EventTag::PreImport,
    );
    let repo = session.repo_mut()?;

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

    match import::apply(&preview, repo) {
        Ok(result) => Ok(ImportResultDto::from(result)),
        Err(err) => {
            // 失败归还。`apply` 失败时磁盘一字未动（D-172），预览仍然有效。
            // 归还这句拿不到锁（中毒）就只能放弃——那是另一条错误路径，不该吞掉
            // 原始错误；此时用户重试会看到 `preview_token`，也就是旧行为。
            if let Ok(mut cache) = state.pending_imports.lock() {
                cache.insert(preview_token, preview);
            }
            Err(err.into())
        }
    }
}

/// D-72：把 CSV 模板写到用户选定位置（Android `content://` / 桌面路径）。
///
/// D-112：示例文本（`texts`）由前端按当前 UI 语言从 i18n 字典取值传入，
/// core 只保留表头与转义示范等结构（契约由 core `tests/import_template.rs` 守着）。
/// 与附件导出同一个模式：桌面 `std::fs::write`，Android 走 provider fd 的 `"wt"` 写入。
#[tauri::command(rename_all = "snake_case")]
pub fn save_import_template(
    app: tauri::AppHandle,
    dest_path: String,
    texts: passwordms_core::import::template::TemplateTexts,
) -> IpcResult<()> {
    // 桌面目标不使用 `app`；显式忽略避免 unused 告警。
    #[cfg(not(target_os = "android"))]
    let _ = &app;

    // serde 反序列化不走 `TemplateTexts::new`，这里重建以强制执行
    // 「文本不得含引号/换行」校验（防前端缺陷产出结构性损坏的模板）。
    let texts = passwordms_core::import::template::TemplateTexts::new(
        texts.folder,
        texts.login_name,
        texts.login_notes,
        texts.note_name,
        texts.note_body,
    )?;
    let template = passwordms_core::import::template::csv_template(&texts);

    #[cfg(target_os = "android")]
    if dest_path.starts_with("content://") {
        return crate::content_uri::write_bytes(&app, &dest_path, template.as_bytes());
    }

    std::fs::write(&dest_path, template)?;
    Ok(())
}
