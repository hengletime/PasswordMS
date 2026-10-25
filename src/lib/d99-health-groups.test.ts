// D-99 守门（2026-10-25）：体检重复密码分组显示。
//
// 主人实测：报告「6 条重复密码」，chip 区既看不出是几组、5 个不在当前视图的
// 条目还显示裸 UUID。根因两处——后端把所有重复组拍平（分组信息丢失）、前端
// titleOf 用 vault.items（筛选后列表）解析标题。修法：HealthReport 增加
// reused_groups 分组结构 + 体检时并行拉全量条目做映射 + 按组渲染。
// 结构断言用 import.meta.glob('?raw') 读源码（svelte-check 环境无 node types，
// 禁止 import 'node:fs'——见 d100-strength.test.ts 教训）。

import { describe, expect, it } from 'vitest';
import { enUS } from './i18n/en-US';
import { zhCN } from './i18n/zh-CN';

const modules = import.meta.glob(['../components/*.svelte'], {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;
const healthSrc = modules['../components/HealthCheck.svelte'] ?? '';

describe('D-99 体检重复密码分组', () => {
  it('HealthCheck 必须用全量条目做标题映射，不得依赖筛选后的 vault.items', () => {
    expect(healthSrc).toContain('api.items.list(null, false, undefined)');
    expect(healthSrc).toContain('titleMap');
    expect(healthSrc).not.toContain('vault.items.find');
  });

  it('体检与全量拉取必须并行（Promise.all），串行会拖慢体检页', () => {
    expect(healthSrc).toContain('Promise.all');
  });

  it('重复密码区必须按 reused_groups 分组渲染，不得再拍平铺 chip', () => {
    expect(healthSrc).toContain('report.reused_groups');
    expect(healthSrc).toContain('health.reused_group');
    // 旧拍平渲染路径已删。
    expect(healthSrc).not.toMatch(/\{#each report\.reused as id/);
  });

  it('i18n 两键双语齐备：reused_group 措辞带组序号', () => {
    expect(zhCN.health.reused_group).toBe('第 {n} 组');
    expect(enUS.health.reused_group).toBe('Group {n}');
  });
});
