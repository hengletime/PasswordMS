//! 密钥文档命令：附件添加 / 移除 / 导出（架构文档 §5.2、F-12；D-60 §3.14.5）。
//!
//! D-60 语义：附件是登录/安全笔记条目的**附属内容**（父条目内嵌 `attachments` 数组），
//! 不再是独立条目。`import_attachment` 更名 `add_attachment`（按父条目定位），
//! 新增 `remove_attachment`，`export_attachment` / `attachment_blob_ready` 改按
//! `attachment_id` 定位。

use std::path::Path;

use crate::dto::ItemDto;
use crate::error::{lock_err, IpcResult};
use crate::state::AppState;

/// F-12 向父条目添加附件（§3.14.5）。
///
/// 校验链（core 层）：父条目存在且非遗留 attachment 型 → 附件数 ≤ 16
/// （`ERR_ATTACHMENT_LIMIT`）→ 文件名 ≤ 255B（`ERR_VALIDATION`）→ 单文件 ≤ 20 MB
/// （`ERR_ATTACHMENT_TOO_LARGE`）→ 库容量 ≤ 200 MB（`ERR_VAULT_CAPACITY_EXCEEDED`）。
/// 返回**父条目**（rev+1），前端据此刷新详情。
#[tauri::command(rename_all = "snake_case")]
pub fn add_attachment(
    state: tauri::State<'_, AppState>,
    item_id: String,
    source_path: String,
    title: String,
) -> IpcResult<ItemDto> {
    let mut session = state.session.lock().map_err(lock_err)?;
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
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    Ok(repo.remove_attachment(&item_id, &attachment_id)?)
}

/// F-12 导出附件（明文风险提示与二次确认由前端负责；导出后不自动打开）。
///
/// D-53 语义保留：附件元数据在但 blob 未同步到本机时报 `ERR_BLOB_MISSING`，
/// 经 `From<CoreError>` 原样透传给前端。
#[tauri::command(rename_all = "snake_case")]
pub fn export_attachment(
    state: tauri::State<'_, AppState>,
    item_id: String,
    attachment_id: String,
    dest_path: String,
) -> IpcResult<()> {
    let session = state.session.lock().map_err(lock_err)?;
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
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    Ok(repo.has_blob(&attachment_id))
}
