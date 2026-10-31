// D-81：vault store 的「保存后必须可见且选中」接线守卫。
//
// 纯函数判定在 `save-visibility.test.ts`（9 例）；这里守的是**接线**：
// createItem / updateItem 成功后必须调用判定、按判定回落过滤器、重拉、并保持选中。
// `api` 整体 mock（vitest，node 环境，与 `theme.test.ts` 的 mock 方式同族）。

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ItemDto } from '../../types';

vi.mock('../ipc', () => ({
  api: {
    items: {
      // D-152：`load()` 现在三路并行，计数命令必须一起 mock——漏了会让整个
      // Promise.all 拒绝，表面上红的是「过滤器没恢复」，真因却是 mock 缺一项。
      counts: vi.fn(),
      list: vi.fn(),
      create: vi.fn(),
      update: vi.fn(),
      remove: vi.fn(),
      setPin: vi.fn(),
      reorderPinned: vi.fn(),
      moveToFolder: vi.fn(),
      recordUsage: vi.fn(),
    },
    folders: {
      list: vi.fn(),
      create: vi.fn(),
      update: vi.fn(),
      remove: vi.fn(),
      reorder: vi.fn(),
      setPin: vi.fn(),
      reorderPinned: vi.fn(),
    },
  },
}));

import { api } from '../ipc';
import { vault } from './vault.svelte';

const listMock = vi.mocked(api.items.list);
const createMock = vi.mocked(api.items.create);
const updateMock = vi.mocked(api.items.update);
const foldersListMock = vi.mocked(api.folders.list);
const itemsSetPinMock = vi.mocked(api.items.setPin);
const itemsReorderPinnedMock = vi.mocked(api.items.reorderPinned);
const itemsMoveMock = vi.mocked(api.items.moveToFolder);
const itemsRecordUsageMock = vi.mocked(api.items.recordUsage);
const foldersSetPinMock = vi.mocked(api.folders.setPin);
const foldersReorderPinnedMock = vi.mocked(api.folders.reorderPinned);

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
    // #531：DTO 新增字段，夹具补默认值。
    pinned: false,
    pin_order: 0,
    use_count: 0,
    last_used_at: null,
    ...overrides,
  };
}

beforeEach((): void => {
  vi.clearAllMocks();
  vault.reset();
  foldersListMock.mockResolvedValue([]);
  vi.mocked(api.items.counts).mockResolvedValue({ total: 0, favorites: 0, ungrouped: 0 });
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

// #531：六个新命令的透传守卫 —— 参数键、命令名由 ipc.ts 钉死（ipc-contract 锚点），
// 这里守「调用真的发生 + 成功后 reload + 失败走 toast 不抛」。
describe('#531 排序与频度命令透传', () => {
  it('setItemPin：调 set_item_pin（对象参数键 item_id/pinned）并 reload', async () => {
    itemsSetPinMock.mockResolvedValue(undefined);
    listMock.mockResolvedValue([]);

    await vault.setItemPin('i1', true);

    expect(itemsSetPinMock).toHaveBeenCalledWith('i1', true);
    expect(listMock).toHaveBeenCalledTimes(1);
  });

  it('reorderPinnedItems：调 reorder_pinned_items（ordered_ids）并 reload', async () => {
    itemsReorderPinnedMock.mockResolvedValue(undefined);
    listMock.mockResolvedValue([]);

    await vault.reorderPinnedItems(['b', 'a']);

    expect(itemsReorderPinnedMock).toHaveBeenCalledWith(['b', 'a']);
    expect(listMock).toHaveBeenCalledTimes(1);
  });

  it('moveItemToFolder：folder_id=null 原样传 null（= 未分组），不转 undefined', async () => {
    itemsMoveMock.mockResolvedValue(undefined);
    listMock.mockResolvedValue([]);

    await vault.moveItemToFolder('i1', null);
    expect(itemsMoveMock).toHaveBeenCalledWith('i1', null);

    await vault.moveItemToFolder('i1', 'folder-b');
    expect(itemsMoveMock).toHaveBeenLastCalledWith('i1', 'folder-b');
    expect(listMock).toHaveBeenCalledTimes(2);
  });

  it('setFolderPin / reorderPinnedFolders：分组侧同构', async () => {
    foldersSetPinMock.mockResolvedValue(undefined);
    foldersReorderPinnedMock.mockResolvedValue(undefined);
    listMock.mockResolvedValue([]);

    await vault.setFolderPin('f1', true);
    expect(foldersSetPinMock).toHaveBeenCalledWith('f1', true);

    await vault.reorderPinnedFolders(['f2', 'f1']);
    expect(foldersReorderPinnedMock).toHaveBeenCalledWith(['f2', 'f1']);
    expect(listMock).toHaveBeenCalledTimes(2);
  });

  it('recordItemUsage：埋点成功后 reload（未钉区按频度排，用过的条目要浮上来）', async () => {
    itemsRecordUsageMock.mockResolvedValue(undefined);
    listMock.mockResolvedValue([]);

    await vault.recordItemUsage('i1');

    expect(itemsRecordUsageMock).toHaveBeenCalledWith('i1');
    expect(listMock).toHaveBeenCalledTimes(1);
  });

  it('失败路径：toast error、不抛、不 reload（复制流不被埋点失败打断界面）', async () => {
    itemsRecordUsageMock.mockRejectedValue(new Error('boom'));

    await expect(vault.recordItemUsage('i1')).resolves.toBeUndefined();
    expect(listMock).not.toHaveBeenCalled();
  });
});
