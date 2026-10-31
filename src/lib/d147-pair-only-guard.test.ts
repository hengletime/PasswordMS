// D-147 的源码结构守卫：配对只建立关系，不搬数据。
//
// 主人的诉求是产品语义（「有时只是想配对」），但落点在**协议两端**——所以这里守的不是
// 界面文案，而是四条一旦改错就会静默丢数据的接线：
//
//   ① 手机端配对命令必须**先** `mark_pair_only()` **再** push：顺序反了就是旧行为
//      （配对时把整库拉过去），而且不会有任何报错。
//   ② 客户端只有收到对端 `pair_only` 确认才跳过合并与落盘；对端是老版本时照常同步
//      （宁可多同步一次，也不能让对端已推进的 base 与本端实际持有对不上）。
//   ③ 服务端在 `pair_only` 分支里**不得**走 merge/export，也必须在 save 之前 return
//      —— 否则水位被推进，那批变更此后永不重发。
//   ④ 前端 join 成功后要跳「状态」页（那里才有「立即同步」），且不得自动触发同步。
//
// 行为级证明在 `crates/passwordms-core/tests/d147_pair_only.rs`（含真实 resync 闭环）；
// 这里钉的是「改的人一眼能看出自己在动什么」。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    '../../src-tauri/src/commands/sync_cmd.rs',
    '../../crates/passwordms-core/src/sync/client.rs',
    '../../crates/passwordms-core/src/sync/server.rs',
    './stores/sync.svelte.ts',
    '../components/MainLayout.svelte',
    '../components/sync/SyncStatus.svelte',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

function src(suffix: string): string {
  return Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1] ?? '';
}

const syncCmd = src('sync_cmd.rs');
const clientRs = src('sync/client.rs');
const serverRs = src('sync/server.rs');
const syncStore = src('stores/sync.svelte.ts');
const mainLayout = src('MainLayout.svelte');
const syncStatus = src('SyncStatus.svelte');

describe('D-147 配对与同步必须分家', () => {
  it('五个源文件都在扫描面内（防止守门空跑）', () => {
    for (const [name, text] of [
      ['sync_cmd.rs', syncCmd],
      ['client.rs', clientRs],
      ['server.rs', serverRs],
      ['sync.svelte.ts', syncStore],
      ['MainLayout.svelte', mainLayout],
    ] as const) {
      expect(text.length, `${name} 必须读到`).toBeGreaterThan(500);
    }
  });

  it('① 配对命令先标记 pair_only，再发起交换', () => {
    const markAt = syncCmd.indexOf('joiner.mark_pair_only()');
    const pushAt = syncCmd.indexOf('joiner.push_and_sync_access(');
    expect(markAt, '配对命令必须标记「只配对」').toBeGreaterThan(-1);
    expect(pushAt).toBeGreaterThan(-1);
    expect(
      markAt,
      'mark_pair_only 必须在 push_and_sync_access 之前——顺序反了就是旧行为，且无任何报错',
    ).toBeLessThan(pushAt);
  });

  it('② 客户端以对端确认为准，且静默不等于不清理', () => {
    expect(clientRs).toContain('pub fn mark_pair_only');
    expect(clientRs).toContain('self.pair_only = true');
    // 跳过合并的唯一依据是回包，不是我们自己发的标记。
    expect(clientRs).toContain('let skip_merge = reply.pair_only;');
    // 只配对的一轮：不写水位——「提前 return」必须发生在保存游标之前。
    const saveAt = clientRs.indexOf('state.last_recv_seq = replay.last_recv_seq()');
    // 文件是 CRLF，跨行匹配一律用正则（别用带 \n 的字面量）。
    const guardMatch = /if skip_merge \{\s*return Ok\(JoinerSyncResult/.exec(clientRs);
    const guardAt = guardMatch ? guardMatch.index : -1;
    expect(saveAt).toBeGreaterThan(-1);
    expect(guardAt, '只配对时必须有提前返回').toBeGreaterThan(-1);
    expect(guardAt, '提前返回必须在保存游标之前，否则水位被推进').toBeLessThan(saveAt);
    // blob 阶段也要显式短路（否则将来有人把 blob_support 改恒 true 就复活）。
    expect(clientRs).toContain('if !skip_merge && blob_transfer::blob_phase_enabled(');
  });

  it('③ 服务端 pair_only 分支不做 merge，且在保存前返回', () => {
    const branchAt = serverRs.indexOf('let pair_only = push.pair_only;');
    expect(branchAt, 'Host 必须读对端的 pair_only').toBeGreaterThan(-1);
    const applyAt = serverRs.indexOf('merge::apply_changes(repo, &mut st, &push.changes)');
    expect(applyAt, '普通路径的合并仍在').toBeGreaterThan(-1);
    expect(branchAt, 'pair_only 分支必须在合并之前分流').toBeLessThan(applyAt);

    // 分支体内不许出现合并/导出（截到下一个 `type SectionResult` 为止）。
    const branchEnd = serverRs.indexOf('type SectionResult', branchAt);
    const branch = serverRs.slice(branchAt, branchEnd);
    expect(branch).not.toContain('merge::apply_changes');
    expect(branch).not.toContain('merge::note_sent_base');
    expect(branch).toContain('return Ok(HostSyncResult');
  });

  it('④ 前端「立即同步」是唯一同步入口，配对流程里不得自动触发', () => {
    // 跳转用自增序号（连配两台时布尔量不会二次触发）。
    expect(syncStore).toContain('tabRequest');
    expect(mainLayout).toContain('sync.tabRequest');
    expect(mainLayout).toMatch(/syncTab = req\.tab;/);

    const joinStart = syncStore.indexOf('async join(');
    // D-148：join 之后紧跟 confirmPairing / finishPairing，两者的体内都合法地带
    // `requestTab`，所以截断点必须落在**下一个方法**上，不能到 loadDevices 为止。
    const joinEnd = syncStore.indexOf('confirmPairing(): void {');
    expect(joinStart, 'join 方法必须在').toBeGreaterThan(-1);
    expect(joinEnd, 'confirmPairing 必须紧接 join（截断点）').toBeGreaterThan(joinStart);
    const joinBody = syncStore.slice(joinStart, joinEnd);
    expect(joinBody, '配对流程不得自动触发同步').not.toContain('syncNow');
    // D-148：配对成功**不跳页**——跳页推迟到用户核对完会话码点「完成」之后，
    // 否则 D-69 的核对块在出现的同一瞬间就被页面切走（见 d148 守门）。
    expect(joinBody, 'D-148：配对成功不得自己跳页').not.toContain('requestTab');
    expect(syncStatus).toContain('sync.syncNow(deviceId)');
  });
});
