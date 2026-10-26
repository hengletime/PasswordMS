// D-101 守门（2026-10-26）：搜索框一键清空。
//
// 主人反馈：一个一个删搜索词麻烦，要 ❌ 一键清空。关键行为约束：
// 清空前必须取消挂着的 220ms 防抖（否则旧词随后 setQuery 又写回去——
// D-81 epoch 机制区分「外部重置」与「自己打字中」，本按钮是自发清空，
// 不得推进 epoch）。结构断言用 import.meta.glob('?raw')（svelte-check
// 环境无 node types，禁止 import 'node:fs'）。

import { describe, expect, it } from 'vitest';
import { enUS } from './i18n/en-US';
import { zhCN } from './i18n/zh-CN';

const modules = import.meta.glob(['../components/*.svelte'], {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;
const src = modules['../components/ItemList.svelte'] ?? '';

describe('D-101 搜索框一键清空', () => {
  it('清空按钮仅搜索词非空时渲染（空态不占位）', () => {
    const at = src.indexOf('list-search-clear');
    const block = src.slice(src.lastIndexOf('{#if', at), at);
    expect(block).toContain('query.length > 0');
  });

  it('clearSearch 必须先取消防抖 timer 再清空——否则旧词 setQuery 写回', () => {
    const at = src.indexOf('function clearSearch');
    const body = src.slice(at, src.indexOf('}', src.indexOf('setQuery', at)));
    const clearTimer = body.indexOf('clearTimeout');
    const setStore = body.indexOf("setQuery('')");
    expect(clearTimer).toBeGreaterThan(-1);
    expect(setStore).toBeGreaterThan(clearTimer);
    expect(body).toContain('query = ');
  });

  it('清空不得推进 queryEpoch（epoch 只属外部重置路径，D-81）', () => {
    const at = src.indexOf('function clearSearch');
    const body = src.slice(at, src.indexOf('function iconFor'));
    expect(body).not.toContain('queryEpoch');
  });

  it('按钮带 aria-label / title 双通道可及性，图标用既有 close', () => {
    const at = src.indexOf('list-search-clear');
    const block = src.slice(at, src.indexOf('{/if}', at));
    expect(block).toContain('aria-label');
    expect(block).toContain('title=');
    expect(block).toContain('name="close"');
  });

  it('i18n 两键双语齐备', () => {
    expect(zhCN.common.clear_search).toBe('清空搜索');
    expect(enUS.common.clear_search).toBe('Clear search');
  });
});
