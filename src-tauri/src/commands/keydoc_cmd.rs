//! 密钥文档命令：附件添加 / 移除 / 导出（架构文档 §5.2、F-12；D-60 §3.14.5）。
//!
//! D-60 语义：附件是登录/安全笔记条目的**附属内容**（父条目内嵌 `attachments` 数组），
//! 不再是独立条目。`import_attachment` 更名 `add_attachment`（按父条目定位），
//! 新增 `remove_attachment`，`export_attachment` / `attachment_blob_ready` 改按
//! `attachment_id` 定位。
//!
//! D-66 根因（Android 附件必失败）：`@tauri-apps/plugin-dialog` 在 Android 上的
//! `open()` 返回的是 **`content://` URI 字符串**（`tauri-plugin-dialog` 的
//! `DialogPlugin.kt`：`uris.add(uri?.toString())`），不是文件系统路径。此前后端把它
//! 直接当路径交给 `Repository::add_attachment`，第一步 `std::fs::metadata("content://…")`
//! 就必然 `NotFound` → `ERR_IO` → toast「文件读写出错。」，手机上永远加不进附件。
//! 修法：`content://` 走 [`crate::content_uri::read_to_bytes`]（provider fd，带 20 MB 上限）
//! → `Repository::add_attachment_bytes`，文件名由
//! [`passwordms_core::attachment_name::filename_from_source`] 百分号解码得到。
//!
//! D-71 根因（Android 导出附件必失败）：`saveFileDialog` 走
//! `Intent.ACTION_CREATE_DOCUMENT`，回调同样是 `content://`（`DialogPlugin.kt`：
//! `callResult.put("file", uri.toString())`）。此前它被当路径交给
//! `Repository::export_attachment` 的 `std::fs::write` → `EINVAL` → `ERR_IO`。
//! 修法：`content://` 用 [`Repository::export_attachment_bytes`] 取字节（锁内），
//! **出锁后**再经 [`crate::content_uri::write_bytes`] 落盘。

use std::path::Path;

#[cfg(target_os = "android")]
use passwordms_core::attachment_name::filename_from_source;
// content:// 通道只在 Android 分支使用；桌面下该导入会变成 unused（-D warnings 下报错）。
#[cfg(target_os = "android")]
use crate::content_uri;

use crate::dto::ItemDto;
use crate::error::{lock_err, IpcResult};
use crate::state::AppState;

/// F-12 向父条目添加附件（§3.14.5）。
///
/// 校验链（core 层）：父条目存在且非遗留 attachment 型 → 附件数 ≤ 16
/// （`ERR_ATTACHMENT_LIMIT`）→ 文件名 ≤ 255B（`ERR_VALIDATION`）→ 单文件 ≤ 20 MB
/// （`ERR_ATTACHMENT_TOO_LARGE`）→ 库容量 ≤ 200 MB（`ERR_VAULT_CAPACITY_EXCEEDED`）。
/// 返回**父条目**（rev+1），前端据此刷新详情。
///
/// D-66：Android 上 `source_path` 可能是 `content://…`，此时走
/// `add_attachment_bytes`（文件名百分号解码）；其余情况（桌面任意路径、Android 上的
/// 普通路径）仍走 `add_attachment`，行为与修复前完全一致。
#[tauri::command(rename_all = "snake_case")]
pub fn add_attachment(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    item_id: String,
    source_path: String,
    title: String,
) -> IpcResult<ItemDto> {
    // 桌面目标不使用 `app`（content:// 只出现在 Android）；显式忽略避免 unused 告警。
    #[cfg(not(target_os = "android"))]
    let _ = &app;

    #[cfg(target_os = "android")]
    if source_path.starts_with("content://") {
        // 先在锁外把字节读出来：ContentProvider 读取可能慢，不占会话锁。
        let data = content_uri::read_to_bytes(&app, &source_path)?;
        let name = filename_from_source(&source_path);
        let mut session = state.lock_session().map_err(lock_err)?;
        let repo = session.repo_mut()?;
        return Ok(repo.add_attachment_bytes(&item_id, &name, &data)?);
    }

    let mut session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    Ok(repo.add_attachment(&item_id, Path::new(&source_path), &title)?)
}

/// F-12 从父条目移除附件（blob 交孤儿 GC，§3.14.4）。返回父条目（rev+1）。
#[tauri::command(rename_all = "snake_case")]
pub fn remove_attachment(
    state: tauri::State<'_, AppState>,
    item_id: String,
    attachment_id: String,
) -> IpcResult<ItemDto> {
    let mut session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    Ok(repo.remove_attachment(&item_id, &attachment_id)?)
}

/// F-12 导出附件（明文风险提示与二次确认由前端负责；导出后不自动打开）。
///
/// D-53 语义保留：附件元数据在但 blob 未同步到本机时报 `ERR_BLOB_MISSING`，
/// 经 `From<CoreError>` 原样透传给前端。
///
/// D-71：Android 上 `dest_path` 可能是 `content://…`（SAF 保存对话框），此时
/// **锁内取字节 → 出锁写 provider**，其余情况（桌面任意路径、Android 普通路径）走原
/// `export_attachment`，行为与修复前完全一致。
#[tauri::command(rename_all = "snake_case")]
pub fn export_attachment(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    item_id: String,
    attachment_id: String,
    dest_path: String,
) -> IpcResult<()> {
    #[cfg(not(target_os = "android"))]
    let _ = &app;

    #[cfg(target_os = "android")]
    if dest_path.starts_with("content://") {
        // 取字节在锁内完成，块结束即释放会话锁——写 provider 可能慢，不持锁做 I/O。
        let data = {
            let session = state.lock_session().map_err(lock_err)?;
            let repo = session.repo()?;
            repo.export_attachment_bytes(&item_id, &attachment_id)?
        };
        return content_uri::write_bytes(&app, &dest_path, &data);
    }

    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    repo.export_attachment(&item_id, &attachment_id, Path::new(&dest_path))?;
    Ok(())
}

/// D-53/D-60：附件 blob 是否已同步到本机（附件行「内容待同步」态的判定来源）。
#[tauri::command(rename_all = "snake_case")]
pub fn attachment_blob_ready(
    state: tauri::State<'_, AppState>,
    attachment_id: String,
) -> IpcResult<bool> {
    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    Ok(repo.has_blob(&attachment_id))
}
