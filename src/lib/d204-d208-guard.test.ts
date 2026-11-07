// D-204 / D-206 / D-207 / D-208 四条真机反馈的守门（1.6.7）。
//
// 四条的共同点：**改回去都能编译、单测也全绿**，只有真机上肉眼才看得出来。
// 所以按本仓惯例做源码级守门 + 可行为化的纯函数断言。
//
// 读源码用 `import.meta.glob('?raw')`：本仓禁止 `node:fs`（svelte-check 不认 node types），
// 且 glob 的 key 是**相对本文件的路径**（src/lib/ → 用 './' 与 '../'）。

import { describe, expect, it } from 'vitest';

import { createResumeGate } from './app-resume';

const modules = import.meta.glob(
  [
    '../App.svelte',
    '../components/AutotypePicker.svelte',
    '../components/sync/ConflictPanel.svelte',
    './app-resume.ts',
    '../../src-tauri/src/hotkey.rs',
    '../../src-tauri/src/lib.rs',
    '../../src-tauri/src/dto.rs',
    '../types.ts',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

function src(key: string): string {
  const text = modules[key];
  expect(text, `glob 没读到 ${key}（key 是相对本测试文件的路径，写错会拿到 undefined）`).toBeTypeOf(
    'string',
  );
  return text ?? '';
}

describe('D-207：冲突卡的确认按钮不许被拉伸成整行', () => {
  it('按钮必须包在右对齐的 .conflict-actions 行里', () => {
    const panel = src('../components/sync/ConflictPanel.svelte');
    // `.conflict-item` 是 flex column，`<button>` 作为直接子项会被 align-items 拉伸成整条
    // ——主人原话「这个确认的按钮太长太丑了」。
    const itemAt = panel.indexOf('<li class="conflict-item">');
    const actionsAt = panel.indexOf('<div class="conflict-actions">');
    const buttonAt = panel.indexOf('class="pms-btn is-primary is-sm"');
    expect(itemAt, '找不到冲突卡片容器').toBeGreaterThan(-1);
    expect(actionsAt, '缺了 .conflict-actions 包裹层').toBeGreaterThan(itemAt);
    expect(buttonAt, '确认按钮必须在包裹层内部').toBeGreaterThan(actionsAt);
    // 右对齐（仓内主操作统一靠右，见 Modal / ConfirmDialog 的 footer）。
    expect(panel).toMatch(/\.conflict-actions\s*\{[^}]*justify-content:\s*flex-end/);
  });
});

describe('D-206：自动输入也是一次「使用」', () => {
  it('注入成功后必须记一次使用频次（此前只统计复制）', () => {
    const picker = src('../components/AutotypePicker.svelte');
    const fillAt = picker.indexOf('api.autotype.fill(');
    expect(fillAt, '浮层必须调用 api.autotype.fill').toBeGreaterThan(-1);
    const after = picker.slice(fillAt);
    // 只看成功分支：`catch (err)` 之后是失败路径，那条不该记使用。
    const catchAt = after.indexOf('catch (err)');
    expect(catchAt, '浮层必须有失败分支（失败不记使用）').toBeGreaterThan(-1);
    const successBranch = after.slice(0, catchAt);
    expect(
      successBranch,
      '自动输入成功没记使用 → 主人日常从浮层取用，"常用条目"永远是 0，未钉区只能按修改时间排',
    ).toContain('recordUsage(');
  });
});

describe('D-208：主热键的三个落点', () => {
  it('解锁态必须走分流，不许再无条件弹小窗', () => {
    const hot = src('../../src-tauri/src/hotkey.rs');
    // 只断言"函数存在"是装饰：函数可以在、调用点却还是老的 toggle。
    expect(hot).toMatch(/if unlocked\(app\)\s*\{\s*show_for_main_hotkey\(app\);/);
    // 反面：解锁分支不许直接 toggle 小窗（那正是被主人质疑的行为）。
    expect(hot).not.toMatch(/if unlocked\(app\)\s*\{\s*crate::quick_search::toggle/);
  });

  it('「小窗已开着 → 收起」必须排在「主窗口可见」之前', () => {
    const hot = src('../../src-tauri/src/hotkey.rs');
    const fnAt = hot.indexOf('fn show_for_main_hotkey');
    expect(fnAt, '找不到分流函数').toBeGreaterThan(-1);
    const body = hot.slice(fnAt, fnAt + 1400);
    const visibleAt = body.indexOf('quick_search::is_visible');
    const focusAt = body.indexOf('is_focused');
    expect(visibleAt, '必须先判小窗可见性，否则小窗弹出来就再也关不掉').toBeGreaterThan(-1);
    expect(focusAt).toBeGreaterThan(visibleAt);
    // 主窗口在前台 ⇒ 直接 return，不许落到 toggle。
    expect(body).toMatch(/is_focused[\s\S]{0,200}return;/);
  });
});

describe('D-205：本端已删、远端改名，卡片要说清哪边是删掉的', () => {
  it('冲突 DTO 的 local_deleted 必须一路透传到界面（漏一环 = 卡片变成两个陌生名字）', () => {
    // 三处字段名跨语言必须逐字一致：core 的 FolderConflictEntry → dto.rs → types.ts
    const panel = src('../components/sync/ConflictPanel.svelte');
    expect(panel).toContain('class:is-deleted={conflict.local_deleted}');
    expect(panel).toContain("'conflict.local_deleted'");
    expect(panel).toContain('{#if conflict.local_deleted}');

    const dto = src('../../src-tauri/src/dto.rs');
    expect(dto).toContain('pub local_deleted: bool');
    // 分组臂必须透传，不许写死 false（写死就等于界面永远不标"本端已删除"）。
    expect(dto).toContain('local_deleted: c.local_deleted');

    const types = src('../types.ts');
    expect(types).toContain('local_deleted: boolean;');
  });
});

describe('D-204：回前台刷新不能再赌单一事件源', () => {
  it('闸门必须有超时复位：refresh 永不 settle 时不能把后续的每次切回都挡在门外', async () => {
    let release: (() => void) | undefined;
    const gate = createResumeGate(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
      { timeoutMs: 5 },
    );

    gate.trigger();
    expect(gate.isIdle(), '刚触发时闸门是关的').toBe(false);
    await new Promise<void>((resolve) => setTimeout(resolve, 30));
    expect(
      gate.isIdle(),
      '查询卡住时闸门必须自己放开——否则真机表现是"只有第一次切回有效"',
    ).toBe(true);
    release?.();
  });

  it('页面可见性必须是第三条腿（安卓上 document 从不丢焦点，focus 事件不触发）', () => {
    const app = src('../App.svelte');
    expect(app).toContain("addEventListener('visibilitychange'");
    expect(app).toContain("visibilityState === 'visible'");
    expect(app).toContain('gate.trigger()');
  });

  it('宿主 emit 的结果必须留痕（前两轮失败查不出根因就是因为丢掉了返回值）', () => {
    const lib = src('../../src-tauri/src/lib.rs');
    expect(lib).toContain('[resume] 已发出');
    expect(lib).toContain('[resume] 发出 app://resumed 失败');
    // 反面：不许退回 `let _ = ...emit(...)` 那种把成败一起丢掉的写法。
    expect(lib).not.toMatch(/let _ = app_handle\.emit\(APP_RESUMED_EVENT/);
  });
});
