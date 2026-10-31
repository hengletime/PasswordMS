// D-148 的源码结构守卫：配对成功**不等于**配对结束，中间必须留一步给人核对会话码。
//
// 主人实测：「手机扫码配对就跳到状态，本次配对核对码的确认就跳过去了。扫码后等用户确认
// 信息。点击完成后手机端的配对信息应该自动收起，然后两端一起跳到状态。」
//
// 前一轮（D-147）把「配对成功 → 跳状态页」写进了 `join()`，于是 D-69 的核对块在出现的
// 同一瞬间就被页面切走 —— 防中间人的核对动作在流程上变成不可能。这里钉住四条接线：
//
//   ① `join()` 成功只置「待确认」，**不得**自己跳页；
//   ② `confirmPairing()` 先收起（清核对码）+ 换文案，**最后**才跳页；
//   ③ `PairScan` 的确认态必须优先于扫码界面（否则用户会以为没成功又扫一次，
//      第二次会把核对中的码换掉）；
//   ④ 两端「完成」都落到状态页（Host 侧走 `finishPairing`）。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    './stores/sync.svelte.ts',
    '../components/sync/PairScan.svelte',
    '../components/sync/PairShow.svelte',
    '../components/MainLayout.svelte',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

/** 取源码；**必须归一化换行**——这些文件是 CRLF，带 `\n` 的字面量会恒不命中（D-147 踩过）。 */
function src(suffix: string): string {
  const text = Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1] ?? '';
  return text.replace(/\r\n/g, '\n');
}

/** 取一个类方法的体（到下一个同缩进的 `  }` 行）。 */
function methodBody(source: string, signature: string): string {
  const at = source.indexOf(signature);
  if (at < 0) {
    return '';
  }
  const rest = source.slice(at);
  const end = rest.indexOf('\n  }\n');
  return end < 0 ? rest : rest.slice(0, end);
}

const syncStore = src('stores/sync.svelte.ts');
const pairScan = src('/PairScan.svelte');
const pairShow = src('/PairShow.svelte');
const mainLayout = src('MainLayout.svelte');

describe('D-148 配对必须留一步人工核对', () => {
  it('① join 成功只进「待确认」态，不跳页', () => {
    const joinBody = methodBody(syncStore, 'async join(qrText: string): Promise<boolean> {');
    expect(joinBody.length, 'join 方法必须能取到').toBeGreaterThan(200);
    expect(joinBody, '配对成功必须进入待确认态').toContain('this.pairAwaitingConfirm = true;');
    expect(joinBody, 'D-147 的回归：配对成功不得自己跳页，核对码会被跳过').not.toContain('requestTab');
    // 提示必须指向「核对会话码」这个下一步，而不是「去同步」——此刻还没到那一步。
    expect(joinBody).toContain("ui.toast('success', 'sync.paired_check')");
  });

  it('② confirmPairing 先收起再跳页（顺序不可颠倒）', () => {
    const body = methodBody(syncStore, 'confirmPairing(): void {');
    expect(body.length, 'confirmPairing 必须存在').toBeGreaterThan(100);
    expect(body).toContain('this.pairAwaitingConfirm = false;');
    // 「收起」的实质：本轮核对码与设备名不得留到下一次配对。
    expect(body).toContain('this.lastPairSessionCode = null;');
    expect(body).toContain('this.lastPairDeviceName = ');
    expect(body).toContain("this.requestTab('status');");

    const clearAt = Math.max(
      body.lastIndexOf('this.lastPairSessionCode = null;'),
      body.lastIndexOf('this.lastPairDeviceName = '),
    );
    const reqAt = body.indexOf("this.requestTab('status');");
    expect(reqAt, '跳页必须是最后一步，否则用户看到的是闪一下的核对码').toBeGreaterThan(clearAt);
  });

  it('③ PairScan 确认态优先于扫码界面，且扫码入口整块收起', () => {
    const ifAt = pairScan.indexOf('{#if sync.pairAwaitingConfirm}');
    const elseAt = pairScan.indexOf('{:else}', ifAt);
    const scanAt = pairScan.indexOf('void startScan()');
    const manualAt = pairScan.indexOf('sync-manual-input');

    expect(ifAt, '确认态分支必须存在').toBeGreaterThan(-1);
    expect(elseAt, '确认态必须有 else，否则扫码界面和核对码同时出现').toBeGreaterThan(ifAt);
    expect(scanAt, '扫码按钮必须在 else 分支里（确认态下不可见）').toBeGreaterThan(elseAt);
    expect(manualAt, '手动输入框也必须在 else 分支里').toBeGreaterThan(elseAt);

    // 确认态自身必须给全「核对什么、怎么结束」。
    const block = pairScan.slice(ifAt, elseAt);
    expect(block).toContain('hasSessionCode(sync.lastPairSessionCode)');
    expect(block).toContain('sync.lastPairSessionCode');
    expect(block, '核对要求是安全动作，必须用警示样式').toContain('sync.session_code_hint');
    expect(block).toContain('sync.confirmPairing()');
    expect(block, '确认态里不该再出现扫码/输入').not.toContain('startScan');
    expect(block, '确认态里不该再出现扫码/输入').not.toContain('textarea');
  });

  it('④ 两端「完成」都落到状态页', () => {
    // 手机端（Joiner）：PairScan 的完成按钮直达 confirmPairing。
    expect(pairScan).toContain('sync.confirmPairing()');
    // 电脑端（Host）：走 finishPairing（= endPairing + 跳页），不能再直接 endPairing。
    expect(pairShow).toContain('sync.finishPairing()');
    expect(pairShow, 'D-148：Host 的完成必须一并跳页').not.toContain('sync.endPairing()');

    const fin = methodBody(syncStore, 'finishPairing(): void {');
    expect(fin).toContain('this.endPairing();');
    expect(fin).toContain("this.requestTab('status');");

    // 两端都靠同一个 tabRequest 机制，跳转由 MainLayout 执行。
    expect(mainLayout).toContain('sync.tabRequest');
  });

  it('⑤ 锁定后不残留上一轮的待确认态', () => {
    const resetBody = methodBody(syncStore, 'reset(): void {');
    expect(resetBody).toContain('this.pairAwaitingConfirm = false;');
  });
});
