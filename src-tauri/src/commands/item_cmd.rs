//! 条目命令：CRUD / 搜索 / 收藏（架构文档 §5.2、F-04、F-08）。

use passwordms_core::store::repository::ItemDraft;

use crate::dto::{ItemCountsDto, ItemDraftDto, ItemDto};
use crate::error::{lock_err, IpcResult};
use crate::state::AppState;

/// 条目计数（D-152）：总数 / 收藏 / 未分组。
///
/// 独立成一条命令而不是塞进 `list_items` 的返回：`list_items` 的结果随筛选变化，
/// 而这三个数**必须与筛选无关** —— 用户在「收藏」视图里也要看到全库总条数，
/// 否则读数一变就又要怀疑同步丢数据了。
#[tauri::command(rename_all = "snake_case")]
pub fn item_counts(state: tauri::State<'_, AppState>) -> IpcResult<ItemCountsDto> {
    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    let c = repo.item_counts();
    Ok(ItemCountsDto {
        total: c.total,
        favorites: c.favorites,
        ungrouped: c.ungrouped,
    })
}

/// 列出条目（支持分组过滤 / 仅收藏 / 模糊搜索 / **未分组**）。
///
/// `ungrouped_only = true` 时忽略 `folder_id`：前端把「未分组」做成与分组同级的视图，
/// 两者不会同时给出。万一同时给出，语义更具体的 `ungrouped_only` 优先 ——
/// 否则「两个筛选条件互相矛盾 → 列表为空」是用户无法解释的结果。
#[tauri::command(rename_all = "snake_case")]
pub fn list_items(
    state: tauri::State<'_, AppState>,
    folder_id: Option<String>,
    favorite_only: Option<bool>,
    query: Option<String>,
    ungrouped_only: Option<bool>,
) -> IpcResult<Vec<ItemDto>> {
    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    let favorite_only = favorite_only.unwrap_or(false);
    if ungrouped_only.unwrap_or(false) {
        return Ok(repo.list_ungrouped_items(favorite_only, query.as_deref()));
    }
    Ok(repo.list_items(folder_id.as_deref(), favorite_only, query.as_deref()))
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
