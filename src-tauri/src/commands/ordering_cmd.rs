//! 钉住 / 频度 / 归组命令（#531：拖拽排序与频度置顶）。
//!
//! 语义裁定见 `docs/02-arch.md` 数据模型节：排序 = 钉住块（`pin_order` ASC）→
//! 未钉住（`use_count` DESC → `last_used_at` DESC → `updated_at` DESC → id ASC）；
//! 「使用」= 仅复制动作（前端在复制成功后调 `record_item_usage` 埋点），60s 节流；
//! 收藏（favorite）只是标记，不参与排序。

use crate::error::{lock_err, IpcResult};
use crate::state::AppState;

/// 记录一次条目使用（复制动作埋点；60 秒节流，窗口内调用为零开销 no-op）。
#[tauri::command(rename_all = "snake_case")]
pub fn record_item_usage(state: tauri::State<'_, AppState>, item_id: String) -> IpcResult<()> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    repo.record_item_usage(&item_id)?;
    Ok(())
}

/// 钉住 / 取消钉住条目（拖拽到顶部 = `pinned = true`）。
#[tauri::command(rename_all = "snake_case")]
pub fn set_item_pin(
    state: tauri::State<'_, AppState>,
    item_id: String,
    pinned: bool,
) -> IpcResult<()> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    repo.set_item_pin(&item_id, pinned)?;
    Ok(())
}

/// 重排已钉住条目（钉住块内拖拽；每个 id 必须存在且已钉住）。
#[tauri::command(rename_all = "snake_case")]
pub fn reorder_pinned_items(
    state: tauri::State<'_, AppState>,
    ordered_ids: Vec<String>,
) -> IpcResult<()> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    repo.reorder_pinned_items(&ordered_ids)?;
    Ok(())
}

/// 钉住 / 取消钉住分组。
#[tauri::command(rename_all = "snake_case")]
pub fn set_folder_pin(
    state: tauri::State<'_, AppState>,
    folder_id: String,
    pinned: bool,
) -> IpcResult<()> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    repo.set_folder_pin(&folder_id, pinned)?;
    Ok(())
}

/// 重排已钉住分组（每个 id 必须存在且已钉住）。
#[tauri::command(rename_all = "snake_case")]
pub fn reorder_pinned_folders(
    state: tauri::State<'_, AppState>,
    ordered_ids: Vec<String>,
) -> IpcResult<()> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    repo.reorder_pinned_folders(&ordered_ids)?;
    Ok(())
}

/// 移动条目到分组（`folder_id = null` 表示移出为未分组；目标分组必须存在）。
#[tauri::command(rename_all = "snake_case")]
pub fn move_item_to_folder(
    state: tauri::State<'_, AppState>,
    item_id: String,
    folder_id: Option<String>,
) -> IpcResult<()> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    repo.move_item_to_folder(&item_id, folder_id)?;
    Ok(())
}
