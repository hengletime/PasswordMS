// D-95 移动端「向左滑动返回」守门。
//
// 无 DOM 测试环境：手势判定走纯函数（src/lib/swipe-back.ts），接线走源码结构断言
// （与 d85-ui-guard / d92-editor-layout 同族）。

import { describe, expect, it } from 'vitest';

import {
  SWIPE_BACK_THRESHOLD_PX,
  SWIPE_BACK_VERTICAL_RATIO,
  swipeBackBegin,
  swipeBackUpdate,
} from './swipe-back';

const modules = import.meta.glob(['../components/*.svelte'], {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

function pickSource(suffix: string): string {
  const key = Object.keys(modules).find((k) => k.endsWith(suffix));
  return key === undefined ? '' : modules[key];
}

describe('D-95 手势判定：向左滑（水平主导）才触发', () => {
  function begin(enabled = true, pointerType = 'touch', isPrimary = true) {
    return swipeBackBegin(pointerType, isPrimary, enabled, 1, 300, 400);
  }

  it('向左滑过阈值且水平主导 → 触发', () => {
    const s = begin();
    expect(s).not.toBeNull();
    // 水平 90px、垂直 10px：dx=90 ≥ 72 且 90 ≥ 10*2。
    expect(swipeBackUpdate(s!, 1, 300 - 90, 400 - 10)).toBe(true);
  });

  it(`水平位移不足 ${SWIPE_BACK_THRESHOLD_PX}px → 不触发`, () => {
    const s = begin();
    expect(swipeBackUpdate(s!, 1, 300 - (SWIPE_BACK_THRESHOLD_PX - 1), 400)).toBe(false);
  });

  it(`垂直主导（dy 超过 dx/${SWIPE_BACK_VERTICAL_RATIO}）→ 不触发（不与滚动打架）`, () => {
    const s = begin();
    // 水平 100px 但垂直 60px：100 < 60*2。
    expect(swipeBackUpdate(s!, 1, 300 - 100, 400 - 60)).toBe(false);
  });

  it('向右滑 → 不触发', () => {
    const s = begin();
    expect(swipeBackUpdate(s!, 1, 300 + 120, 400)).toBe(false);
  });

  it('一次手势只触发一次（触发后继续滑不再返回）', () => {
    const s = begin();
    expect(swipeBackUpdate(s!, 1, 300 - 100, 400)).toBe(true);
    expect(swipeBackUpdate(s!, 1, 300 - 300, 400)).toBe(false);
  });

  it('第二根手指（pointerId 不同）的事件被忽略', () => {
    const s = begin();
    expect(swipeBackUpdate(s!, 2, 300 - 300, 400)).toBe(false);
  });

  it('鼠标 / 触控笔不启用（桌面免疫）', () => {
    expect(begin(true, 'mouse')).toBeNull();
    expect(begin(true, 'pen')).toBeNull();
  });

  it('非主指针（isPrimary=false，多指第二指按下）不启用', () => {
    expect(begin(true, 'touch', false)).toBeNull();
  });

  it('调用方未启用（如编辑态）→ 状态为 null，move 恒不触发', () => {
    const s = begin(false);
    expect(s).toBeNull();
    expect(swipeBackUpdate(null, 1, 300 - 300, 400)).toBe(false);
  });

  it('刚好卡在「垂直主导」边界之外的水平位移不触发（dx 判定用绝对值）', () => {
    const s = begin();
    // 水平 73px（> 阈值）但垂直 40px：73 < 40*2。
    expect(swipeBackUpdate(s!, 1, 300 - 73, 400 - 40)).toBe(false);
  });
});

describe('D-95 接线守门：MainLayout 必须仅在详情查看态启用手势', () => {
  const src = pickSource('MainLayout.svelte');

  it('启用条件是「条目分区 + 非编辑态 + 有选中条目」（编辑态不启用，防丢未保存内容）', () => {
    expect(src).toContain(
      'const swipeBackEnabled = $derived(inItems && !editing && vault.selectedItemId !== null);',
    );
  });

  it('手势绑定在 pane-detail 上，触发即调既有 goBack()（不新增导航状态，D-31）', () => {
    const paneAt = src.indexOf('class="pane pane-detail"');
    expect(paneAt).toBeGreaterThan(-1);
    const tagEnd = src.indexOf('>', paneAt);
    const openTag = src.slice(paneAt, tagEnd);
    for (const ev of ['onpointerdown', 'onpointermove', 'onpointerup', 'onpointercancel']) {
      expect(openTag, `pane-detail 必须绑定 ${ev}`).toContain(ev);
    }
    // 触发路径：swipeBackUpdate(...) 为真 → goBack()。
    const moveAt = src.indexOf('function onDetailPointerMove');
    expect(src.slice(moveAt, src.indexOf('}', moveAt))).toContain('goBack()');
  });

  it('pointerup / pointercancel 都要清手势状态（跨手势不复用）', () => {
    const endAt = src.indexOf('function onDetailPointerEnd');
    const block = src.slice(endAt, src.indexOf('}', endAt));
    expect(block).toContain('swipeState = null');
  });
});
