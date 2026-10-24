// D-86 导入弹窗 UI 守卫：「确认导入」常驻 footer + 选完文件自动预览。
//
// 背景：旧版确认按钮沉在弹窗内容最底部，Modal `.dialog` max-height 85vh + body
// overflow:auto，内容撑长后按钮被裁进滚动区 —— 主人视角「按钮不存在」，导入走不完（P0）。
// 无 DOM 测试环境，用**源码结构断言**钉（与 d85-ui-guard / d81-ui-guard 同族）。

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob('../components/*.svelte', {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

function pickSource(suffix: string): string {
  const key = Object.keys(modules).find((k) => k.endsWith(suffix));
  return key === undefined ? '' : modules[key];
}

describe('D-86 需求 1：Modal 归属 ImportWizard，确认导入常驻 footer', () => {
  const src = pickSource('ImportWizard.svelte');

  it('Modal 在 ImportWizard 内部，footer 按 preview 条件传入（未预览 = 整个不渲染）', () => {
    expect(src).toContain('<Modal');
    expect(src).toContain('footer={preview !== null ? applyFooter : undefined}');
  });

  it('确认导入按钮在 footer snippet 内：is-primary + disabled={busy} + void apply()', () => {
    const snippetAt = src.indexOf('{#snippet applyFooter()}');
    expect(snippetAt, '应有 applyFooter snippet').toBeGreaterThan(-1);
    const snippetEnd = src.indexOf('{/snippet}', snippetAt);
    const snippet = src.slice(snippetAt, snippetEnd);
    expect(snippet).toContain('pms-btn is-primary');
    expect(snippet).toContain('disabled={busy}');
    expect(snippet).toContain('void apply()');
    expect(snippet).toContain("ui.t('import.apply')");
  });

  it('Settings 调用点最小改法：只传 open/onclose，不再用 Modal 包 ImportWizard', () => {
    const settings = pickSource('Settings.svelte');
    expect(settings).toContain('<ImportWizard open={importOpen} onclose={() => (importOpen = false)} />');
    expect(settings, 'Settings 不应再用 Modal 包导入向导').not.toContain('<Modal open={importOpen}');
  });
});

describe('D-86 需求 2：选完文件自动预览', () => {
  const src = pickSource('ImportWizard.svelte');

  it('choose() 在取消早退之后自动调 doPreview()（取消选择不会误触发）', () => {
    const fnAt = src.indexOf('async function choose(): Promise<void>');
    expect(fnAt).toBeGreaterThan(-1);
    const cancelGuardAt = src.indexOf("if (typeof picked !== 'string')", fnAt);
    expect(cancelGuardAt, '取消选择必须先早退').toBeGreaterThan(fnAt);
    const autoAt = src.indexOf('await doPreview();', cancelGuardAt);
    expect(autoAt, '选完文件必须自动预览').toBeGreaterThan(cancelGuardAt);
  });

  it('格式手动变更只清 preview、不自动预览；手动「预览」按钮保留', () => {
    expect(src).toContain('onchange={() => (preview = null)}');
    // doPreview 的调用点只有两处：choose 的自动预览 + 预览按钮的 onclick。
    const autoCalls = src.split('await doPreview();').length - 1;
    const buttonCalls = src.split('void doPreview()').length - 1;
    expect(autoCalls).toBe(1);
    expect(buttonCalls, '手动预览按钮必须保留').toBe(1);
  });
});

describe('D-87：编码容错提示（后端 GBK/UTF-16 自动转换 + 用户引导）', () => {
  const src = pickSource('ImportWizard.svelte');

  it('编码提示常驻渲染（pms-meta，与支持范围说明同级）', () => {
    const hintAt = src.indexOf("{ui.t('import.encoding_hint')}");
    expect(hintAt, 'ImportWizard 必须渲染 encoding_hint').toBeGreaterThan(-1);
    // 提示行必须是 pms-meta（中性说明），且在下载模板按钮之前（先说明再操作）。
    const lineStart = src.lastIndexOf('<p', hintAt);
    expect(src.slice(lineStart, hintAt)).toContain('pms-meta');
    const btnAt = src.indexOf('import.download_template');
    expect(btnAt, '下载模板按钮必须存在').toBeGreaterThan(-1);
    expect(hintAt, '编码提示应出现在下载模板按钮之前').toBeLessThan(btnAt);
  });

  it('两份字典都有 encoding_hint 且错误文案不再只说「解析失败」', async () => {
    const { dictionaries } = await import('./i18n');
    for (const dict of Object.values(dictionaries)) {
      const imp = (dict as Record<string, unknown>).import as Record<string, string>;
      expect(imp.encoding_hint, 'encoding_hint 必须双语齐全').toBeTruthy();
      expect(imp.encoding_hint).toMatch(/UTF-8/);
    }
    const zh = (await import('./i18n/zh-CN')).zhCN;
    expect((zh.err as Record<string, string>).import_parse).toContain('Bitwarden');
  });
});
