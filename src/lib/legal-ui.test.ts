// D-63 交互防回归（源码结构断言）。
//
// 本工单最容易复发的 bug：把「免责声明 / 用户协议」两个链接写进 `<label class="pms-check">` 内部。
// HTML 规范里 `<label>` 会把**内部任意可点元素**的点击隐式转发给关联控件，于是用户点「用户协议」
// 会顺手把勾选取消掉——弹窗打开、`acknowledged` 变 false、创建按钮变灰，且不报任何错。
// 正确结构是：label 只包 checkbox + 前缀文字，两个链接作为 label 的**兄弟节点**并列。
//
// 用 `?raw` 读源码做结构断言（与 ipc-contract.test.ts / i18n-usage.test.ts 同一套写法），
// 不开浏览器也能守住这条交互契约。

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob('../components/{CreateVaultWizard,Settings,LegalDialog}.svelte', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const wizard: string = modules['../components/CreateVaultWizard.svelte'] ?? '';
const settings: string = modules['../components/Settings.svelte'] ?? '';
const legalDialog: string = modules['../components/LegalDialog.svelte'] ?? '';

/** 取出 `<label class="pms-check">…</label>` 的内部区间（第一个出现处）。 */
function labelBlock(source: string): { start: number; end: number; inner: string } {
  const openTag = '<label class="pms-check">';
  const start = source.indexOf(openTag);
  expect(start, 'CreateVaultWizard 里应有 label.pms-check').toBeGreaterThan(-1);
  const closeAt = source.indexOf('</label>', start);
  expect(closeAt, 'label.pms-check 应有闭合标签').toBeGreaterThan(start);
  return { start, end: closeAt + '</label>'.length, inner: source.slice(start, closeAt) };
}

/** 取出 `function name(...) { ... }` 的函数体（按缩进两个空格的闭合括号截断）。 */
function functionBody(source: string, name: string): string {
  const start = source.indexOf(`function ${name}(`);
  expect(start, `应存在函数 ${name}`).toBeGreaterThan(-1);
  const end = source.indexOf('\n  }', start);
  expect(end, `函数 ${name} 应有闭合括号`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe('D-63 勾选行结构：链接必须是 label 的兄弟节点', () => {
  it('读取到三个相关组件源码', () => {
    expect(wizard.length).toBeGreaterThan(1000);
    expect(settings.length).toBeGreaterThan(1000);
    expect(legalDialog.length).toBeGreaterThan(500);
  });

  it('label.pms-check 内部只有 checkbox，不含任何 <button>', () => {
    const { inner } = labelBlock(wizard);
    // 只有一个绑定 acknowledged 的复选框。
    const checkboxes = inner.match(/<input[^>]*type="checkbox"[^>]*>/g) ?? [];
    expect(checkboxes).toHaveLength(1);
    expect(inner).toContain('bind:checked={acknowledged}');
    // 关键：不许有按钮/链接钻进 label（否则点击会被隐式转发成 toggle）。
    expect(inner).not.toContain('<button');
    expect(inner).not.toContain('pms-link');
  });

  it('两个 .pms-link 链接都在 </label> 之后，且分列 acknowledge_middle 两侧', () => {
    const { end } = labelBlock(wizard);
    const after = wizard.slice(end);
    const disclaimerAt = after.indexOf(`openLegal('disclaimer')`);
    const termsAt = after.indexOf(`openLegal('terms')`);
    const middleAt = after.indexOf('create.acknowledge_middle');
    expect(disclaimerAt).toBeGreaterThan(-1);
    expect(termsAt).toBeGreaterThan(-1);
    expect(middleAt).toBeGreaterThan(-1);
    // 顺序：免责声明 → 「与」 → 用户协议（视觉上读成一句话）。
    expect(disclaimerAt).toBeLessThan(middleAt);
    expect(middleAt).toBeLessThan(termsAt);
    // 两个链接都用共享的 .pms-link 类（§3 唯一权威），不是临时内联样式。
    expect(after.match(/class="pms-link"/g) ?? []).toHaveLength(2);
  });

  it('openLegal() 只写弹窗状态，绝不触碰 acknowledged 门禁', () => {
    const body = functionBody(wizard, 'openLegal');
    expect(body).toContain('legalKind');
    expect(body).toContain('legalOpen');
    expect(body).not.toContain('acknowledged');
  });

  it('canSubmit 门禁仍包含 acknowledged 与最小长度约束（未被拆掉）', () => {
    // Svelte 5 里是 `let canSubmit = $derived(expr)`，跨行书写，按 $derived(...) 整块取。
    const match = wizard.match(/(?:const|let)\s+canSubmit\s*=\s*\$derived\(([\s\S]*?)\);/);
    expect(match, '应存在 canSubmit = $derived(...) 派生）。').not.toBeNull();
    const expr = match?.[1] ?? '';
    expect(expr).toContain('acknowledged');
    expect(expr).toContain('MIN_MASTER_PASSWORD_LEN');
  });

  it('LegalDialog 在两个入口都挂载（创建向导 + 设置「关于」）', () => {
    expect(wizard).toContain('<LegalDialog bind:open={legalOpen} kind={legalKind} />');
    expect(settings).toContain('<LegalDialog bind:open={legalOpen} kind={legalKind} />');
  });

  it('「关于」的版本号来自运行时 getVersion()，不是硬编码字符串', () => {
    expect(settings).toContain("from '@tauri-apps/api/app'");
    expect(settings).toContain('getVersion()');
    // 模板里出现的是 appVersion 变量 + 「—」兜底，不是写死的版本号。
    expect(settings).toContain("{appVersion ?? '—'}");
    expect(settings).not.toMatch(/about_version[^\n]*\d+\.\d+\.\d+/);
  });

  it('项目主页走安全校验后再开外链（不直连 opener）', () => {
    const body = functionBody(settings, 'openHomepage');
    expect(body).toContain('isSafeUrl(');
    expect(body).toContain('api.system.openUrl(');
    expect(body).toContain('err.unsafe_url');
  });
});
