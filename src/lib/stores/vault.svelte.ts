// 保险库数据状态（Svelte 5 runes）：条目、分组、筛选与 CRUD。

import type { FolderDto, ItemDraftDto, ItemDto } from '../../types';
import { toAppError } from '../errors';
import { api } from '../ipc';
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
  /** 当前选中条目。 */
  selectedItemId = $state<string | null>(null);
  /** 是否加载中。 */
  busy = $state(false);

  /** 当前选中条目对象。 */
  get selectedItem(): ItemDto | null {
    if (this.selectedItemId === null) {
      return null;
    }
    return this.items.find((i): boolean => i.item_id === this.selectedItemId) ?? null;
  }

  /** 加载（条目 + 分组）。 */
  async load(): Promise<void> {
    this.busy = true;
    try {
      const [items, folders] = await Promise.all([
        api.items.list(this.selectedFolderId, this.favoriteOnly, this.query || undefined),
        api.folders.list(),
      ]);
      this.items = items;
      this.folders = folders;
      if (this.selectedItemId !== null && !items.some((i): boolean => i.item_id === this.selectedItemId)) {
        this.selectedItemId = null;
      }
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    } finally {
      this.busy = false;
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

  /** 新建条目。 */
  async createItem(draft: ItemDraftDto): Promise<ItemDto | null> {
    try {
      const created = await api.items.create(draft);
      await this.load();
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
    this.selectedItemId = null;
  }
}

/** 单例。 */
export const vault = new VaultStore();
