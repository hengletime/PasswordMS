// D-75：配对成功后保留会话码供核对 —— 三层守门。
//
// 缺陷（主人真机实测）：手机扫码成功 → 手机端弹「请核对电脑上的会话码」，而**电脑屏的
// 配对面板在这一瞬间已经关了**，会话码消失；重新生成二维码又是一个新会话、新码。
// ⇒ D-69 两屏核对会话码（防中间人）**从未真正可用**。
//
// 根因不是显示 bug，是**时序互斥**：`pairing://ended` 在成功与失败两种结局都发，
// 前端无条件 `sync.pairing = null`，而会话码正好挂在这个对象上 ——
// 「会话结束」事件把「核对会话码」这个动作的前提一起删掉了。
//
// 三层：
//  ① 纯函数 `nextPairingState(payload)` —— 语义（成功不清屏 / 未知载荷保守当失败）；
//  ② store 的**结构**：handler 必须先算状态再决定清不清屏（不许退回无条件清空）；
//  ③ 组件 `PairShow.svelte`：成功态分支存在、在二维码分支**之前**、且成功态里没有二维码。

import { describe, expect, it } from 'vitest';
import { nextPairingState, PAIRING_ENDED_EVENT, PAIRING_PAYLOAD_COMPLETED } from './pairing-state';

// ⚠️ `import.meta.glob` 的键**不由我们决定**：Vite 会把它规整成「相对 glob 基目录」的形态，
// 实测 `../lib/stores/sync.svelte.ts` 的键是 `./stores/sync.svelte.ts`（不是 `../lib/...`）。
// 硬写键名会在换写法时静默读到 `''` → 断言全绿（假绿），所以这里按**后缀**取，并单独断言取到了。
const storeModules = import.meta.glob('../lib/stores/*.svelte.ts', {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;
const pairShowModules = import.meta.glob('../components/sync/PairShow.svelte', {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

/**
 * 按键名**后缀**取源码（不依赖 Vite 的键规整规则）。
 *
 * @param modules glob 结果
 * @param suffix 键名后缀，例如 `stores/sync.svelte.ts`
 * @returns 源码；未命中返回空串（由「扫描面有效」那条断言负责判红）
 */
function pickSource(modules: Record<string, string>, suffix: string): string {
  const key = Object.keys(modules).find((k) => k.endsWith(suffix));
  return key === undefined ? '' : modules[key];
}

const storeSrc = pickSource(storeModules, 'stores/sync.svelte.ts');
const pairShowSrc = pickSource(pairShowModules, 'sync/PairShow.svelte');

/** 取模板部分（`</script>` 之后），只看真正上屏的结构。 */
function template(source: string): string {
  const at = source.indexOf('</script>');
  expect(at, '组件应有 </script> 结束脚本块').toBeGreaterThan(-1);
  return source.slice(at);
}

/** 取 `listen(PAIRING_ENDED_EVENT, …)` 的回调体（到 `});` 为止）。 */
function pairingEndedHandler(source: string): string {
  const at = source.indexOf('listen(PAIRING_ENDED_EVENT');
  expect(at, 'store 应监听 PAIRING_ENDED_EVENT').toBeGreaterThan(-1);
  const end = source.indexOf('});', at);
  expect(end).toBeGreaterThan(at);
  return source.slice(at, end);
}

describe('D-75 ①：nextPairingState —— 载荷语义', () => {
  it('扫描面有效：两个源文件都读到了（防 glob 写错退化成空断言假绿）', () => {
    expect(storeSrc.length).toBeGreaterThan(2000);
    expect(pairShowSrc.length).toBeGreaterThan(1000);
    expect(storeSrc).toContain('PAIRING_ENDED_EVENT');
  });

  it('事件名与载荷常量是后端那两个字面量（改了这里就等于改了协议）', () => {
    expect(PAIRING_ENDED_EVENT).toBe('pairing://ended');
    expect(PAIRING_PAYLOAD_COMPLETED).toBe('completed');
  });

  it('completed → 保留 pairing（核心回归锚点：清屏就等于把核对时机删掉）', () => {
    expect(nextPairingState('completed')).toEqual({
      clearPairing: false,
      completed: true,
      ended: false,
    });
  });

  it('aborted → 清屏并进入「请重新生成二维码」态', () => {
    expect(nextPairingState('aborted')).toEqual({
      clearPairing: true,
      completed: false,
      ended: true,
    });
  });

  it('未知载荷（空串 / null / undefined / 大小写不符）一律按失败保守处理，不留假成功态', () => {
    for (const payload of ['', null, undefined, 'COMPLETED', 'Completed', 'ok', 0, {}]) {
      const next = nextPairingState(payload);
      expect(next, `payload=${String(payload)} 不得被判成成功`).toEqual({
        clearPairing: true,
        completed: false,
        ended: true,
      });
    }
  });

  it('三个字段互斥：completed 与 ended 不可能同时为真，成功态必定不清屏', () => {
    for (const payload of ['completed', 'aborted', '', null, 'whatever']) {
      const next = nextPairingState(payload);
      expect(next.completed && next.ended, '不能既成功又结束').toBe(false);
      expect(next.completed === next.clearPairing, '成功 ⇔ 不清屏，二者必须反向').toBe(false);
    }
  });
});

describe('D-75 ②：store 的 handler 必须先算状态、再决定是否清屏', () => {
  it('handler 里出现 nextPairingState( 且早于任何 sync.pairing = null', () => {
    const handler = pairingEndedHandler(storeSrc);
    const calcAt = handler.indexOf('nextPairingState(');
    const clearAt = handler.indexOf('sync.pairing = null');
    expect(calcAt).toBeGreaterThan(-1);
    expect(clearAt).toBeGreaterThan(-1);
    expect(calcAt).toBeLessThan(clearAt);
    // 清屏必须在判定分支内（`if (next.clearPairing)`），不是无条件的。
    expect(handler).toContain('if (next.clearPairing)');
  });

  it('负向：回调不再直接用字面量比较载荷（判定已收敛进纯函数）', () => {
    const handler = pairingEndedHandler(storeSrc);
    // ⚠️ 正向锚点必须与负向同处一个 it：只写 `not.toContain`，在「整块被删掉」时恒绿
    //    （D-68 时代踩过这个静默失效，见 sync-ui-guard.test.ts 的同款注释）。
    expect(handler).toContain('nextPairingState(ev.payload)');
    expect(handler).not.toContain("=== 'aborted'");
    expect(handler).not.toContain("=== 'completed'");
  });

  it('两个新入口都在：pairCompleted 状态位 + endPairing()', () => {
    expect(storeSrc).toContain('pairCompleted = $state<boolean>(false)');
    expect(storeSrc).toContain('endPairing(): void');
    // 三处复位：重开一轮 / 取消 / 锁定。
    const resets = storeSrc.match(/this\.pairCompleted = false;/g) ?? [];
    expect(resets.length).toBeGreaterThanOrEqual(3);
  });

  it('handler 用常量而非字面量事件名（协议字面量只许出现在 pairing-state.ts 一处）', () => {
    expect(storeSrc).toContain('listen(PAIRING_ENDED_EVENT');
    expect(storeSrc).not.toContain("listen('pairing://ended'");
  });
});

describe('D-75 ③：PairShow 的成功态分支', () => {
  it('成功态判定用 sync.pairCompleted，且要求 pairing 仍在', () => {
    const tpl = template(pairShowSrc);
    expect(tpl).toContain('sync.pairCompleted');
    expect(tpl).toMatch(/\{#if\s+sync\.pairCompleted\s*&&\s*sync\.pairing !== null\}/);
  });

  it('成功态在二维码分支之前（否则 pairing 非空会先命中二维码分支，核对码被挤到下面）', () => {
    const tpl = template(pairShowSrc);
    const successAt = tpl.indexOf('sync.pairCompleted');
    const qrAt = tpl.indexOf('sync.pairing.qr_svg');
    expect(successAt).toBeGreaterThan(-1);
    expect(qrAt).toBeGreaterThan(-1);
    expect(successAt).toBeLessThan(qrAt);
  });

  it('成功态里显示会话码（与手机屏同值）与「完成」按钮', () => {
    const tpl = template(pairShowSrc);
    const successBlock = tpl.slice(tpl.indexOf('sync.pairCompleted'), tpl.indexOf('{:else if sync.pairing === null}'));
    expect(successBlock).toContain('sync.pairing.session_code');
    expect(successBlock).toContain("ui.t('sync.session_code')");
    expect(successBlock).toContain("ui.t('sync.pair_completed_hint')");
    expect(successBlock).toContain('sync.endPairing()');
    expect(successBlock).toContain("ui.t('sync.pair_done')");
  });

  it('成功态里不渲染二维码（已扫过，留着会让人以为还能再扫）', () => {
    const tpl = template(pairShowSrc);
    const successBlock = tpl.slice(tpl.indexOf('sync.pairCompleted'), tpl.indexOf('{:else if sync.pairing === null}'));
    expect(successBlock.length).toBeGreaterThan(200);
    expect(successBlock).not.toContain('qr_svg');
    expect(successBlock).not.toContain('sync-qr');
  });

  it('成功态仍由 hasSessionCode() 判空后才渲染会话码（空码不渲染空标签）', () => {
    const tpl = template(pairShowSrc);
    const successBlock = tpl.slice(tpl.indexOf('sync.pairCompleted'), tpl.indexOf('{:else if sync.pairing === null}'));
    expect(successBlock).toMatch(/\{#if\s+hasSessionCode\(sync\.pairing\.session_code\)\}/);
  });

  it('失败态（pairEnded）分支保持原样：仍提示重新生成二维码', () => {
    const tpl = template(pairShowSrc);
    expect(tpl).toContain("ui.t('sync.pair_ended')");
    expect(tpl).toContain('sync.pairEnded');
  });
});

describe('配对完成必须刷新（2026-10-30 主人实测缺陷）', () => {
  it('completed 分支必须重载条目/设备列表/冲突箱——否则同步页显示「没有已配对设备/从未同步」，切一圈设置页才冒出来', () => {
    const body = pairingEndedHandler(storeSrc);
    const at = body.indexOf('if (next.completed)');
    expect(at, 'completed 分支应存在').toBeGreaterThan(-1);
    const branch = body.slice(at);
    expect(branch).toContain('vault.load()');
    expect(branch).toContain('sync.loadDevices()');
    expect(branch).toContain('sync.loadConflicts()');
  });
});
