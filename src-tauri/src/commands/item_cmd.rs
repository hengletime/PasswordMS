//! 条目命令：CRUD / 搜索 / 收藏（架构文档 §5.2、F-04、F-08）。

use passwordms_core::store::repository::ItemDraft;

use crate::dto::{ItemDraftDto, ItemDto};
use crate::error::{lock_err, IpcResult};
use crate::state::AppState;

/// 列出条目（支持分组过滤 / 仅收藏 / 模糊搜索）。
#[tauri::command(rename_all = "snake_case")]
pub fn list_items(
    state: tauri::State<'_, AppState>,
    folder_id: Option<String>,
    favorite_only: Option<bool>,
    query: Option<String>,
) -> IpcResult<Vec<ItemDto>> {
    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    Ok(repo.list_items(
        folder_id.as_deref(),
        favorite_only.unwrap_or(false),
        query.as_deref(),
    ))
}

/// 取单条。
#[tauri::command(rename_all = "snake_case")]
pub fn get_item(state: tauri::State<'_, AppState>, item_id: String) -> IpcResult<ItemDto> {
    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    Ok(repo.get_item(&item_id)?)
}

/// 新建条目。
#[tauri::command(rename_all = "snake_case")]
pub fn create_item(state: tauri::State<'_, AppState>, draft: ItemDraftDto) -> IpcResult<ItemDto> {
    let mut session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    Ok(repo.create_item(to_draft(draft))?)
}

/// 更新条目（逐条加密：仅重写该条）。
#[tauri::command(rename_all = "snake_case")]
pub fn update_item(
    state: tauri::State<'_, AppState>,
    item_id: String,
    draft: ItemDraftDto,
) -> IpcResult<ItemDto> {
    let mut session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    Ok(repo.update_item(&item_id, to_draft(draft))?)
}

/// 删除条目（默认进回收站）。
#[tauri::command(rename_all = "snake_case")]
pub fn delete_item(
    state: tauri::State<'_, AppState>,
    item_id: String,
    permanent: bool,
) -> IpcResult<()> {
    let mut session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    repo.delete_item(&item_id, permanent)?;
    Ok(())
}

/// 收藏切换。
#[tauri::command(rename_all = "snake_case")]
pub fn toggle_favorite(
    state: tauri::State<'_, AppState>,
    item_id: String,
    favorite: bool,
) -> IpcResult<ItemDto> {
    let mut session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    Ok(repo.toggle_favorite(&item_id, favorite)?)
}

/// DTO → 核心草稿。
fn to_draft(dto: ItemDraftDto) -> ItemDraft {
    ItemDraft {
        kind: dto.kind,
        folder_id: dto.folder_id,
        favorite: dto.favorite,
    }
}
