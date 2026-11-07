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
 * 成功 toast 的**实参切片**：从 `toast('success',` 取到右括号（允许跨行）。
 *
 * 取切片而不是取固定形态，是因为形态会长出来：字面量、常量、三元、跨行参数……
 * 每多一种，预知形态的正则就漏一条，而且**漏得无声无息**（少扫一条仍然全绿）。
 * 切片 + "掏不出键就红" 把这种事变成可见的失败。
 */
const SUCCESS_TOAST_ARG = /toast\(\s*['"]success['"]\s*,([^)]*)/g;

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
    // 口径：**凡是 `toast('success',` 就必须掏出至少一个 key**，掏不出即红。
    //
    // 为什么这么写而不预知形态：第三轮评审发现三元
    // `toast('success', cond ? 'a' : 'b')` 不在任何白名单里，整条路静默漏扫。
    // 现在改成"取实参那一段切片，再从切片里掏所有字符串字面量与全大写标识符"——
    // 形态未知也能收，且**掏不出来就报错**，不会再无声无息少扫一条。
    const consts = new Map<string, string>();
    for (const path of scannable) {
      for (const m of (modules[path] ?? '').matchAll(CONST_DEF)) {
        consts.set(m[1], m[2]);
      }
    }

    const found: Array<{ file: string; key: string }> = [];
    const unresolved: string[] = [];
    let calls = 0;
    for (const path of scannable) {
      const src = modules[path] ?? '';
      for (const m of src.matchAll(SUCCESS_TOAST_ARG)) {
        calls += 1;
        const slice = m[1];
        const keys: string[] = [];
        // 字面量键（`'attach.added'`）；三元里的两个分支也会各自被收进来。
        for (const lit of slice.matchAll(/'([^']+)'/g)) {
          if (/^[a-z][a-z0-9_]*\.[a-z0-9_]+$/.test(lit[1])) {
            keys.push(lit[1]);
          }
        }
        // 全大写常量（值从定义处取）。
        for (const id of slice.matchAll(/\b([A-Z][A-Z0-9_]+)\b/g)) {
          const key = consts.get(id[1]);
          if (key !== undefined) {
            keys.push(key);
          }
        }
        if (keys.length === 0) {
          unresolved.push(`${path}→${slice.trim().slice(0, 40)}`);
        }
        for (const key of keys) {
          found.push({ file: path, key });
        }
      }
    }
    // 掏不出 key 的调用点 = 口径断了，先报这个，别让人去猜后面的断言。
    expect(unresolved, `这些成功 toast 掏不出文案键：\n${unresolved.join('\n')}`).toEqual([]);
    // 下界取实测值：少一条就红，逼人回来看一眼是不是漏扫了（而不是默默少扫）。
    // 删掉一条成功提示时把这个数一起改小——红一次比永远绿好。
    expect(calls, '扫到的成功 toast 调用点少于实测值，正则或扫描面出问题了').toBeGreaterThanOrEqual(27);
    expect(found.length, '掏出的键少于调用点，说明有调用点没被解析').toBeGreaterThanOrEqual(calls);

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

describe('D-194 孤儿键 `err.unknown`：已由反扫报出并删除，不许悄悄长回来', () => {
  // 它和 D-193 是同一类（字典里有字、界面上没有位置）。当初**刻意留着**：那会儿还没有
  // "字典 → 代码"反向扫描，它是唯一一个已知阳性样本——反扫报 0 条时得先能报出它，
  // 那个 0 才算数。
  //
  // 2026-11-07 结案：反向扫描落地在 `i18n-orphans.test.ts`，首跑 31 条孤儿里报出了
  // 这一条（这就是"先能报出阳性"的现场），随后按登记的四步走：四本字典同删、基线里
  // 不留它的行、生产代码仍不许引用。
  //
  // "反扫不是瞎的"这层保障**换了载体**：现在由 `i18n-orphans.test.ts` 的探针承担
  // （伪造键必须被判成孤儿）。所以这里不再需要拿一个真键当样本。

  it('四本字典里都没有了', () => {
    for (const lang of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
      expect(
        dictValueOf(modules[`./i18n/${lang}.ts`] ?? '', 'err.unknown'),
        `${lang} 的 err.unknown 又出现了——它是 D-194 已删的孤儿键，别再加回来`,
      ).toBeNull();
    }
  });

  it('仍然没有任何生产点（删了字典又写回代码里的字面量同样要红）', () => {
    const users = scannable.filter((path) => /err\.unknown/.test(modules[path] ?? ''));
    expect(users, `err.unknown 被这些地方引用了：${users.join(', ')}`).toEqual([]);
  });

  it('反扫的"不瞎"由探针接着担（这个文件必须还在，且探针断言没被删）', () => {
    const scan = modules['./i18n-orphans.test.ts'] ?? '';
    expect(scan.length, '反向扫描用例不见了——err.unknown 当初就是靠它才敢删').toBeGreaterThan(2000);
    expect(scan, '反扫的伪造键探针被删了，等于把它变成只会报 0 的瞎扫描器').toContain(
      'zz.probe.not.a.real.key',
    );
  });
});
