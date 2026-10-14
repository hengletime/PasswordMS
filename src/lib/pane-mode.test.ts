// 右侧面板 reducer（D-31 回归）：点列表条目必须退出编辑 / 新建态。

import { describe, expect, it } from 'vitest';

import { PANE_DETAIL, paneReducer, type PaneEvent, type PaneMode } from './pane-mode';

describe('paneReducer（右侧面板状态机）', () => {
  it('new → 进入对应 kind 的新建态', () => {
    expect(paneReducer(PANE_DETAIL, { type: 'new', kind: 'login' })).toEqual({
      view: 'create',
      kind: 'login',
    });
    expect(paneReducer(PANE_DETAIL, { type: 'new', kind: 'secure_note' })).toEqual({
      view: 'create',
      kind: 'secure_note',
    });
  });

  it('edit → 进入编辑态', () => {
    expect(paneReducer(PANE_DETAIL, { type: 'edit' })).toEqual({ view: 'edit' });
  });

  it('done → 回到只读详情', () => {
    expect(paneReducer({ view: 'edit' }, { type: 'done' })).toEqual({ view: 'detail' });
    expect(paneReducer({ view: 'create', kind: 'login' }, { type: 'done' })).toEqual({ view: 'detail' });
  });

  it('section → 回到只读详情', () => {
    expect(paneReducer({ view: 'edit' }, { type: 'section' })).toEqual({ view: 'detail' });
    expect(paneReducer({ view: 'create', kind: 'secure_note' }, { type: 'section' })).toEqual({
      view: 'detail',
    });
  });

  // —— D-31 核心回归：列表选中必须退出编辑 / 新建态。
  it('[D-31] create + select → detail（两种 kind）', () => {
    expect(paneReducer({ view: 'create', kind: 'login' }, { type: 'select' })).toEqual({ view: 'detail' });
    expect(paneReducer({ view: 'create', kind: 'secure_note' }, { type: 'select' })).toEqual({
      view: 'detail',
    });
  });

  it('[D-31] edit + select → detail', () => {
    expect(paneReducer({ view: 'edit' }, { type: 'select' })).toEqual({ view: 'detail' });
  });

  it('detail + select → detail（幂等）', () => {
    expect(paneReducer(PANE_DETAIL, { type: 'select' })).toEqual({ view: 'detail' });
  });

  it('[不变量] 任何状态下 select 的结果永远不是 create / edit', () => {
    const modes: PaneMode[] = [
      PANE_DETAIL,
      { view: 'create', kind: 'login' },
      { view: 'create', kind: 'secure_note' },
      { view: 'edit' },
    ];
    const select: PaneEvent = { type: 'select' };
    for (const mode of modes) {
      expect(paneReducer(mode, select).view).toBe('detail');
    }
  });
});
