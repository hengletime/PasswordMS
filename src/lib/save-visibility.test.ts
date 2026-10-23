// D-81：保存可见性判定的纯函数守卫。
//
// 五种藏匿情形（规格要求至少 5 例，这里 9 例）：
//   ① 组不匹配 → 只清组；② query 不匹配 → 只清 query；③ 收藏不匹配 → 只清收藏；
//   ④ 全匹配（条目就在刚拉回的列表里）→ 原样；⑤ update 场景同判。
// 另加三条边界：字段全匹配却不在列表（状态不一致 → 保守全回落）、
// query 语义与后端逐字对齐（trim + 小写 + title/username/url）、多过滤位同时藏 → 全清。

import { describe, expect, it } from 'vitest';
import { matchesSavedItemQuery, revealFilterAfterSave, type ItemFilter } from './save-visibility';
import type { ItemDto } from '../types';
/** 构造条目（只填判定用得到的字段，其余给合法默认值）。 */
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

describe('D-81 revealFilterAfterSave：五种藏匿情形', () => {
  it('① 组不匹配 → 清组，query / favoriteOnly 原样保留', () => {
    const saved = item({ item_id: 'i1', folder_id: 'folder-b' });
    const current: ItemFilter = { selectedFolderId: 'folder-a', favoriteOnly: false, query: '' };
    const next = revealFilterAfterSave(current, saved, []);
    expect(next.selectedFolderId).toBeNull();
    expect(next.favoriteOnly).toBe(false);
    expect(next.query).toBe('');
  });

  it('② query 不匹配 → 清 query，组与收藏保留（用户在组内搜索时保存了不匹配的新条目）', () => {
    const saved = item({ item_id: 'i2', folder_id: 'folder-a', kind: { type: 'login', title: '银行', custom_fields: [] } });
    const current: ItemFilter = { selectedFolderId: 'folder-a', favoriteOnly: false, query: 'github' };
    const next = revealFilterAfterSave(current, saved, []);
    expect(next.selectedFolderId).toBe('folder-a');
    expect(next.favoriteOnly).toBe(false);
    expect(next.query).toBe('');
  });

  it('③ favoriteOnly 不匹配 → 清收藏，组保留', () => {
    const saved = item({ item_id: 'i3', folder_id: 'folder-a', favorite: false });
    const current: ItemFilter = { selectedFolderId: 'folder-a', favoriteOnly: true, query: '' };
    const next = revealFilterAfterSave(current, saved, []);
    expect(next.selectedFolderId).toBe('folder-a');
    expect(next.favoriteOnly).toBe(false);
    expect(next.query).toBe('');
  });

  it('④ 条目就在刚拉回的列表里 → 过滤器原样（返回与 current 字段完全相同）', () => {
    const saved = item({ item_id: 'i4', folder_id: 'folder-a', favorite: true });
    const current: ItemFilter = { selectedFolderId: 'folder-a', favoriteOnly: true, query: 'Default' };
    const items = [saved];
    const next = revealFilterAfterSave(current, saved, items);
    expect(next).toEqual(current);
  });

  it('⑤ update 场景同判：把条目改到别的分组后不在原组列表里 → 清组', () => {
    const updated = item({ item_id: 'i5', folder_id: 'folder-b' });
    const current: ItemFilter = { selectedFolderId: 'folder-a', favoriteOnly: false, query: '' };
    // items 是用旧过滤器（组 A）拉回的列表，改完分组的条目不在其中。
    const next = revealFilterAfterSave(current, updated, []);
    expect(next.selectedFolderId).toBeNull();
  });
});

describe('D-81 revealFilterAfterSave：边界', () => {
  it('字段判断全匹配却不在列表 → 保守回落全部条目（状态不一致时保住「必须可见」）', () => {
    const saved = item({ item_id: 'i6', folder_id: 'folder-a', favorite: true });
    const current: ItemFilter = { selectedFolderId: 'folder-a', favoriteOnly: true, query: 'default' };
    // 组匹配（A）、收藏匹配、query 匹配（标题含 default），但 items 里没有它。
    const next = revealFilterAfterSave(current, saved, []);
    expect(next).toEqual({ selectedFolderId: null, favoriteOnly: false, query: '' });
  });

  it('多个过滤位同时藏 → 一次全清（组 + 收藏 + query 都不匹配）', () => {
    const saved = item({ item_id: 'i7', folder_id: null, favorite: false, kind: { type: 'secure_note', title: '随手记', body: '' } });
    const current: ItemFilter = { selectedFolderId: 'folder-a', favoriteOnly: true, query: 'github' };
    const next = revealFilterAfterSave(current, saved, []);
    expect(next).toEqual({ selectedFolderId: null, favoriteOnly: false, query: '' });
  });

  it('query 语义与后端 item_matches 逐字对齐：trim + 大小写不敏感 + title/username/url', () => {
    const saved = item({
      item_id: 'i8',
      kind: { type: 'login', title: 'My Bank', url: 'https://Bank.example.com', username: 'User@X', custom_fields: [] },
    });
    // 前后空白 + 大小写：后端 needle = 'bank'，title 含 'bank' → 匹配。
    expect(matchesSavedItemQuery(saved, '  BANK ')).toBe(true);
    // username 命中。
    expect(matchesSavedItemQuery(saved, 'user@x')).toBe(true);
    // url 命中。
    expect(matchesSavedItemQuery(saved, 'bank.example')).toBe(true);
    // notes 不参与搜索（后端 item_matches 不查 notes）。
    expect(matchesSavedItemQuery(saved, 'zzz-not-anywhere')).toBe(false);
    // 纯空白 query = 无过滤（后端 filter(|q| !q.is_empty())）。
    expect(matchesSavedItemQuery(saved, '   ')).toBe(true);
  });

  it('收藏条目在 favoriteOnly 下保存 → 不清收藏（可见路径不走回落）', () => {
    const saved = item({ item_id: 'i9', folder_id: 'folder-a', favorite: true });
    const current: ItemFilter = { selectedFolderId: 'folder-a', favoriteOnly: true, query: '' };
    // items 里包含它 → 原样返回（收藏保留）。
    const next = revealFilterAfterSave(current, saved, [saved]);
    expect(next).toEqual(current);
    expect(next.favoriteOnly).toBe(true);
  });
});
