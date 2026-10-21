// `sync://applied` 载荷判定（v1.2.4 根因 A）单测。
//
// 守的是什么：被动同步命中后前端必须刷新（否则「手机删了附件、电脑上还在」）。
// 同时守反向：三项全零时**不许**无条件 reload（会白刷列表、打断编辑）。

import { describe, expect, it } from 'vitest';

import {
  PEER_SYNC_APPLIED_EVENT,
  parsePeerSyncApplied,
  shouldReloadAfterPeerSync,
} from './sync-applied';

describe('被动同步事件：载荷归一化与刷新判定', () => {
  it('事件名与后端 emit 的字符串严格一致', () => {
    // 后端 src-tauri/src/state.rs 里 `emit(&app, "sync://applied", ...)`，改名即双向失效。
    expect(PEER_SYNC_APPLIED_EVENT).toBe('sync://applied');
  });

  it('完整载荷原样解析', () => {
    expect(parsePeerSyncApplied({ applied: 2, blobs: 1, conflicts: 0 })).toEqual({
      applied: 2,
      blobs: 1,
      conflicts: 0,
    });
  });

  it('缺字段 / 异常值一律按 0 计（不抛异常）', () => {
    expect(parsePeerSyncApplied({})).toEqual({ applied: 0, blobs: 0, conflicts: 0 });
    expect(parsePeerSyncApplied(null)).toEqual({ applied: 0, blobs: 0, conflicts: 0 });
    expect(parsePeerSyncApplied('nope')).toEqual({ applied: 0, blobs: 0, conflicts: 0 });
    expect(parsePeerSyncApplied({ applied: -3, blobs: Number.NaN, conflicts: '2' })).toEqual({
      applied: 0,
      blobs: 0,
      conflicts: 0,
    });
  });

  it('小数计数向下取整（后端只会发整数，此处只做防御）', () => {
    expect(parsePeerSyncApplied({ applied: 1.9, blobs: 0.4, conflicts: 3 }).applied).toBe(1);
    expect(parsePeerSyncApplied({ applied: 1.9, blobs: 0.4, conflicts: 3 }).blobs).toBe(0);
  });

  it('任一项非零 → 需要刷新（覆盖附件删除：applied=1 是最典型形态）', () => {
    expect(shouldReloadAfterPeerSync({ applied: 1, blobs: 0, conflicts: 0 })).toBe(true);
    expect(shouldReloadAfterPeerSync({ applied: 0, blobs: 2, conflicts: 0 })).toBe(true);
    expect(shouldReloadAfterPeerSync({ applied: 0, blobs: 0, conflicts: 1 })).toBe(true);
  });

  it('三项全零 → 不刷新（对端只是拿走了本端数据）', () => {
    expect(shouldReloadAfterPeerSync({ applied: 0, blobs: 0, conflicts: 0 })).toBe(false);
  });

  it('未知/畸形载荷 → 不刷新（宁可不刷，也不要在坏载荷上抖动列表）', () => {
    expect(shouldReloadAfterPeerSync(undefined)).toBe(false);
    expect(shouldReloadAfterPeerSync(null)).toBe(false);
    expect(shouldReloadAfterPeerSync(42)).toBe(false);
  });
});
