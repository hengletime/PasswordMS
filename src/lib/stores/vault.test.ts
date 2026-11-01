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
const countsMock = vi.mocked(api.items.counts);
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
    expect(listMock).toHaveBeenNthCalledWith(1, 'folder-a', false, undefined, false);
    expect(listMock).toHaveBeenNthCalledWith(2, null, false, undefined, false);
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
    expect(listMock).toHaveBeenCalledWith('folder-a', false, undefined, false);
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
    expect(listMock).toHaveBeenNthCalledWith(2, 'folder-a', false, undefined, false);
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
    expect(listMock).toHaveBeenNthCalledWith(2, null, false, undefined, false);
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

// D-159：切视图（分组 / 收藏 / 未分组）**保留**搜索词 —— 分组是范围，搜索是条件，两者叠加。
//
// 这一节原先是 D-158 的「切视图重置搜索词」（主人 2026-11-01 当天的第一版反馈），
// 当天即被主人推翻：「搜 j → 直接点侧栏『个人』，搜索框不应该自动清空，应该只显示
// 『个人』分组里 j 的搜索结果。」记录在这里是为了防止下次有人凭「导航该重置筛选」
// 这条通用直觉又把它清回去。
//
// D-158 真正的问题（搜索框空着、筛选还生效）由 ItemList 的显示值对齐解决；
// 词既然始终看得见，「看全库」就交给清空按钮，而不是替用户清。
describe('D-159 切视图保留搜索词', () => {
  beforeEach(() => {
    listMock.mockReset().mockResolvedValue([]);
    foldersListMock.mockReset().mockResolvedValue([]);
    countsMock.mockReset().mockResolvedValue({ total: 0, favorites: 0, ungrouped: 0 });
    vault.reset();
    // reset 会推进 epoch，同步一次基准。
  });

  it('search 之后切分组：query 保留、epoch 不动、新请求仍带 query（组内搜索）', async () => {
    await vault.setQuery('j');
    expect(vault.query).toBe('j');
    const before = vault.queryEpoch;

    await vault.selectFolder('f-work');

    expect(vault.query).toBe('j');
    // 这次是用户点侧栏，不是程序改写搜索框 —— 推进 epoch 会把输入框里的词清掉。
    expect(vault.queryEpoch).toBe(before);
    // 关键：真的重新查了、且**带着**关键词（只查该组 ∩ 词，不是该组全部）。
    const last = listMock.mock.calls.at(-1);
    expect(last?.[0]).toBe('f-work');
    expect(last?.[2]).toBe('j');
  });

  it('切「收藏」与「未分组」同样保留（三者同级视图，不许留一条偷清词的路径）', async () => {
    await vault.setQuery('j');
    await vault.toggleFavoriteOnly();
    expect(vault.query).toBe('j');
    expect(listMock.mock.calls.at(-1)?.[2]).toBe('j');

    await vault.selectUngrouped();
    expect(vault.query).toBe('j');
    expect(listMock.mock.calls.at(-1)?.[2]).toBe('j');
  });

  it('切分组后再点「全部」仍是同一个词（不许给「回不去」留余地靠猜）', async () => {
    await vault.setQuery('j');
    await vault.selectFolder('f-a');
    await vault.selectFolder(null);

    expect(vault.query).toBe('j');
    expect(listMock.mock.calls.at(-1)?.[0]).toBeNull();
    expect(listMock.mock.calls.at(-1)?.[2]).toBe('j');
  });

  it('用户自己清空后切分组不会把词找回来（保留的是现值，不是历史）', async () => {
    await vault.setQuery('j');
    await vault.setQuery('');

    await vault.selectFolder('f-a');

    expect(vault.query).toBe('');
    expect(listMock.mock.calls.at(-1)?.[2]).toBeUndefined();
  });

  it('打字路径不得推进 epoch（那是用户自己的输入，不算程序化重置）', async () => {
    const before = vault.queryEpoch;
    await vault.setQuery('abc');
    expect(vault.queryEpoch).toBe(before);
  });
});

// D-161：收藏是**跨组**视图 —— 进收藏必须把分组一起清掉。
//
// 主人复现（2026-11-01）：「『个人』和『Commander 办公』各收藏一条，如果从『个人』跳到收藏，
// 收藏里只有『个人』分组里的一条；如果从『Commander 办公』跳到收藏，收藏里只有
// 『Commander 办公』分组里的一条；从其它分组跳到收藏就为空，从全部跳到收藏才 2 条都有。」
//
// 这段话本身就是判据：**同一份收藏，从不同分组进来看到不同的东西** ——
// 那么这个视图就不是「收藏」，而是「上一个分组 ∩ 收藏」。
// 根因是 `toggleFavoriteOnly` 只清了 `ungroupedOnly`、漏了 `selectedFolderId`；
// 后端 `list_scoped` 是交集（`scope ∩ favorite_only ∩ query`），漏一位不会报错，
// 只会安静地少给数据 —— 所以这里断言的不是「store 字段是什么」，
// 而是「发出去的请求到底带没带分组」。
describe('D-161 收藏视图跨组（进收藏必须清掉分组）', () => {
  /** 最近一次 `items.list` 请求的三个范围位（与 ipc.list 形参顺序一致）。 */
  function lastScope(): {
    folderId: string | null | undefined;
    favoriteOnly: boolean;
    ungroupedOnly: boolean;
  } {
    const call = listMock.mock.calls.at(-1);
    return {
      folderId: call?.[0],
      favoriteOnly: call?.[1] === true,
      ungroupedOnly: call?.[3] === true,
    };
  }

  beforeEach(() => {
    listMock.mockReset().mockResolvedValue([]);
    foldersListMock.mockReset().mockResolvedValue([]);
    countsMock.mockReset().mockResolvedValue({ total: 0, favorites: 0, ungrouped: 0 });
    vault.reset();
  });

  it('从某个分组里点「收藏」：请求不再带分组（否则只是「该组 ∩ 收藏」）', async () => {
    await vault.selectFolder('f-personal');
    expect(lastScope().folderId).toBe('f-personal');

    await vault.toggleFavoriteOnly();

    expect(vault.selectedFolderId).toBeNull();
    const scope = lastScope();
    expect(scope.folderId).toBeNull();
    expect(scope.favoriteOnly).toBe(true);
    expect(scope.ungroupedOnly).toBe(false);
  });

  it('换一个分组再进收藏，看到的范围必须一样（主人这句「从别的组进去不同」就是判据）', async () => {
    await vault.selectFolder('f-personal');
    await vault.toggleFavoriteOnly();
    const fromPersonal = lastScope();

    await vault.selectFolder('f-office');
    await vault.toggleFavoriteOnly();
    const fromOffice = lastScope();

    // 两次请求逐字相同 —— 收藏与「从哪儿点进来的」无关。
    expect(fromOffice).toEqual(fromPersonal);
  });

  it('从「未分组」进收藏：两位一起清（folder_id 与 ungrouped_only 谁都不许留下）', async () => {
    await vault.selectUngrouped();
    expect(lastScope()).toEqual({ folderId: null, favoriteOnly: false, ungroupedOnly: true });

    await vault.toggleFavoriteOnly();

    expect(lastScope()).toEqual({ folderId: null, favoriteOnly: true, ungroupedOnly: false });
  });

  it('退出收藏落在「全部」（界面上从没有「上次的分组」这个概念）', async () => {
    await vault.selectFolder('f-office');
    await vault.toggleFavoriteOnly();
    await vault.toggleFavoriteOnly();

    expect(vault.favoriteOnly).toBe(false);
    expect(lastScope()).toEqual({ folderId: null, favoriteOnly: false, ungroupedOnly: false });
  });

  it('从收藏切分组：收藏标记必须清（回归——这条一直是好的，不许被本轮改坏）', async () => {
    await vault.toggleFavoriteOnly();
    await vault.selectFolder('f-office');

    expect(vault.favoriteOnly).toBe(false);
    expect(lastScope()).toEqual({ folderId: 'f-office', favoriteOnly: false, ungroupedOnly: false });
  });

  it('任意切换序列下三个位互斥：不出现「某分组 + 收藏 / 未分组」的请求', async () => {
    const steps: (() => Promise<void>)[] = [
      () => vault.selectFolder('f-a'),
      () => vault.toggleFavoriteOnly(),
      () => vault.selectUngrouped(),
      () => vault.toggleFavoriteOnly(),
      () => vault.selectFolder(null),
      () => vault.selectUngrouped(),
    ];
    for (const step of steps) {
      await step();
      const scope = lastScope();
      const hasFolder = scope.folderId !== null && scope.folderId !== undefined;
      // 收藏与未分组都是「跨组」的读法，带上一只具体的组就自相矛盾（顶栏写着「收藏」、
      // 列表只有一个组的条目 —— D-150/D-151 那类「界面自相矛盾」）。
      expect(hasFolder && scope.favoriteOnly, '出现「分组 ∩ 收藏」').toBe(false);
      expect(hasFolder && scope.ungroupedOnly, '出现「分组 ∩ 未分组」').toBe(false);
    }
  });
});
