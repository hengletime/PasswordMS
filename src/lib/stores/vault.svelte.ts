// 保险库数据状态（Svelte 5 runes）：条目、分组、筛选与 CRUD。

import type { FolderDto, ItemDraftDto, ItemDto } from '../../types';
import { toAppError } from '../errors';
import { api } from '../ipc';
import { revealFilterAfterSave, type ItemFilter } from '../save-visibility';
import { ui } from './ui.svelte';

/** 保险库 store。 */
class VaultStore {
  /** 当前列表（已按筛选条件过滤）。 */
  items = $state<ItemDto[]>([]);
  /** 全部分组。 */
  folders = $state<FolderDto[]>([]);
  /** 当前分组筛选（null = 全部）。 */
  selectedFolderId = $state<string | null>(null);
  /** 是否仅看收藏。 */
  favoriteOnly = $state(false);
  /** 搜索关键词。 */
  query = $state('');
  /**
   * query 被程序化改写的代数（D-81）。
   * 只在**外部重置**（保存回落「全部条目」、锁定 `reset()`）时 +1；
   * 用户打字走 `setQuery`，不改它。`ItemList` 靠它区分「自己的防抖延迟」与「外部重置」，
   * 前者不能动输入框（会清掉正在打的字），后者必须同步（搜索框显示 = 实际过滤）。
   */
  queryEpoch = $state(0);
  /** 当前选中条目。 */
  selectedItemId = $state<string | null>(null);
  /** 是否加载中。 */
  busy = $state(false);

  /** D-106（豆包审查 3.5-1）：单调递增请求序号——过期响应不得覆盖最新列表。 */
  private loadSeq = 0;

  /** 当前选中条目对象。 */
  get selectedItem(): ItemDto | null {
    if (this.selectedItemId === null) {
      return null;
    }
    return this.items.find((i): boolean => i.item_id === this.selectedItemId) ?? null;
  }

  /** 加载（条目 + 分组）。 */
  async load(): Promise<void> {
    const seq = ++this.loadSeq;
    this.busy = true;
    try {
      const [items, folders] = await Promise.all([
        api.items.list(this.selectedFolderId, this.favoriteOnly, this.query || undefined),
        api.folders.list(),
      ]);
      // 快速切分组/搜索时多个请求在途：只有最新一次的响应才允许写状态，
      // 否则后返回的旧结果会覆盖新筛选的列表（UI 状态与数据不一致）。
      if (seq !== this.loadSeq) {
        return;
      }
      this.items = items;
      this.folders = folders;
      if (this.selectedItemId !== null && !items.some((i): boolean => i.item_id === this.selectedItemId)) {
        this.selectedItemId = null;
      }
    } catch (err) {
      // 过期请求的失败同样不打扰当前视图（toast 属于最新一次交互）。
      if (seq === this.loadSeq) {
        ui.toast('error', toAppError(err).messageKey);
      }
    } finally {
      if (seq === this.loadSeq) {
        this.busy = false;
      }
    }
  }

  /** 选择分组。 */
  async selectFolder(folderId: string | null): Promise<void> {
    this.selectedFolderId = folderId;
    this.favoriteOnly = false;
    await this.load();
  }

  /** 切换「仅收藏」。 */
  async toggleFavoriteOnly(): Promise<void> {
    this.favoriteOnly = !this.favoriteOnly;
    await this.load();
  }

  /** 设置搜索词并刷新。 */
  async setQuery(query: string): Promise<void> {
    this.query = query;
    await this.load();
  }

  /** 选中条目。 */
  selectItem(itemId: string | null): void {
    this.selectedItemId = itemId;
  }

  /** 当前过滤器快照（传给 save-visibility 的纯函数；不持有引用，避免误改）。 */
  private filterSnapshot(): ItemFilter {
    return {
      selectedFolderId: this.selectedFolderId,
      favoriteOnly: this.favoriteOnly,
      query: this.query,
    };
  }

  /**
   * D-81：保存成功的条目**必须立刻可见且选中**。
   *
   * 在保存前的过滤器下重新 `load()` 过一次之后调用：若条目仍不在列表里
   * （分组 / query / favoriteOnly 任一藏了它），按纯函数 `revealFilterAfterSave`
   * 给出的回落过滤器重置并再拉一次，否则用户视角就是「保存就消失」
   * （编辑器关了、列表里没有、详情区也没有 —— 条目其实存进了别的视图看不见的地方）。
   *
   * @param saved 刚保存的条目（create / update 的返回值）
   */
  private async revealSavedItem(saved: ItemDto): Promise<void> {
    const before = this.filterSnapshot();
    const next = revealFilterAfterSave(before, saved, this.items);
    if (
      next.selectedFolderId === before.selectedFolderId &&
      next.favoriteOnly === before.favoriteOnly &&
      next.query === before.query
    ) {
      return;
    }
    this.selectedFolderId = next.selectedFolderId;
    this.favoriteOnly = next.favoriteOnly;
    this.query = next.query;
    this.queryEpoch += 1;
    await this.load();
  }

  /** 新建条目。 */
  async createItem(draft: ItemDraftDto): Promise<ItemDto | null> {
    try {
      const created = await api.items.create(draft);
      await this.load();
      await this.revealSavedItem(created);
      this.selectedItemId = created.item_id;
      return created;
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
      return null;
    }
  }

  /** 更新条目。 */
  async updateItem(itemId: string, draft: ItemDraftDto): Promise<ItemDto | null> {
    try {
      const updated = await api.items.update(itemId, draft);
      await this.load();
      await this.revealSavedItem(updated);
      this.selectedItemId = itemId;
      return updated;
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
      return null;
    }
  }

  /** 删除条目（默认进回收站）。 */
  async deleteItem(itemId: string, permanent: boolean): Promise<void> {
    try {
      await api.items.remove(itemId, permanent);
      if (this.selectedItemId === itemId) {
        this.selectedItemId = null;
      }
      await this.load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 切换收藏。 */
  async toggleFavorite(itemId: string, favorite: boolean): Promise<void> {
    try {
      await api.items.toggleFavorite(itemId, favorite);
      await this.load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 新建分组。 */
  async createFolder(name: string): Promise<void> {
    try {
      await api.folders.create(name, null, null);
      await this.load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 更新分组。 */
  async updateFolder(folderId: string, name: string): Promise<void> {
    try {
      await api.folders.update(folderId, name, null, null);
      await this.load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  // ===== #531：排序与频度（命令后端并行注册中；错误统一 toast，沿用现有模式） =====

  /**
   * 复制成功后的使用埋点（后端 60s 节流）。成功后 reload：
   * 未钉住区按频度排，埋点会让刚用过的条目浮到前面 —— 这正是「频度置顶」的可见行为。
   */
  async recordItemUsage(itemId: string): Promise<void> {
    try {
      await api.items.recordUsage(itemId);
      await this.load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 钉住 / 取消钉住条目（拖拽松手与菜单共用）。 */
  async setItemPin(itemId: string, pinned: boolean): Promise<void> {
    try {
      await api.items.setPin(itemId, pinned);
      await this.load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 已钉住条目的新顺序（仅钉住集合内的相对顺序）。 */
  async reorderPinnedItems(orderedIds: string[]): Promise<void> {
    try {
      await api.items.reorderPinned(orderedIds);
      await this.load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 钉住 / 取消钉住分组。 */
  async setFolderPin(folderId: string, pinned: boolean): Promise<void> {
    try {
      await api.folders.setPin(folderId, pinned);
      await this.load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 已钉住分组的新顺序。 */
  async reorderPinnedFolders(orderedIds: string[]): Promise<void> {
    try {
      await api.folders.reorderPinned(orderedIds);
      await this.load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 条目跨分组移动；folderId 传 null = 移到未分组。 */
  async moveItemToFolder(itemId: string, folderId: string | null): Promise<void> {
    try {
      await api.items.moveToFolder(itemId, folderId);
      await this.load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 删除分组。 */
  async deleteFolder(folderId: string, strategy: 'to_ungrouped' | 'to_trash'): Promise<void> {
    try {
      await api.folders.remove(folderId, strategy);
      if (this.selectedFolderId === folderId) {
        this.selectedFolderId = null;
      }
      await this.load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 重置全部内存态（锁定时调用）。 */
  reset(): void {
    this.items = [];
    this.folders = [];
    this.selectedFolderId = null;
    this.favoriteOnly = false;
    this.query = '';
    this.queryEpoch += 1;
    this.selectedItemId = null;
  }
}

/** 单例。 */
export const vault = new VaultStore();
