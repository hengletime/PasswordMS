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
 * 成功 toast 的**字面量**键：`toast('success', 'attach.added')`。
 *
 * 常量形态（`toast('success', ATTACHMENT_OK_KEY)`）另外由 `SUCCESS_TOAST_CONST` 收，
 * 靠"全大写常量 → 它的字面量值"这条映射对上，不再靠"常量定义处是字面量所以自然被覆盖"
 * ——那句话只对 `common.ok` 成立，对其余常量不成立。
 */
const SUCCESS_TOAST_KEY = /toast\(\s*['"]success['"]\s*,\s*(['"])([^'"]+)\1/g;

/** `toast('success', SOME_CONST)` 里的常量名（值在下面的映射表里查）。 */
const SUCCESS_TOAST_CONST = /toast\(\s*['"]success['"]\s*,\s*([A-Z][A-Z0-9_]+)\s*[,)]/g;

/** 全大写常量的定义：`export const X_OK_KEY = 'ns.key';`（只收带命名空间的键值）。 */
const CONST_DEF = /(?:export\s+)?const\s+([A-Z][A-Z0-9_]+)\s*=\s*'([a-z][a-z0-9_]*\.[a-z0-9_]+)'/g;

/**
 * 按钮词集合**从字典派生**，不手写名单。
 *
 * 判据：`common` 命名空间下、值的字符数 ≤ 2 的那些键——中文两字以内基本只可能是
 * 按钮/状态标签（确定、取消、保存、移除…）。阈值卡 2 而不卡 3，是因为
 * `common.copied`「已复制」是**结果句**且正在被合法用作成功提示；卡 3 会把它误判。
 *
 * 手写名单的问题是漏（`common.edit`、`common.retry` 当年就没写进去），派生则跟着字典走。
 */
const zhCN = modules['./i18n/zh-CN.ts'] ?? '';

function dictBlock(src: string, ns: string): string {
  const m = new RegExp(`\\n {2}${ns}: \\{([\\s\\S]*?)\\n {2}\\},`).exec(src);
  return m ? m[1] : '';
}

function dictPairs(block: string): Array<[string, string]> {
  return [...block.matchAll(/\n {4}([a-z0-9_]+):\s*'((?:[^'\\]|\\.)*)'/g)].map((m) => [m[1], m[2]]);
}

const commonPairs = dictPairs(dictBlock(zhCN, 'common'));
const BUTTON_WORDS = new Set(commonPairs.filter(([, v]) => v.length <= 2).map(([, v]) => v));
const BUTTON_KEYS = new Set(
  commonPairs.filter(([, v]) => v.length <= 2).map(([k]) => `common.${k}`),
);

const scannable = Object.keys(modules).filter(isScannable);

/** 取字典里某个键的字符串值（字典是 `key: 'value',` 的平铺结构，够用且不引解析器）。 */
function dictValue(src: string, key: string): string | null {
  const m = new RegExp(`\\n\\s*${key}:\\s*'((?:[^'\\\\]|\\\\.)*)'`).exec(src);
  return m ? m[1] : null;
}

/** 带命名空间的键（`attach.added`）在某本字典里的值。 */
function dictValueOf(src: string, dotted: string): string | null {
  const dot = dotted.indexOf('.');
  if (dot < 0) {
    return null;
  }
  const block = dictBlock(src, dotted.slice(0, dot));
  const key = dotted.slice(dot + 1);
  const m = new RegExp(`\\n {4}${key}:\\s*'((?:[^'\\\\]|\\\\.)*)'`).exec(block);
  return m ? m[1] : null;
}

/** 带命名空间的键（`attach.added`）是否真的存在于某本字典里。 */
function dictHasKey(src: string, dotted: string): boolean {
  return dictValueOf(src, dotted) !== null;
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

  it('所有成功 toast 的键都不是按钮词、值也不等于按钮标签，且键真的存在于字典', () => {
    // 两种写法都收：字面量 `'attach.added'` 与全大写常量 `ATTACHMENT_OK_KEY`
    // （常量的值从它的定义处取）。只收字面量会漏掉常量形态——那条路永远不会红。
    const consts = new Map<string, string>();
    for (const path of scannable) {
      for (const m of (modules[path] ?? '').matchAll(CONST_DEF)) {
        consts.set(m[1], m[2]);
      }
    }

    const found: Array<{ file: string; key: string }> = [];
    const unresolved: string[] = [];
    for (const path of scannable) {
      const src = modules[path] ?? '';
      for (const m of src.matchAll(SUCCESS_TOAST_KEY)) {
        found.push({ file: path, key: m[2] });
      }
      for (const m of src.matchAll(SUCCESS_TOAST_CONST)) {
        const key = consts.get(m[1]);
        if (key === undefined) {
          unresolved.push(`${path}→${m[1]}`);
        } else {
          found.push({ file: path, key });
        }
      }
    }
    // 常量名解析不到 = 扫描口径断了，先报这个，别让人去猜后面的断言。
    expect(unresolved, `成功 toast 用了找不到定义的常量：${unresolved.join(', ')}`).toEqual([]);
    // 下界取实测值（19 条字面量 + 3 条常量 = 22）：少一条就红，逼人回来看一眼是不是漏扫了
    // （而不是默默少扫）。删掉一条成功提示时，把这个数一起改小——红一次比永远绿好。
    expect(found.length, '扫到的成功 toast 少于实测值，正则或扫描面出问题了').toBeGreaterThanOrEqual(22);

    const asButtonKey = found.filter((f) => BUTTON_KEYS.has(f.key));
    expect(
      asButtonKey,
      `成功 toast 直接借用了按钮键：${asButtonKey.map((f) => `${f.file}→${f.key}`).join(', ')}`,
    ).toEqual([]);

    // 值级判据：拦住"换个命名空间写出同一个按钮词"这类规避（`attach.remove` 的值就是「移除」）。
    const asButtonWord = found.filter((f) => {
      const value = dictValueOf(zhCN, f.key);
      return value !== null && BUTTON_WORDS.has(value.trim());
    });
    expect(
      asButtonWord,
      `成功 toast 的文案就是按钮上的字：${asButtonWord.map((f) => `${f.file}→${f.key}`).join(', ')}`,
    ).toEqual([]);

    const unknown = found.filter((f) => !dictHasKey(zhCN, f.key));
    expect(
      unknown,
      `成功 toast 用了字典里没有的键：${unknown.map((f) => `${f.file}→${f.key}`).join(', ')}`,
    ).toEqual([]);
  });

  it('派生出来的按钮词集合本身非空且包含「确定」（否则上面那条是空跑）', () => {
    expect(BUTTON_WORDS.size, '一个按钮词都没派生出来，阈值或字典块解析坏了').toBeGreaterThan(10);
    expect(BUTTON_WORDS.has('确定'), '按钮词集合里没有「确定」').toBe(true);
    // 结果句不许被误判成按钮词：`common.copied`「已复制」正在被合法用作成功提示。
    expect(BUTTON_WORDS.has('已复制'), '把结果句「已复制」当成了按钮词，阈值卡错了').toBe(false);
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
