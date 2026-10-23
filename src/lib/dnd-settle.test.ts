// D-83 防回归守卫：松手结算的**时序**——onDrop 执行时 drag.state 必须仍持有投放目标，
// drag.end() 之后才清空。
//
// 背景：finish() 原实现先 drag.end() 再 callbacks.onDrop()，而 ItemList.handleItemDrop /
// FolderSidebar.handleFolderDrop 都在 onDrop 同步段首行读 `drag.state` 取投放目标
// → 恒为 null → 一切投放静默 no-op（真机「有边缘动画但没有拖动」）。
//
// 落点说明：结算时序抽成 `finishDrag(draggedId, onDrop)`（dnd.svelte.ts），控制器 finish()
// 只调它；本文件直接测这条可注入路径。`.svelte.ts` 的 $state 在本项目 vitest node 环境
// 可编译（stores/vault.test.ts 已证），无需组件级测试。

import { describe, expect, it } from 'vitest';

import { drag, finishDrag, type DragState } from './dnd.svelte';

describe('D-83 finishDrag：结算时序（先 onDrop 后 end）', () => {
  it('onDrop 执行时 drag.state 仍持有投放目标（overKind/overId/overBefore 原样）', () => {
    drag.begin('item', 'i1', 'GitLab', 10, 20);
    drag.setOver('folder', 'f-42', true);
    const seen: DragState[] = [];
    finishDrag('i1', (id) => {
      expect(id).toBe('i1');
      seen.push({ ...drag.state });
    });
    expect(seen.length).toBe(1);
    expect(seen[0]?.overKind).toBe('folder');
    expect(seen[0]?.overId).toBe('f-42');
    expect(seen[0]?.overBefore).toBe(true);
    expect(seen[0]?.active).toBe(true);
  });

  it('end() 只在 onDrop 之后执行：回调返回后共享状态全清空', () => {
    drag.begin('folder', 'f-1', '工作', 5, 6);
    drag.setOver('item', 'i-9', false);
    finishDrag('f-1', () => {
      // 回调同步段内目标还在（这正是 handleItemDrop 首行 `drag.state` 能读到东西的前提）。
      expect(drag.state.active).toBe(true);
      expect(drag.state.overId).toBe('i-9');
    });
    // finishDrag 返回后必须全空。
    expect(drag.state.active).toBe(false);
    expect(drag.state.kind).toBeNull();
    expect(drag.state.draggedId).toBeNull();
    expect(drag.state.overKind).toBeNull();
    expect(drag.state.overId).toBeNull();
  });

  it('end() 同步复位 x/y（视觉一致性补齐：D-83 修复令附带项）', () => {
    drag.begin('item', 'i2', '标题', 120, 88);
    drag.move(456, 320);
    expect(drag.state.x).toBe(456);
    expect(drag.state.y).toBe(320);
    drag.end();
    expect(drag.state.x).toBe(0);
    expect(drag.state.y).toBe(0);
  });
});
