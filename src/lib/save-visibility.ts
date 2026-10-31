// D-81：保存可见性判定（纯函数）。
//
// 缺陷：选中某分组后新建条目，条目存进根目录（编辑器不预填分组），而列表按
// `selectedFolderId` 过滤 → 刚存的条目不在返回列表里 → 用户视角「保存就消失」。
//
// 这里的职责只有一件：**判断当前过滤器会不会藏住刚保存的条目，会的话给出回落后的过滤器**。
// 「回落到全部条目」是 store 层的硬保证（保存成功的条目必须立刻可见且选中），
// 判据抽成纯函数是为了让各种藏匿情形都能离屏单测。
//
// `matchesSavedItemQuery` 逐字段镜像后端 `Repository::list_items` 的搜索语义
// （`repository.rs::item_matches`）：needle = query.trim().toLowerCase()，
// 匹配 title（三种 kind 都有）/ login.username / login.url，子串包含、大小写不敏感。
// **后端语义一变，这里必须跟着变** —— 否则判定会说「可见」而列表里没有。

import type { ItemDto } from '../types';

/** 列表过滤器（`vault.svelte.ts` 的四个过滤位）。 */
export interface ItemFilter {
  /** 当前分组（null = 全部）。 */
  selectedFolderId: string | null;
  /** 是否仅看收藏。 */
  favoriteOnly: boolean;
  /** 搜索关键词。 */
  query: string;
  /**
   * 是否仅看未分组（D-153）。
   *
   * 与 `selectedFolderId` 是**两个位**：后者的 `null` 表示「全部」，不能兼任「未分组」。
   * 少了这一位，在未分组视图里新建带分组的条目会以为「可见」而实际不在列表里 ——
   * 正好复现 D-81 原本那个「保存就消失」。
   */
  ungroupedOnly: boolean;
}

/**
 * 判断条目是否匹配搜索词（镜像后端 `item_matches`：title / login.username / login.url，
 * 子串包含、大小写不敏感；空白词视为匹配一切）。
 *
 * @param saved 刚保存的条目
 * @param query 搜索词（未归一化，与 store 里存的原始值一致）
 * @returns 是否匹配
 */
export function matchesSavedItemQuery(saved: ItemDto, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === '') {
    return true;
  }
  if (saved.kind.title.toLowerCase().includes(needle)) {
    return true;
  }
  if (saved.kind.type === 'login') {
    if (saved.kind.username !== null && saved.kind.username !== undefined) {
      if (saved.kind.username.toLowerCase().includes(needle)) {
        return true;
      }
    }
    if (saved.kind.url !== null && saved.kind.url !== undefined) {
      if (saved.kind.url.toLowerCase().includes(needle)) {
        return true;
      }
    }
  }
  return false;
}

/**
 * 保存成功后应该使用的过滤器。
 *
 * 判定顺序：
 * 1. 条目已在 `items`（用当前过滤器刚拉回的列表）里 → 过滤器没藏它，**原样返回**；
 * 2. 否则逐位判断哪个过滤位藏了它，**只清藏了它的那几位**（组不匹配清组、未分组的
 *    条目带了组就清未分组、收藏不匹配清收藏、query 不匹配清 query），其余保留 ——
 *    用户不该因为一次保存丢掉与这次保存无关的过滤状态；
 * 3. 各过滤位按字段判断全都匹配、条目却不在列表里 → 状态不一致（陈旧等未知原因），
 *    保守回落「全部条目」，保住「保存成功的条目必须立刻可见」这条硬保证。
 *
 * @param current 当前四个过滤位
 * @param saved 刚保存的条目（create 的返回值 / update 的返回值）
 * @param items 用 `current` 过滤器刚拉回的列表
 * @returns 应使用的过滤器（可见时与 `current` 字段完全相同）
 */
export function revealFilterAfterSave(
  current: ItemFilter,
  saved: ItemDto,
  items: readonly ItemDto[],
): ItemFilter {
  if (items.some((item): boolean => item.item_id === saved.item_id)) {
    return current;
  }
  const folderHides =
    current.selectedFolderId !== null && saved.folder_id !== current.selectedFolderId;
  const ungroupedHides = current.ungroupedOnly && saved.folder_id !== null;
  const favoriteHides = current.favoriteOnly && !saved.favorite;
  const queryHides = current.query.trim() !== '' && !matchesSavedItemQuery(saved, current.query);
  if (!folderHides && !ungroupedHides && !favoriteHides && !queryHides) {
    return { selectedFolderId: null, favoriteOnly: false, query: '', ungroupedOnly: false };
  }
  return {
    selectedFolderId: folderHides ? null : current.selectedFolderId,
    favoriteOnly: favoriteHides ? false : current.favoriteOnly,
    query: queryHides ? '' : current.query,
    ungroupedOnly: ungroupedHides ? false : current.ungroupedOnly,
  };
}
