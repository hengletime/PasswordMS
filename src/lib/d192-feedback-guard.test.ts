// D-192：成功提示不许借用按钮标签。
//
// 缺陷：`common.ok` 在四本字典里的值就是**按钮标签**「确定」/「OK」/「ตกลง」。
// 把它当成功 toast 的文案，用户在屏幕上看到的只有一句「确定」——不说发生了什么，
// 也不说结果是好是坏。D-114 修过改密那一处（`settings.password_changed`），
// 但同一族还有四处没修，D-192 一并补齐：
//
//   1. `attachment-import.ts`        添加附件（用户最常走的那条）
//   2. `AttachmentView.svelte`       导出单个附件
//   3. `AttachmentView.svelte`       移除附件
//   4. `vault-export.ts`             导出保险库的兜底分支（后端不回报字节数时）
//
// 这里守的是**不变量**而不是这四行代码：源码里（字典与测试除外）不得再出现把
// `common.ok` 当文案用的字面量。第 5 条之所以做成全仓扫描，是因为这个错误是**被复制**
// 出来的——附件模块当初就是照着导出模块抄的 `common.ok`。只钉四个点，下一个新模块
// 照抄时照样绿。
//
// ⚠️ 扫描用的是「带引号的字面量」而不是裸子串：注释里提到这个键名不算违规
// （本文件的注释里就写了它十几次），否则守门会把人逼去删注释。

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob(['../**/*.ts', '../**/*.svelte'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

/** 字典（`common.ok` 的定义处，天然要出现这个键）与测试文件（本文件自己）不算违规。 */
function isScannable(path: string): boolean {
  if (path.includes('/i18n/')) {
    return false;
  }
  return !path.endsWith('.test.ts');
}

/** 带引号的字面量用法：`'common.ok'` / `"common.ok"`。反引号与裸键名不在内。 */
const OK_LITERAL = /['"]common\.ok['"]/;

/**
 * 成功 toast 的字面量键：`toast('success', 'attach.added')`。
 *
 * 这里刻意只收**字面量**：常量（`ATTACHMENT_OK_KEY` / `EXPORT_OK_KEY`）在定义处就是
 * 字面量，已被上一条覆盖，重复收会把同一个问题报两遍。
 */
const SUCCESS_TOAST_KEY = /toast\(\s*['"]success['"]\s*,\s*(['"])([^'"]+)\1/g;

/**
 * 按钮词键名单：这些键的值是按钮上的字（「确定」「取消」…），当结果文案用等于没说。
 *
 * 不按 `common.*` 整个命名空间禁——`common.copied`（「已复制」）是**结果句**，
 * 按命名空间一刀切会把合法的用法也判红。只点名这几个。
 */
const BUTTON_KEYS = new Set([
  'common.ok',
  'common.cancel',
  'common.confirm',
  'common.close',
  'common.save',
  'common.delete',
  'common.back',
  'common.yes',
  'common.no',
]);

const scannable = Object.keys(modules).filter(isScannable);
const zhCN = modules['./i18n/zh-CN.ts'] ?? '';

/** 取字典里某个键的字符串值（字典是 `key: 'value',` 的平铺结构，够用且不引解析器）。 */
function dictValue(src: string, key: string): string | null {
  const m = new RegExp(`\\n\\s*${key}:\\s*'((?:[^'\\\\]|\\\\.)*)'`).exec(src);
  return m ? m[1] : null;
}

/** 带命名空间的键（`attach.added`）是否真的存在于某本字典里。 */
function dictHasKey(src: string, dotted: string): boolean {
  const dot = dotted.indexOf('.');
  if (dot < 0) {
    return false;
  }
  const ns = dotted.slice(0, dot);
  const key = dotted.slice(dot + 1);
  const block = new RegExp(`\\n {2}${ns}: \\{([\\s\\S]*?)\\n {2}\\},`).exec(src);
  if (block === null) {
    return false;
  }
  return new RegExp(`\\n {4}${key}:`).test(block[1]);
}

describe('D-192 成功提示不得复用 `common.ok`（那是按钮上的「确定」）', () => {
  it('扫描面本身不能是空的（否则下面几条都是空跑）', () => {
    // 这条是"守门本身的守门"：glob 静默返回空集时，`every` 恒为 true。
    expect(scannable.length).toBeGreaterThan(50);
    expect(scannable.some((p) => p.endsWith('attachment-import.ts'))).toBe(true);
    expect(scannable.some((p) => p.endsWith('AttachmentView.svelte'))).toBe(true);
    expect(scannable.some((p) => p.endsWith('vault-export.ts'))).toBe(true);
  });

  it('四本字典里 `common.ok` 确实就是按钮标签「确定」（根因成立才能这么判）', () => {
    for (const lang of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
      const value = dictValue(modules[`./i18n/${lang}.ts`] ?? '', 'ok');
      expect(value, `${lang} 没有 ok 键`).not.toBeNull();
      expect(value, `${lang} 的 ok 不再是按钮标签`).toMatch(/确定|確定|^OK$|ตกลง/);
    }
  });

  it('源码里没有任何地方把 `common.ok` 当文案字面量用', () => {
    const offenders = scannable.filter((path) => OK_LITERAL.test(modules[path] ?? ''));
    expect(offenders, `这些文件仍把 common.ok 当文案：${offenders.join(', ')}`).toEqual([]);
  });

  it('所有成功 toast 的字面量键都不是按钮词，而且真的存在于字典里', () => {
    // 这一条比上一条宽：它管的是"形状"——任何新写的 `toast('success', '某键')` 都会被看，
    // 不只 `common.ok`。两个失败面各自有名字：
    //   · 命中按钮词 → 又一次「结果是一条按钮」；
    //   · 查不到这个键 → 界面会原样印出键名（D-185 同族，`translate()` 三级回落兜底）。
    const found: Array<{ file: string; key: string }> = [];
    for (const path of scannable) {
      const src = modules[path] ?? '';
      for (const m of src.matchAll(SUCCESS_TOAST_KEY)) {
        found.push({ file: path, key: m[2] });
      }
    }
    // 扫描面空转保护：全套源码里成功 toast 至少两位数，否则正则写错了。
    expect(found.length, '一条成功 toast 都没扫到，正则或扫描面有问题').toBeGreaterThan(5);

    const asButton = found.filter((f) => BUTTON_KEYS.has(f.key));
    expect(asButton, `成功 toast 借用了按钮词：${asButton.map((f) => `${f.file}→${f.key}`).join(', ')}`).toEqual(
      [],
    );

    const unknown = found.filter((f) => !dictHasKey(zhCN, f.key));
    expect(unknown, `成功 toast 用了字典里没有的键：${unknown.map((f) => `${f.file}→${f.key}`).join(', ')}`).toEqual(
      [],
    );
  });

  it('四个新文案在四本字典里都存在、够长、且没有退化成按钮词', () => {
    // 键名在整份字典里唯一（`added` / `exported` / `removed` / `export_done`），
    // 所以按名字取即可 —— 不引解析器，也不依赖命名空间顺序。
    const keys = ['added', 'exported', 'removed', 'export_done'];
    for (const lang of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
      const src = modules[`./i18n/${lang}.ts`] ?? '';
      for (const key of keys) {
        const value = dictValue(src, key);
        expect(value, `${lang} 缺 ${key}`).not.toBeNull();
        const text = (value ?? '').trim();
        // 至少要能说出"发生了什么"：按钮词「确定」只有 2 个字符，这里挡的就是它。
        expect(text.length, `${lang}.${key} 太短，说不清发生了什么`).toBeGreaterThan(4);
        expect(text, `${lang}.${key} 退化成按钮词`).not.toMatch(/^(确定|確定|OK|ตกลง)$/);
      }
    }
  });
});

describe('D-193 孤儿键 `create.weak_hint` 已删（字典里有字、界面上没有位置）', () => {
  it('四本字典都不再有 `weak_hint`', () => {
    // 这条键从 v2.31b 起被反复润色（英式拼写、passphrase 用词都改过），却**从来没有
    // 任何组件渲染它**：创建页的弱口令提示一直是强度条 + `create.hint`，不拦人。
    // 它确凿造成过一次伤害——走查单 1.2 的判据把它写成了验收项，走查的人永远看不到。
    // 真要加「弱口令二次确认」，请连界面一起加，并把这个断言改成"必须有渲染点"。
    for (const lang of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
      const src = modules[`./i18n/${lang}.ts`] ?? '';
      expect(src, `${lang} 又出现了无人渲染的 weak_hint`).not.toContain('weak_hint:');
    }
  });
});
