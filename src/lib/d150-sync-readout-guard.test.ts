// D-150 / D-151 的源码结构守卫：两个「永远不动」的读数。
//
// 主人真机 2026-10-31：同步过很多次，手机与电脑的「已配对设备」里仍然写着
// 「上次同步：从未」，电脑「状态」页的「待同步变更」恒为 58。
//
// 三条根因，三种完全不同的失效方式，共同点是**都不会报错**：
//
//   ① D-150：`syncNow()` 成功回调里只刷了 status / conflicts，**没刷 devices**。
//      设备页用的是解锁时那一份快照——同步写进了库，界面永远看不到。纯前端，
//      不修就永远是「从未」。
//   ② D-150：重配对时 `persist_host_device` / `persist_device` 无条件写
//      `last_sync_at: None`。重配对是排查同步问题的常规动作，配完反而更像没同步过。
//   ③ D-151：「待同步」取最落后对端（口径本身没错），但残影设备的 sync_state 是空的，
//      `export_changes(空状态)` 恒等于全库条数 → 数字被顶住且**没有主语**。
//      修法是归因（数字不变、不隐藏），不是把数字改小。
//
// 这里钉的是接线，不是行为：行为由 `crates/passwordms-core/tests/d150_last_sync_at.rs`
// （真 socket 重同步 + 重配对）与 `sync::status` 单测负责。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    '../lib/stores/sync.svelte.ts',
    '../components/sync/SyncStatus.svelte',
    '../lib/i18n/zh-CN.ts',
    '../lib/i18n/zh-TW.ts',
    '../lib/i18n/en-US.ts',
    '../lib/i18n/th.ts',
    '../../src-tauri/src/dto.rs',
    '../../src-tauri/src/commands/sync_cmd.rs',
    '../../crates/passwordms-core/src/sync/client.rs',
    '../../crates/passwordms-core/src/sync/server.rs',
    '../../crates/passwordms-core/src/sync/status.rs',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

/** 取源码；**必须归一化换行**——这些文件是 CRLF，带 `\n` 的字面量会恒不命中（D-147 踩过）。 */
function src(suffix: string): string {
  const text = Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1] ?? '';
  return text.replace(/\r\n/g, '\n');
}

describe('D-150：同步成功后设备列表必须跟着刷', () => {
  it('`syncNow` 回调里必须 reload 设备列表（否则「上次同步」永远是加载那一刻的值）', () => {
    const store = src('/stores/sync.svelte.ts');
    const start = store.indexOf('async syncNow(');
    expect(start, '找不到 syncNow，测试已失效').toBeGreaterThan(-1);
    // 截断点必须取**紧邻的下一个方法**：D-148 曾把截断点取到更远的方法上，
    // 中间插入的代码让断言变成假红/假绿（同一条教训）。
    const end = store.indexOf('choiceOf(', start);
    expect(end, 'syncNow 之后应紧接 choiceOf').toBeGreaterThan(start);
    const body = store.slice(start, end);

    expect(body).toContain('await this.loadDevices();');
    // 状态与冲突早就刷了，这里钉的是「三份都要刷」，少一份就是本次的缺陷。
    expect(body).toContain('await this.loadStatus();');
    expect(body).toContain('await this.loadConflicts();');
  });

  it('重配对不得把「上次同步」清回 None（两端各一处）', () => {
    const client = src('/sync/client.rs');
    const server = src('/sync/server.rs');

    // 必须从既有记录继承：出现「取旧值」的调用点。
    expect(client).toContain('.get_paired_device(&self.host_id)');
    expect(server).toContain('.get_paired_device(device_id)');
    // 结构体字面量里要写字段名（简写），而不是硬编码 None。
    expect(client).toMatch(/last_sync_at,\n\s+peer_ip/);
    expect(server).toMatch(/last_sync_at,\n\s+peer_ip/);
    // 反向断言：任何一处重新变成无条件 None 都必须红。
    expect(client).not.toMatch(/last_sync_at: None,\n\s+peer_ip/);
    expect(server).not.toMatch(/last_sync_at: None,\n\s+peer_ip/);
  });
});

describe('D-151：「待同步」这个数字必须有主语', () => {
  it('后端把归因算在 core（可测），命令层只做转发', () => {
    expect(src('/commands/sync_cmd.rs')).toContain(
      'passwordms_core::sync::status::pending_summary(repo)',
    );
    expect(src('/sync/status.rs')).toContain('pub fn pending_summary(');
  });

  it('DTO 暴露归因字段（名字 + 是否从未同步）', () => {
    const dto = src('/dto.rs');
    expect(dto).toContain('pub pending_device_name: Option<String>,');
    expect(dto).toContain('pub pending_device_never_synced: bool,');
  });

  it('归因行必须挂在上面的数字上，且「从未同步」走另一条文案', () => {
    const svelte = src('/SyncStatus.svelte');
    // 只判 pending_changes > 0 会在没有归因时渲染出半句话；两个条件缺一不可。
    expect(svelte).toMatch(
      /\{#if sync\.status\.pending_changes > 0 && sync\.status\.pending_device_name\}/,
    );
    expect(svelte).toContain(`ui.t('sync.pending_from_never'`);
    expect(svelte).toContain(`ui.t('sync.pending_from'`);
  });

  it('四语言都有这两条文案（i18n.test.ts 只比对键集合，这里额外钉语义分叉）', () => {
    for (const lang of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
      const dict = src(`/i18n/${lang}.ts`);
      expect(dict, `${lang} 缺 pending_from`).toContain('pending_from:');
      expect(dict, `${lang} 缺 pending_from_never`).toContain('pending_from_never:');
      expect(dict, `${lang} 的归因文案必须带 {name} 占位`).toMatch(/pending_from: '[^']*\{name\}/);
    }
  });
});
