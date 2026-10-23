// D-81：vault store 的「保存后必须可见且选中」接线守卫。
//
// 纯函数判定在 `save-visibility.test.ts`（9 例）；这里守的是**接线**：
// createItem / updateItem 成功后必须调用判定、按判定回落过滤器、重拉、并保持选中。
// `api` 整体 mock（vitest，node 环境，与 `theme.test.ts` 的 mock 方式同族）。

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ItemDto } from '../../types';

vi.mock('../ipc', () => ({
  api: {
    items: { list: vi.fn(), create: vi.fn(), update: vi.fn(), remove: vi.fn() },
    folders: { list: vi.fn() },
  },
}));

import { api } from '../ipc';
import { vault } from './vault.svelte';

const listMock = vi.mocked(api.items.list);
const createMock = vi.mocked(api.items.create);
const updateMock = vi.mocked(api.items.update);
const foldersListMock = vi.mocked(api.folders.list);

/** 构造条目。 */
function item(overrides: Partial<ItemDto> & { item_id: string }): ItemDto {
  return {
    kind: { type: 'login', title: 'Default', custom_fields: [] },
    attachments: [],
    folder_id: null,
    favorite: false,
    created_at: 1,
    updated_at: 1,
    rev: 1,
    modified_by: 'local',
    ...overrides,
  };
}

beforeEach((): void => {
  vi.clearAllMocks();
  vault.reset();
  foldersListMock.mockResolvedValue([]);
});

describe('D-81 createItem：保存后必须可见且选中', () => {
  it('过滤器藏住了新条目（组不匹配）→ 回落全部条目、重拉、选中刚存条目', async () => {
    vault.selectedFolderId = 'folder-a';
    const created = item({ item_id: 'new-1', folder_id: null });
    createMock.mockResolvedValue(created);
    // 第一次 load 用旧过滤器（组 A）→ 返回不含新条目；第二次 load（全部）→ 返回它。
    listMock.mockResolvedValueOnce([]).mockResolvedValueOnce([created]);

    const result = await vault.createItem({
      kind: { type: 'login', title: 'Default', custom_fields: [] },
      folder_id: null,
      favorite: false,
    });

    expect(result).not.toBeNull();
    expect(vault.selectedFolderId).toBeNull();
    expect(vault.favoriteOnly).toBe(false);
    expect(vault.query).toBe('');
    expect(vault.selectedItemId).toBe('new-1');
    // 两次拉取：先旧过滤器，后回落后的「全部」。
    expect(listMock).toHaveBeenNthCalledWith(1, 'folder-a', false, undefined);
    expect(listMock).toHaveBeenNthCalledWith(2, null, false, undefined);
  });

  it('过滤器没藏住新条目（条目在列表里）→ 过滤器原样，只拉一次', async () => {
    vault.selectedFolderId = 'folder-a';
    const created = item({ item_id: 'new-2', folder_id: 'folder-a' });
    createMock.mockResolvedValue(created);
    listMock.mockResolvedValue([created]);

    await vault.createItem({
      kind: { type: 'login', title: 'Default', custom_fields: [] },
      folder_id: 'folder-a',
      favorite: false,
    });

    expect(vault.selectedFolderId).toBe('folder-a');
    expect(vault.selectedItemId).toBe('new-2');
    expect(listMock).toHaveBeenCalledTimes(1);
    expect(listMock).toHaveBeenCalledWith('folder-a', false, undefined);
  });

  it('query 藏住新条目 → 只清 query，组与收藏保留', async () => {
    vault.selectedFolderId = 'folder-a';
    vault.query = 'github';
    const created = item({ item_id: 'new-3', folder_id: 'folder-a', kind: { type: 'login', title: '银行', custom_fields: [] } });
    createMock.mockResolvedValue(created);
    listMock.mockResolvedValueOnce([]).mockResolvedValueOnce([created]);

    await vault.createItem({
      kind: { type: 'login', title: '银行', custom_fields: [] },
      folder_id: 'folder-a',
      favorite: false,
    });

    expect(vault.selectedFolderId).toBe('folder-a');
    expect(vault.query).toBe('');
    expect(listMock).toHaveBeenNthCalledWith(2, 'folder-a', false, undefined);
  });
});

describe('D-81 updateItem：更新后同样必须可见且选中', () => {
  it('把条目改到别的分组 → 回落全部条目、选中它（不再「保存就消失」）', async () => {
    vault.selectedFolderId = 'folder-a';
    vault.selectItem('moved-1');
    const updated = item({ item_id: 'moved-1', folder_id: 'folder-b' });
    updateMock.mockResolvedValue(updated);
    listMock.mockResolvedValueOnce([]).mockResolvedValueOnce([updated]);

    const result = await vault.updateItem('moved-1', {
      kind: { type: 'login', title: 'Default', custom_fields: [] },
      folder_id: 'folder-b',
      favorite: false,
    });

    expect(result).not.toBeNull();
    expect(vault.selectedFolderId).toBeNull();
    expect(vault.selectedItemId).toBe('moved-1');
    expect(listMock).toHaveBeenNthCalledWith(2, null, false, undefined);
  });

  it('更新后仍在原过滤器里 → 过滤器原样、选中保持', async () => {
    vault.selectedFolderId = 'folder-a';
    vault.selectItem('stay-1');
    const updated = item({ item_id: 'stay-1', folder_id: 'folder-a' });
    updateMock.mockResolvedValue(updated);
    listMock.mockResolvedValue([updated]);

    await vault.updateItem('stay-1', {
      kind: { type: 'login', title: 'Default', custom_fields: [] },
      folder_id: 'folder-a',
      favorite: false,
    });

    expect(vault.selectedFolderId).toBe('folder-a');
    expect(vault.selectedItemId).toBe('stay-1');
    expect(listMock).toHaveBeenCalledTimes(1);
  });
});

describe('D-81 queryEpoch：外部重置的信号', () => {
  it('保存回落时 epoch +1（ItemList 靠它同步搜索框显示）', async () => {
    vault.selectedFolderId = 'folder-a';
    const before = vault.queryEpoch;
    const created = item({ item_id: 'new-4', folder_id: null });
    createMock.mockResolvedValue(created);
    listMock.mockResolvedValueOnce([]).mockResolvedValueOnce([created]);

    await vault.createItem({
      kind: { type: 'login', title: 'Default', custom_fields: [] },
      folder_id: null,
      favorite: false,
    });

    expect(vault.queryEpoch).toBe(before + 1);
  });

  it('可见路径不 +1（过滤器没变，搜索框无须同步）', async () => {
    vault.selectedFolderId = 'folder-a';
    const before = vault.queryEpoch;
    const created = item({ item_id: 'new-5', folder_id: 'folder-a' });
    createMock.mockResolvedValue(created);
    listMock.mockResolvedValue([created]);

    await vault.createItem({
      kind: { type: 'login', title: 'Default', custom_fields: [] },
      folder_id: 'folder-a',
      favorite: false,
    });

    expect(vault.queryEpoch).toBe(before);
  });
});
