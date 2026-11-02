// D-168 守门：两件「详情页在别的条目上乱说话」的事。
//
// 主人的原话：
// ① 「在某个登录条目复制用户名或密码后，切换到所有条目都会显示『已复制用户名/密码，
//    剪贴板将在 XX 秒后清空』，不应该只要在复制的那个条目里显示就可以了吗？！」
// ② 「无论是复制了用户名、密码还是动态口令，动态口令的显示都会隐藏，为什么动态口令要
//    隐藏，一直显示不好吗？如果复制的是动态口令，还可以看到它有没有变到下一个。」
//
// 两个都是**状态归属**问题：剪贴板全局、提示行却属于某一条详情页；动态口令的复位判据
// 写成了「item 变了个对象」而不是「条目 id 变了」。第二条尤其隐蔽——复制会写使用埋点，
// 埋点会 `load()`，`selectedItem` 于是变成新对象引用，复位 effect 白跑一次，`totpCode`
// 被清成空串，而重新取码要等下一个 30 秒窗口（界面上就是「复制完口令就没了」）。

import { describe, expect, it } from 'vitest';

import { shouldShowCopyHint, type CopyHintState } from './copied-field';

const components = import.meta.glob('../components/ItemDetail.svelte', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const detail = Object.values(components)[0] ?? '';

/** 取 `[from, to)` 之间的片段；找不到标记直接失败（而不是悄悄返回空串）。 */
function section(source: string, from: string, to: string, label: string): string {
  const start = source.indexOf(from);
  expect(start, `${label}：起点标记 ${from} 不存在`).toBeGreaterThanOrEqual(0);
  const end = source.indexOf(to, start);
  expect(end, `${label}：终点标记 ${to} 不存在`).toBeGreaterThan(start);
  return source.slice(start, end);
}

function hint(over: Partial<CopyHintState> = {}): boolean {
  return shouldShowCopyHint({
    countdown: 23,
    field: 'password',
    copiedItemId: 'A',
    currentItemId: 'A',
    ...over,
  });
}

describe('D-168 提示行归属（行为判据）', () => {
  it('在复制的那个条目上显示', () => {
    expect(hint()).toBe(true);
  });

  it('切到别的条目立刻不显示（这就是主人报的现象）', () => {
    expect(hint({ currentItemId: 'B' })).toBe(false);
  });

  it('切走再切回来仍然显示（复制还在有效期内，剩余秒数照常）', () => {
    // 实现若在切换条目时把 copiedItemId 抹掉，这条会红——「切回来看不到进度」也是错。
    expect(hint({ currentItemId: 'B' })).toBe(false);
    expect(hint({ currentItemId: 'A' })).toBe(true);
  });

  it('详情页没有条目时不得显示（两边都是 null 时相等判断会误判）', () => {
    // 这是唯一需要额外 `copiedItemId !== null` 的原因：null === null 为真。
    expect(hint({ copiedItemId: null, currentItemId: null })).toBe(false);
    expect(hint({ copiedItemId: 'A', currentItemId: null })).toBe(false);
    expect(hint({ copiedItemId: null, currentItemId: 'A' })).toBe(false);
  });

  it('倒计时归零、或字段已被清空时不显示', () => {
    expect(hint({ countdown: 0 })).toBe(false);
    expect(hint({ field: null })).toBe(false);
  });

  it('三个字段都走同一条判定（不是只对密码生效）', () => {
    for (const field of ['username', 'password', 'totp'] as const) {
      expect(hint({ field }), field).toBe(true);
      expect(hint({ field, currentItemId: 'B' }), field).toBe(false);
    }
  });
});

describe('D-168 提示行接线', () => {
  it('渲染由 copyHintVisible 支配，四个入参全部来自真实状态', () => {
    expect(detail).toContain('{#if copyHintVisible}');
    const block = section(detail, 'const copyHintVisible = $derived(', '});', 'copyHintVisible');
    for (const key of ['countdown: copyCountdown', 'field: copiedField', 'copiedItemId,']) {
      expect(block, key).toContain(key);
    }
    // 当前条目必须现取——写成 `currentItemId: copiedItemId` 会让条件恒真，
    // 而所有「出现过 currentItemId 这个词」的断言都还是绿的。
    expect(block).toContain('currentItemId: item?.item_id ?? null');
  });

  it('复制成功后记下条目归属，归零时一并清空', () => {
    const copyFn = section(detail, 'async function copyText(', 'async function openUrl(', 'copyText');
    expect(copyFn).toContain('copiedItemId = item?.item_id ?? null');
    // 归零分支里必须同时清字段与归属，否则「剪贴板空了还挂着一行提示」。
    const zeroAt = copyFn.indexOf('if (remaining === 0)');
    expect(zeroAt).toBeGreaterThan(0);
    const zeroBlock = copyFn.slice(zeroAt, zeroAt + 400);
    expect(zeroBlock).toContain('copiedField = null');
    expect(zeroBlock).toContain('copiedItemId = null');
  });
});

describe('D-168 秘密状态的复位按条目 id，不按对象引用', () => {
  const resetBlock = section(detail, 'let lastResetId', 'const totpSecret = $derived', '复位 effect');

  it('复位前先比 id，同一条目直接返回', () => {
    expect(resetBlock).toContain('if (id === lastResetId)');
    expect(resetBlock).toContain('return;');
  });

  it('守卫必须排在清空之前（放在后面等于没写）', () => {
    // 「测了代码写着、没测代码生效」的典型位置：`return` 若在 `totpCode = ''` 之后，
    // 口令照样被清空，而所有字符串判据都还在。
    const guardAt = resetBlock.indexOf('if (id === lastResetId)');
    const clearAt = resetBlock.indexOf("totpCode = ''");
    expect(guardAt).toBeGreaterThanOrEqual(0);
    expect(clearAt).toBeGreaterThan(guardAt);
    expect(resetBlock.indexOf('return;')).toBeGreaterThan(guardAt);
    expect(resetBlock.indexOf('return;')).toBeLessThan(clearAt);
  });

  it('切换条目仍然复位（id 变了就该重置）', () => {
    // 反向约束：不能为了让口令不消失就把整个复位删掉——换条目后还显示上一条的 code 更糟。
    expect(resetBlock).toContain('lastResetId = id;');
    expect(resetBlock).toContain('revealed = false;');
    expect(resetBlock).toContain("totpCode = '';");
  });
});

describe('D-168 触发源仍在（否则前两条无从谈起）', () => {
  it('复制后仍写使用埋点——它正是「load() → 新对象引用」的来源', () => {
    const copyFn = section(detail, 'async function copyText(', 'async function openUrl(', 'copyText');
    expect(copyFn).toContain('recordItemUsage');
  });
});
