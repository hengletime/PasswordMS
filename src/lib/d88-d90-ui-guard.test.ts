// D-88 / D-89 / D-90 UI 守卫：登录备注编辑框 + 登录转安全笔记 + 导入合并结果上报。
//
// 无 DOM 测试环境，用源码结构断言（与 import-wizard-ui / d85-ui-guard 同族）。

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob(['../components/*.svelte', './i18n/*.ts'], {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

function pickSource(suffix: string): string {
  const key = Object.keys(modules).find((k) => k.endsWith(suffix));
  return key === undefined ? '' : modules[key];
}

describe('D-89：登录表单必须有备注编辑框', () => {
  const src = pickSource('ItemEditor.svelte');

  it('登录分支内含 ed-notes textarea，绑定 notes 状态（导入的备注不再「看得见改不了」）', () => {
    const loginAt = src.indexOf("{#if kind === 'login'}");
    expect(loginAt).toBeGreaterThan(-1);
    const elseAt = src.indexOf('{:else}', loginAt);
    const loginBlock = src.slice(loginAt, elseAt);
    const textareaAt = loginBlock.indexOf('id="ed-notes"');
    expect(textareaAt, '登录表单必须有备注 textarea').toBeGreaterThan(-1);
    const openAt = loginBlock.lastIndexOf('<textarea', textareaAt);
    const tagEnd = loginBlock.indexOf('>', textareaAt);
    expect(loginBlock.slice(openAt, tagEnd)).toContain('bind:value={notes}');
  });

  it('备注框在 TOTP 之后、自定义字段之前（拍板的排布）', () => {
    const totpAt = src.indexOf('id="ed-totp"');
    const notesAt = src.indexOf('id="ed-notes"');
    const customAt = src.indexOf('editor-custom');
    expect(notesAt).toBeGreaterThan(totpAt);
    expect(notesAt).toBeLessThan(customAt);
  });
});

describe('D-88：登录项一键转安全笔记（A1 映射不丢数据）', () => {
  const src = pickSource('ItemDetail.svelte');

  it('转换按钮仅在登录项渲染（icon 按钮，位于编辑与删除之间）', () => {
    expect(src).toContain("{#if item.kind.type === 'login'}");
    expect(src).toContain("ui.t('item.convert_note')");
    const editAt = src.indexOf("ui.t('common.edit')");
    // 模板区 onclick 的调用点（函数定义在 script 区，位置在模板之前）。
    const convertAt = src.indexOf('convertToNote()', editAt);
    const trashAt = src.indexOf("ui.t('item.delete')");
    expect(convertAt, '模板区必须有 onclick 调用').toBeGreaterThan(-1);
    expect(convertAt).toBeGreaterThan(editAt);
    expect(convertAt).toBeLessThan(trashAt);
  });

  it('A1 映射：url/username/password/totp/自定义字段全部并入正文，不丢弃', () => {
    const fnAt = src.indexOf('async function convertToNote()');
    const fnEnd = src.indexOf('async function', fnAt + 10) === -1 ? src.length : src.indexOf('</script>');
    const fn = src.slice(fnAt, src.indexOf('</script>'));
    void fnEnd;
    for (const needle of [
      'login.url',
      'login.username',
      'login.password',
      'login.totp_secret',
      'custom_fields',
      'convert_fields_header',
    ]) {
      expect(fn).toContain(needle);
    }
  });

  it('走 update_item（item_id 不变），有二次确认', () => {
    const fn = src.slice(src.indexOf('async function convertToNote()'), src.indexOf('</script>'));
    expect(fn).toContain("ui.confirm('item.convert_note_title', 'item.convert_note_body')");
    expect(fn).toContain('vault.updateItem(item.item_id');
    expect(fn, '不得走新建+删除路径').not.toContain('createItem');
    expect(fn).not.toContain('deleteItem');
  });
});

describe('D-90：导入合并结果如实上报', () => {
  const src = pickSource('ImportWizard.svelte');

  it('结果行追加 duplicate_skipped / merged_folders（0 时不追加）', () => {
    const fn = src.slice(src.indexOf('async function apply()'), src.indexOf('</script>'));
    expect(fn).toContain('result.duplicate_skipped');
    expect(fn).toContain('result.merged_folders');
    expect(fn).toContain("ui.t('import.duplicates'");
    expect(fn).toContain("ui.t('import.merged_folders'");
  });

  it('两份字典键齐全（convert 系列 + 导入合并计数）', async () => {
    const { dictionaries } = await import('./i18n');
    for (const dict of Object.values(dictionaries)) {
      const d = dict as Record<string, Record<string, string>>;
      expect(d.item.convert_note).toBeTruthy();
      expect(d.item.convert_note_title).toBeTruthy();
      expect(d.item.convert_note_body).toBeTruthy();
      expect(d.item.convert_fields_header).toBeTruthy();
      expect(d.item.convert_done).toBeTruthy();
      expect(d.import.duplicates).toBeTruthy();
      expect(d.import.merged_folders).toBeTruthy();
    }
    const zh = (await import('./i18n/zh-CN')).zhCN as unknown as Record<string, Record<string, string>>;
    expect(zh.item.convert_note_body).toContain('并入正文');
    expect(zh.import.duplicates).toContain('{n}');
    expect(zh.import.merged_folders).toContain('{n}');
  });
});
