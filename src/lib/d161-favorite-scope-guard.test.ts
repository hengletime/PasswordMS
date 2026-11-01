// D-161 守门：三个视图位是**一个不可分割的整体**，切换视图必须三位一起写。
//
// 主人复现（2026-11-01）：「『个人』和『Commander 办公』各收藏一条，如果从『个人』
// 跳到收藏，收藏里只有『个人』分组里的一条；如果从『Commander 办公』跳到收藏，
// 收藏里只有『Commander 办公』分组里的一条；从其它分组跳到收藏就为空，
// 从全部跳到收藏才 2 条都有。」
//
// 根因：`toggleFavoriteOnly` 当年只写了两行（`favoriteOnly` 取反 + 清 `ungroupedOnly`），
// 漏了 `selectedFolderId`。后端 `list_scoped` 是交集（`scope ∩ favorite_only ∩ query`），
// 少传一位不报错、不告警，只是安静地少给数据 —— 这类缺陷靠肉眼 review 极难发现，
// 因为「清掉两个位」和「清掉三个位」在 diff 里长得几乎一样。
//
// 所以修法不是补一行，而是**把三位收成一个函数**（`setScope`）：
// 参数三个字段全部必填、无默认值 —— 默认值正是这次漏掉的形态（漏写等于沿用旧值，
// TypeScript 保持沉默）。本文件钉住这层结构：
//   ① 三个切换入口一律走 `setScope`，函数体内不再直接写这三个位；
//   ② `setScope` 体内三个位**各赋值一次**，且最后统一 `load()`；
//   ③ `toggleFavoriteOnly` 的 `folderId` 是字面量 `null`（收藏跨组，那一位必须主动清掉）；
//   ④ `setScope` 的参数不接受可选字段（不许出现 `?:`）。
//
// 与 `vault.test.ts` D-161 段的分工：那边断言「发出去的请求对不对」（行为），
// 这边断言「结构上还能不能再次漏一位」（接线）。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(['./stores/vault.svelte.ts', './list-scope.ts'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

/** 取源码；**必须归一化换行**——文件是 CRLF，带 `\n` 的字面量会恒不命中（D-147 踩过）。 */
function src(suffix: string): string {
  const text = Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1] ?? '';
  return text.replace(/\r\n/g, '\n');
}

/** 切出区间（含首尾标记）。找不到返回空串——断言前先钉长度 > 0，否则切片失败会假绿。 */
function section(text: string, open: string, close: string): string {
  const start = text.indexOf(open);
  if (start < 0) {
    return '';
  }
  const end = text.indexOf(close, start + open.length);
  return end < 0 ? '' : text.slice(start, end + close.length);
}

/** 三个「同级单一视图」的切换入口。名字写死：改名等于换语义，得先来改这个测试。 */
const VIEW_SWITCHERS = ['async selectFolder(', 'async selectUngrouped(', 'async toggleFavoriteOnly('];

describe('D-161：切视图必须三位一起写', () => {
  it('① 三个切换入口一律走 setScope，体内不再直接写任何一个位', () => {
    const store = src('/stores/vault.svelte.ts');
    for (const marker of VIEW_SWITCHERS) {
      const body = section(store, marker, '\n  }');
      expect(body, `${marker} 切片失败`).not.toBe('');
      expect(body, `${marker} 没走 setScope`).toContain('await this.setScope(');
      // 只写两位正是本 bug 的形态：写不写第三位，diff 上看不出来。
      expect(body, `${marker} 仍直接写 selectedFolderId`).not.toMatch(
        /\bthis\.selectedFolderId\s*=[^=]/,
      );
      expect(body, `${marker} 仍直接写 favoriteOnly`).not.toMatch(/\bthis\.favoriteOnly\s*=[^=]/);
      expect(body, `${marker} 仍直接写 ungroupedOnly`).not.toMatch(
        /\bthis\.ungroupedOnly\s*=[^=]/,
      );
      // 三个字段都得出现（`selectFolder` 用简写 `folderId`，其余两个带冒号）。
      expect(body, `${marker} 漏传 folderId`).toContain('folderId');
      expect(body, `${marker} 漏传 favoriteOnly`).toContain('favoriteOnly:');
      expect(body, `${marker} 漏传 ungroupedOnly`).toContain('ungroupedOnly:');
    }
  });

  it('② 三个位的直接赋值只出现在四个已知位置（剔除后别处一律不许写）', () => {
    const store = src('/stores/vault.svelte.ts');
    // 四处各自有独立语义：setScope（视图切换的唯一入口）、revealSavedItem（D-81 保存后
    // 必须可见的回落）、deleteFolder（被删的组不在了）、reset（锁定时清空内存态）。
    // 用「剔除已知例外 → 剩余文本必须干净」的加法判据，而不是数总次数：
    // 魔数阈值会在新增正当用法时莫名变红，而漏一位的 bug 恰恰是次数不变、语义不同。
    // 先钉 setScope 自己：三位各一行、谁都不许漏。漏一位的症状取决于调用方
    // （漏 `ungroupedOnly` 只在「从未分组进收藏」时露头），靠行为例容易漏测。
    const setter = section(store, 'private async setScope(', 'await this.load();\n  }');
    expect(setter, 'setScope 切片失败').not.toBe('');
    expect(setter).toContain('this.selectedFolderId = scope.folderId;');
    expect(setter).toContain('this.favoriteOnly = scope.favoriteOnly;');
    expect(setter).toContain('this.ungroupedOnly = scope.ungroupedOnly;');

    let rest = store;
    // 注意 `setScope` 的收尾标记不能写 `\n  }`：它参数对象自己的 `  }` 会先被命中，
    // 切出来只剩参数声明、函数体留在外面（第一版就这么假红了一次）。
    for (const [marker, close] of [
      ['private async setScope(', 'await this.load();\n  }'],
      ['private async revealSavedItem(', '\n  }'],
      ['async deleteFolder(', '\n  }'],
      ['reset(): void {', '\n  }'],
    ] as const) {
      const body = section(rest, marker, close);
      expect(body, `${marker} 切片失败`).not.toBe('');
      rest = rest.replace(body, '');
    }
    for (const field of ['selectedFolderId', 'favoriteOnly', 'ungroupedOnly']) {
      expect(
        rest,
        `${field} 在视图切换之外被直接赋值（新写一处就得先说明它为什么不走 setScope）`,
      ).not.toMatch(new RegExp(`\\bthis\\.${field}\\s*=[^=]`));
    }
  });

  it('③ toggleFavoriteOnly 把 folderId 明确清成 null（收藏跨组，这一位必须主动清）', () => {
    const store = src('/stores/vault.svelte.ts');
    const body = section(store, 'async toggleFavoriteOnly(', '\n  }');
    expect(body, 'toggleFavoriteOnly 切片失败').not.toBe('');
    // 字面量 null，不许写成 `folderId: this.selectedFolderId`（那就是本 bug 原样）。
    expect(body).toMatch(/folderId:\s*null,/);
    expect(body, '把当前分组又传进去了').not.toContain('folderId: this.selectedFolderId');
  });

  it('④ setScope 的参数三个字段全必填（不给默认值 —— 漏写的沉默成本比多写高得多）', () => {
    const store = src('/stores/vault.svelte.ts');
    const params = section(store, 'private async setScope(', '}): Promise<void>');
    expect(params, 'setScope 参数块切片失败').not.toBe('');
    expect(params, '出现可选字段').not.toContain('?:');
    expect(params).toContain('folderId: string | null;');
    expect(params).toContain('favoriteOnly: boolean;');
    expect(params).toContain('ungroupedOnly: boolean;');
  });

  it('⑤ 收藏仍算「跨组」，D-160 的组名显示不回退（收藏真跨组了，这条才第一次名副其实）', () => {
    const scope = src('/list-scope.ts');
    const body = section(scope, 'export function shouldShowRowFolder(', '\n}');
    expect(body, 'shouldShowRowFolder 切片失败').not.toBe('');
    // 第一个分支必须是「收藏 → 显示」，且必须先于 folderId 判定。
    expect(body).toMatch(/if \(scope\.favoriteOnly\) \{\s*return true;/);
    expect(body.indexOf('scope.favoriteOnly')).toBeLessThan(body.indexOf('scope.folderId'));
  });
});
