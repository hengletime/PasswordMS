// #531 拖拽纯逻辑守卫：阈值 / 长按 / 插入位置 / 钉住顺序。
// 这些函数决定「松手后发什么 IPC」，判错一位用户就白拖一次 —— 必须离屏钉死。

import { describe, expect, it } from 'vitest';
import {
  AUTOSCROLL_EDGE_PX,
  DRAG_START_THRESHOLD_PX,
  LONG_PRESS_MS,
  autoScrollDelta,
  computeDropTarget,
  exceededDragThreshold,
  insertBefore,
  isLongPress,
  pinnedOrderAfterDrop,
  type RowRect,
} from './dnd';

describe('#531 拖拽阈值与长按', () => {
  it('位移不超过阈值不算拖（点击语义保留）', () => {
    expect(exceededDragThreshold(100, 100, 103, 104)).toBe(false); // 距离 5 ≤ 6
    expect(exceededDragThreshold(100, 100, 100, 100)).toBe(false);
  });

  it('位移超过阈值即拖（欧氏距离，斜向也算）', () => {
    expect(exceededDragThreshold(100, 100, 105, 106)).toBe(true); // √61 ≈ 7.8 > 6
    expect(exceededDragThreshold(100, 100, 112, 100)).toBe(true);
  });

  it('阈值常量 = 6px、长按 = 220ms（客户拍板的交互参数，改了要过一遍真机）', () => {
    expect(DRAG_START_THRESHOLD_PX).toBe(6);
    expect(LONG_PRESS_MS).toBe(220);
  });

  it('长按判定：到点即武装，早于阈值不武装', () => {
    expect(isLongPress(219)).toBe(false);
    expect(isLongPress(220)).toBe(true);
    expect(isLongPress(1000)).toBe(true);
  });
});

describe('#531 插入位置', () => {
  it('行上半 = before，行下半 = after（56px 行的中点是分界）', () => {
    expect(insertBefore(100, 100, 156)).toBe(true); // 上半
    expect(insertBefore(127, 100, 156)).toBe(true); // 中点 128 之前
    expect(insertBefore(129, 100, 156)).toBe(false); // 中点之后
    expect(insertBefore(156, 100, 156)).toBe(false);
  });

  it('computeDropTarget：命中行给 (id, before)，列表外 = null', () => {
    const rows: RowRect[] = [
      { id: 'a', top: 0, bottom: 56 },
      { id: 'b', top: 56, bottom: 112 },
    ];
    expect(computeDropTarget(10, rows)).toEqual({ targetId: 'a', before: true });
    expect(computeDropTarget(56.4, rows)).toEqual({ targetId: 'b', before: true }); // b 的上半
    expect(computeDropTarget(90, rows)).toEqual({ targetId: 'b', before: false }); // b 的下半
    expect(computeDropTarget(111, rows)).toEqual({ targetId: 'b', before: false });
    expect(computeDropTarget(-5, rows)).toBeNull();
    expect(computeDropTarget(500, rows)).toBeNull();
  });
});

describe('#531 钉住顺序 pinnedOrderAfterDrop', () => {
  it('已钉互拖：a 拖到 c 之前 → [b? 不对] —— 从 [a,b,c] 把 a 插到 c 前 = [b,a,c]', () => {
    expect(pinnedOrderAfterDrop(['a', 'b', 'c'], 'a', 'c', true)).toEqual(['b', 'a', 'c']);
  });

  it('已钉互拖：c 拖到 a 之后 → [a,c,b]（b 被顶到后面）', () => {
    expect(pinnedOrderAfterDrop(['a', 'b', 'c'], 'c', 'a', false)).toEqual(['a', 'c', 'b']);
  });

  it('已钉拖到未钉区（目标 null）→ 挪到钉住块末尾', () => {
    expect(pinnedOrderAfterDrop(['a', 'b'], 'a', null, true)).toEqual(['b', 'a']);
  });

  it('未钉拖到钉住块内 → 钉住并插入目标位置', () => {
    expect(pinnedOrderAfterDrop(['a', 'b'], 'x', 'a', true)).toEqual(['x', 'a', 'b']);
    expect(pinnedOrderAfterDrop(['a', 'b'], 'x', 'b', false)).toEqual(['a', 'b', 'x']);
  });

  it('未钉拖到空白处（目标 null）→ null（频度序原样，零 IPC；D-84 语义保留）', () => {
    expect(pinnedOrderAfterDrop(['a'], 'x', null, true)).toBeNull();
    expect(pinnedOrderAfterDrop([], 'x', null, false)).toBeNull();
  });

  it('拖回原位 → null（不调 IPC）：a 本来就在 b 前，插回 b 之前等于没动', () => {
    expect(pinnedOrderAfterDrop(['a', 'b'], 'a', 'b', true)).toBeNull();
    // b 本来就在 a 后，插回 a 之后等于没动。
    expect(pinnedOrderAfterDrop(['a', 'b'], 'b', 'a', false)).toBeNull();
  });

  it('拖到自己上 → null', () => {
    expect(pinnedOrderAfterDrop(['a', 'b'], 'a', 'a', true)).toBeNull();
  });
});

describe('D-84 未钉区手动排序（收编语义）', () => {
  it('A、B 都未钉 → 一并收编进钉住块：before → [A, B]，after → [B, A]', () => {
    // 空钉住块：A、B 相邻置于块首。
    expect(pinnedOrderAfterDrop([], 'x', 'y', true)).toEqual(['x', 'y']);
    expect(pinnedOrderAfterDrop([], 'x', 'y', false)).toEqual(['y', 'x']);
    // 已有钉住块 p：两者相邻缀在块后 —— [...钉住块, A, B] / [...钉住块, B, A]。
    expect(pinnedOrderAfterDrop(['p'], 'x', 'y', true)).toEqual(['p', 'x', 'y']);
    expect(pinnedOrderAfterDrop(['p'], 'x', 'y', false)).toEqual(['p', 'y', 'x']);
  });

  it('A 已钉、B 未钉 → B 收编，A 插在 B 前/后（原「防御 null」路径改为收编）', () => {
    expect(pinnedOrderAfterDrop(['a', 'b'], 'b', 'y', true)).toEqual(['a', 'b', 'y']);
    expect(pinnedOrderAfterDrop(['a', 'b'], 'b', 'y', false)).toEqual(['a', 'y', 'b']);
    expect(pinnedOrderAfterDrop(['a'], 'a', 'ghost', true)).toEqual(['a', 'ghost']);
  });

  it('A 未钉、B 已钉 → 既有行为不变（A 收编插 B 前/后）', () => {
    expect(pinnedOrderAfterDrop(['a', 'b'], 'x', 'a', true)).toEqual(['x', 'a', 'b']);
    expect(pinnedOrderAfterDrop(['a', 'b'], 'x', 'b', false)).toEqual(['a', 'b', 'x']);
  });

  it('目标已在钉住集合 → 不重复收编（结果长度不膨胀）', () => {
    expect(pinnedOrderAfterDrop(['a', 'b'], 'x', 'a', true)).toEqual(['x', 'a', 'b']);
    expect(pinnedOrderAfterDrop(['a', 'b'], 'a', 'b', true)).toBeNull(); // 拖回原位仍零 IPC
  });

  it('dragged === target → 仍 null；未钉拖到 null → 仍 null（不变量复钉）', () => {
    expect(pinnedOrderAfterDrop([], 'x', 'x', true)).toBeNull();
    expect(pinnedOrderAfterDrop([], 'x', null, true)).toBeNull();
  });

  it('收编场景永不误判「拖回原位」：next 必然变长，恒发 IPC', () => {
    // x、y 都未钉：即使视觉上「x 本来就在 y 前」，收编也是状态变更。
    const order = pinnedOrderAfterDrop([], 'x', 'y', true);
    expect(order).toEqual(['x', 'y']);
  });
});

describe('#531 自动滚动', () => {
  const TOP = 0;
  const BOTTOM = 800;
  const EDGE = AUTOSCROLL_EDGE_PX; // 48

  it('指针在顶部感应区 → 向上滚，越深入越快', () => {
    expect(autoScrollDelta(EDGE - 1, TOP, BOTTOM)).toBeLessThan(0);
    const shallow = autoScrollDelta(EDGE - 10, TOP, BOTTOM);
    const deep = autoScrollDelta(2, TOP, BOTTOM);
    expect(Math.abs(deep)).toBeGreaterThan(Math.abs(shallow));
  });

  it('指针在底部感应区 → 向下滚', () => {
    expect(autoScrollDelta(BOTTOM - 10, TOP, BOTTOM)).toBeGreaterThan(0);
  });

  it('底部预留把「死区」算进去：预留后更高的位置就触发滚动（移动端导航遮挡 56px）', () => {
    // 无预留：BOTTOM-100 距容器底 100 > 48 感应区 → 不滚。
    expect(autoScrollDelta(BOTTOM - 100, TOP, BOTTOM, 0)).toBe(0);
    // 预留 56：可用底 = BOTTOM-56，感应区起点 = BOTTOM-104 → BOTTOM-100 已在内 → 滚。
    expect(autoScrollDelta(BOTTOM - 100, TOP, BOTTOM, 56)).toBeGreaterThan(0);
  });

  it('感应区之外不滚', () => {
    expect(autoScrollDelta(400, TOP, BOTTOM)).toBe(0);
    expect(autoScrollDelta(400, TOP, BOTTOM, 56)).toBe(0);
  });
});
