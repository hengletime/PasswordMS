// 自动输入（1.5.39）的结构化守门。
//
// 这一轮改动有四条「写错了也能编译、能跑，但会在真机上以最难看的方式失败」的纪律，
// 单测夹具覆盖不到（要在真机按热键），所以按本仓惯例（d192 / bio-android-guard 同族）
// 做源码级守门：
//
// 1. **四本字典的 autotype.* 与 settings.autotype_hotkey 键集合一致**，且值不是
//    按钮词（按钮词集合从 `common` 命名空间派生，判据照 D-192：值 ≤2 字符的键）。
// 2. **浮层里没有明文**（M3 的教训）：`auto_type_fill` 的入参与返回不含明文；
//    浮层列表由 `autotype_candidates` 提供，只含标题、用户名、网址与使用次数。
//    浮层源码里一旦出现拉全量条目的调用（那个接口带 password）或取详情，这条先红
//    ——早先这里只禁了取详情，于是"明文不经过前端"是一句假话。
// 3. **失败文案必须显式映射**（D-185 铁律）：后端错误码 → 文案键的映射表是字面量，
//    每个映射目标在四本字典里真实存在；禁止模板现拼键名
//    （`autotype.failed_${reason}` 拼出来的键翻译器找不到，界面会印裸键名）。
// 4. **平台门控**（D-121/D-122 同一口径）：自动输入热键的设置控件只在 Windows 分支
//    渲染 `<button>`；Android 分支只渲染纯文案 `<p>`——挂一个录不上的按钮等于给
//    不存在的功能做广告。
// 5. **后端 message_key 面（S1）**：Rust 里所有 `"err.` 开头的字面量都必须在字典里
//    有键——缺一个就是界面印裸键名。
//
// 读源码用 `import.meta.glob('?raw')`：本仓禁止 `node:fs`（svelte-check 不认 node types）。

import { describe, expect, it } from 'vitest';

import { hasKey, LOCALES } from './i18n/index';
// S10：正反两处扫描共用同一个抽取函数（与 ipc-contract.test.ts 是同一把尺子）。
import { extractMatches } from './source-scan';

const modules = import.meta.glob(
  ['../**/*.ts', '../**/*.svelte'],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

/** Rust 全树（S1：后端 message_key 字面量面的守门；`**` 递归，防只扫顶层得到假绿）。 */
const rustModules = import.meta.glob('../../src-tauri/**/*.rs', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const picker = modules['../components/AutotypePicker.svelte'] ?? '';
const autotypeTs = modules['./autotype.ts'] ?? '';
const ipcTs = modules['./ipc.ts'] ?? '';
const settings = modules['../components/Settings.svelte'] ?? '';

const dicts: Array<[string, string]> = [
  ['zh-CN', modules['./i18n/zh-CN.ts'] ?? ''],
  ['zh-TW', modules['./i18n/zh-TW.ts'] ?? ''],
  ['en-US', modules['./i18n/en-US.ts'] ?? ''],
  ['th', modules['./i18n/th.ts'] ?? ''],
];

/** 守门本身的守门：glob 写错会静默返回空集，后面所有 `every` 恒真。 */
describe('扫描面非空（防空跑）', () => {
  it('四个关键源码文件都被读到了', () => {
    expect(picker.length).toBeGreaterThan(1000);
    expect(autotypeTs.length).toBeGreaterThan(500);
    expect(ipcTs.length).toBeGreaterThan(5000);
    expect(settings.length).toBeGreaterThan(10000);
  });

  it('四本字典都被读到了', () => {
    for (const [name, src] of dicts) {
      expect(src.length, `${name} 字典没读到`).toBeGreaterThan(10000);
    }
  });
});

// ── ① 字典键集合一致、值不是按钮词 ────────────────────────────────────────────

/** 自动输入命名空间的全部键（zh-CN 是唯一事实来源，这份清单与它逐字同步）。 */
const AUTOTYPE_KEYS = [
  'search_placeholder',
  'empty',
  'failed',
  'failed_focus',
  'failed_not_unlocked',
  'failed_not_supported',
  'failed_blocked',
  'no_username',
  'no_url',
  'no_totp',
  'done',
  'foot_hint',
];

/** 设置页改键区块的键。录制/未设置/已更新提示复用现有 `settings.hotkey_*`。 */
const AUTOTYPE_SETTINGS_KEYS = ['autotype_hotkey', 'autotype_hotkey_hint'];

/** 取某个命名空间块里的 `key: 'value'` 对（字典是平铺结构，够用且不引解析器）。 */
function dictBlock(src: string, ns: string): string {
  const m = new RegExp(`\\n {2}${ns}: \\{([\\s\\S]*?)\\n {2}\\},`).exec(src);
  return m ? m[1] : '';
}

/** 去掉注释后的代码（照 bio-android-guard 的 `codeOnly`）：断言查代码不查注释。 */
function codeOnly(src: string): string {
  return src
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .split('\n')
    .filter((l) => !l.trim().startsWith('//'))
    .join('\n');
}

function dictPairs(block: string): Array<[string, string]> {
  return [...block.matchAll(/\n {4}([a-z0-9_]+):\s*'((?:[^'\\]|\\.)*)'/g)].map((m) => [
    m[1],
    m[2],
  ]);
}

function dictValueOf(src: string, dotted: string): string | null {
  const dot = dotted.indexOf('.');
  if (dot < 0) {
    return null;
  }
  const pairs = dictPairs(dictBlock(src, dotted.slice(0, dot)));
  const key = dotted.slice(dot + 1);
  const hit = pairs.find(([k]) => k === key);
  return hit ? hit[1] : null;
}

/**
 * 按钮词集合**从字典派生**，不手写名单（判据照 D-192：`common` 下值 ≤2 字符的键——
 * 中文两字以内基本只可能是按钮/状态标签）。
 */
const commonPairs = dictPairs(dictBlock(dicts[0][1], 'common'));
const BUTTON_WORDS = new Set(commonPairs.filter(([, v]) => v.length <= 2).map(([, v]) => v));

describe('① autotype 键集合四语言一致，值非按钮词、够长', () => {
  it('按钮词集合派生非空（否则下面的判据是空跑）', () => {
    expect(BUTTON_WORDS.size).toBeGreaterThan(10);
  });

  it('每本字典的 autotype.* 键集合与 zh-CN 完全一致', () => {
    for (const [name, src] of dicts) {
      const keys = dictPairs(dictBlock(src, 'autotype')).map(([k]) => k);
      expect(keys, `${name} 的 autotype 键集合`).toEqual(AUTOTYPE_KEYS);
    }
  });

  it('每本字典都有设置页改键区块的键，且都能被 translate 找到', () => {
    for (const [name, src] of dicts) {
      for (const key of AUTOTYPE_SETTINGS_KEYS) {
        expect(
          dictValueOf(src, `settings.${key}`),
          `${name} 缺 settings.${key}`,
        ).not.toBeNull();
      }
    }
    // 走真正的翻译器再验一遍：hasKey 是运行时真正用的查找路径。
    for (const locale of LOCALES) {
      for (const key of AUTOTYPE_KEYS) {
        expect(hasKey(locale, `autotype.${key}`), `${locale}:autotype.${key}`).toBe(true);
      }
      for (const key of AUTOTYPE_SETTINGS_KEYS) {
        expect(hasKey(locale, `settings.${key}`), `${locale}:settings.${key}`).toBe(true);
      }
    }
  });

  it('所有新值都 >4 字符（说得清发生了什么），且不是按钮词', () => {
    for (const [name, src] of dicts) {
      const pairs = [
        ...dictPairs(dictBlock(src, 'autotype')).map(([k, v]) => [`autotype.${k}`, v] as const),
        ...AUTOTYPE_SETTINGS_KEYS.map(
          (k) => [`settings.${k}`, dictValueOf(src, `settings.${k}`) ?? ''] as const,
        ),
      ];
      for (const [key, value] of pairs) {
        const text = value.trim();
        expect(text.length, `${name}:${key} 太短，说不清发生了什么`).toBeGreaterThan(4);
        expect(
          BUTTON_WORDS.has(text),
          `${name}:${key} 的文案「${text}」与按钮词同形`,
        ).toBe(false);
      }
    }
  });

  it('foot_hint 一句话说清五种动作（每语言各查代表性 token）', () => {
    // 键值必须是**整句**（D-185：不许把字典片段拼起来），所以这里按语言查整句里
    // 该出现的动作词——少一种动作就是用户按了不知道会发生什么。
    const tokens: Record<string, string[]> = {
      'zh-CN': ['网址', '动态口令', 'Shift', 'Ctrl+Shift', 'Esc'],
      'zh-TW': ['網址', '動態密碼', 'Shift', 'Ctrl+Shift', 'Esc'],
      'en-US': ['URL', 'one-time', 'Shift', 'Ctrl+Shift', 'Esc'],
      th: ['URL', 'รหัส OTP', 'Shift', 'Ctrl+Shift', 'Esc'],
    };
    for (const [name, src] of dicts) {
      const text = dictValueOf(src, 'autotype.foot_hint') ?? '';
      const expected = tokens[name] ?? [];
      expect(expected.length, `${name} 没登记 foot_hint 的 token`).toBeGreaterThan(0);
      for (const token of expected) {
        expect(text, `${name} 的 foot_hint 缺「${token}」`).toContain(token);
      }
      expect(text.length, `${name} 的 foot_hint 太短，说不清五种动作`).toBeGreaterThan(20);
    }
  });
});

// ── ② 浮层里没有明文（M3） ───────────────────────────────────────────────────

describe('② 浮层走专用候选，明文不经过浮层', () => {
  it('浮层必须调 fill 与 candidates，且不得出现拉全量条目 / 取详情的调用', () => {
    // 判据（安全官更正）：**断言的正确性不得依赖扫描面为空**。
    // 期望值本来就非零 → 存在性断言；本来就为零 → 换成语义正确的面 + 否定性断言。
    // 浮层走 `api.*` 三层封装，`invoke(` 的正确出现次数就是 0——给那半条加
    // 「匹配数 > 0」只会逼实现写一句裸 `invoke(` 来喂测试（用测试反向定制实现，更坏），
    // 所以那半条整段删掉，改断言下面四个正确的面。
    expect(picker, '浮层必须调用 api.autotype.fill').toContain('api.autotype.fill(');
    expect(picker, '浮层必须调用 api.autotype.candidates').toContain(
      'api.autotype.candidates(',
    );
    // M3 的根因：全量条目列表带 password（item_cmd.rs 直传 Vec<Item>），
    // 整个库的明文会躺在浮层 JS 堆里。两处入口都禁——只禁取详情曾让这句断言永远是绿的。
    expect(picker, '浮层不得调用 api.items.list（带 password 的全量列表）').not.toContain(
      'api.items.list(',
    );
    expect(picker, '浮层不得调用 api.items.get（明文详情接口）').not.toContain(
      'api.items.get(',
    );
  });

  it('ipc.ts 的 autotype 封装：候选只回五字段，fill 只传 item_id 与动作名', () => {
    // 自动输入树里不许出现取明文的形状；wire 键就是 item_id + action。
    // 查**代码**不查注释：注释里提 password 是为了说明为什么禁它，属正常。
    const start = ipcTs.indexOf('export const autotype');
    const end = ipcTs.indexOf('};', start);
    const block = codeOnly(ipcTs.slice(start, end));
    expect(block.length).toBeGreaterThan(0);
    expect(block).toContain("invoke<AutotypeCandidate[]>('autotype_candidates')");
    expect(block).toContain("invoke<void>('auto_type_fill', { item_id: itemId, action })");
    expect(block).not.toContain('password');
  });

  it('五种动作齐全，且 Ctrl+Shift 的判定在单键分支之前', () => {
    // 契约：default / username / password / url / totp 五种取值逐字一致。
    for (const action of ['default', 'username', 'password', 'url', 'totp']) {
      expect(picker, `浮层缺少动作 ${action}`).toContain(`'${action}'`);
    }
    // Ctrl+Shift+Enter 是 Ctrl 与 Shift 的叠加：判定顺序反了会落进「仅密码」。
    const combo = picker.indexOf('event.ctrlKey && event.shiftKey');
    const ctrlOnly = picker.indexOf("? 'password'");
    expect(combo, '缺 Ctrl+Shift 组合判定').toBeGreaterThan(-1);
    expect(ctrlOnly, '缺仅 Ctrl 分支').toBeGreaterThan(-1);
    expect(combo, 'Ctrl+Shift 必须先判').toBeLessThan(ctrlOnly);
  });
});

// ── ③ 失败文案显式映射（D-185） ───────────────────────────────────────────────

describe('③ 失败文案是显式映射表，不许模板现拼键名', () => {
  it('映射表字面量在 autotype.ts 里，每个目标键在四本字典都存在', () => {
    const at = autotypeTs.indexOf('AUTOTYPE_FAILURE_KEY_BY_CODE');
    expect(at, '缺映射表 AUTOTYPE_FAILURE_KEY_BY_CODE').toBeGreaterThan(-1);
    const block = autotypeTs.slice(at, autotypeTs.indexOf('};', at));
    // 显式条目逐条钉住：映射表少一条、或键名拼错，这里就红。
    const expected: Array<[string, string]> = [
      ['ERR_AUTOTYPE_FOCUS_LOST', 'autotype.failed_focus'],
      ['ERR_NOT_UNLOCKED', 'autotype.failed_not_unlocked'],
      ['ERR_NOT_SUPPORTED', 'autotype.failed_not_supported'],
      ['ERR_AUTOTYPE_NO_USERNAME', 'autotype.no_username'],
      ['ERR_AUTOTYPE_NO_URL', 'autotype.no_url'],
      ['ERR_AUTOTYPE_NO_TOTP', 'autotype.no_totp'],
      ['ERR_AUTOTYPE_BLOCKED', 'autotype.failed_blocked'],
    ];
    for (const [code, key] of expected) {
      expect(block, `映射表缺 ${code} → ${key}`).toContain(`${code}: '${key}'`);
    }
    // 兜底键本身也要真实存在：映射不到时界面显示的是它，不是裸键名。
    expect(autotypeTs).toContain("AUTOTYPE_FAILURE_FALLBACK_KEY = 'autotype.failed'");
    for (const locale of LOCALES) {
      for (const [, key] of expected) {
        expect(hasKey(locale, key), `${locale}:${key}`).toBe(true);
      }
      expect(hasKey(locale, 'autotype.failed'), `${locale}:autotype.failed`).toBe(true);
    }
  });

  it('源码里没有任何 autotype 键是模板现拼的', () => {
    // D-185 的事故形态：`autotype.failed_${reason}` 拼出字典里不存在的键，
    // 界面原样印键名。模板字符串一出现即红——新原因必须进显式映射表。
    const offenders = Object.entries(modules)
      .filter(([p]) => p.endsWith('.ts') || p.endsWith('.svelte'))
      .filter(([p]) => !p.endsWith('.test.ts') && !p.includes('/i18n/'))
      .filter(([, src]) => /autotype\.[a-z_]*\$\{/.test(src))
      .map(([p]) => p);
    expect(offenders, `这些文件在现拼 autotype 键名：${offenders.join(', ')}`).toEqual([]);
  });
});

// ── ④ 平台门控（D-121/D-122 同一口径） ────────────────────────────────────────

describe('④ 自动输入热键设置的平台门控', () => {
  it('判据独立成派生态，平台来源是后端 DTO（禁止 navigator.userAgent）', () => {
    const at = settings.indexOf('const autotypeHotkeyEditable');
    expect(at, '缺少 autotypeHotkeyEditable').toBeGreaterThan(-1);
    const line = settings.slice(at, settings.indexOf('\n', at));
    expect(line).toContain("bioPlatform === 'windows'");

    const at2 = settings.indexOf('const autotypeHotkeyTextOnly');
    expect(at2, '缺少 autotypeHotkeyTextOnly').toBeGreaterThan(-1);
    const line2 = settings.slice(at2, settings.indexOf('\n', at2));
    expect(line2).toContain("bioPlatform === 'android'");

    // bioPlatform 由 `bioPlatformOf(bio)` 派生（后端 DTO 的 platform 字段）。
    // 查**代码**不查注释：注释里提 UA 是为了说明为什么禁它，属正常。
    const settingsCode = codeOnly(settings);
    expect(settingsCode).toContain('bioPlatformOf(bio)');
    expect(settingsCode, '平台判定不得退回 UA 嗅探').not.toContain('navigator.userAgent');
  });

  it('Windows 分支渲染改键按钮，Android 分支只有纯文案', () => {
    const iCond = settings.indexOf('{#if autotypeHotkeyEditable}');
    expect(iCond, '缺少 {#if autotypeHotkeyEditable} 块').toBeGreaterThan(-1);
    const iElse = settings.indexOf('{:else if autotypeHotkeyTextOnly}', iCond);
    expect(iElse, '缺少 Android 分支').toBeGreaterThan(iCond);
    const iEnd = settings.indexOf('{/if}', iElse);
    const winBranch = settings.slice(iCond, iElse);
    const androidBranch = settings.slice(iElse, iEnd);
    expect(winBranch, 'Windows 分支必须给改键按钮').toContain('<button');
    expect(winBranch).toContain('settings.autotype_hotkey');
    expect(androidBranch, 'Android 分支必须给文案').toContain('autotype_hotkey_hint');
    expect(androidBranch, 'Android 分支不许有按钮——录不上的按钮是假功能').not.toContain(
      '<button',
    );
  });
});

// ── ⑤ 后端 message_key 字面量在字典里都有键（S1） ─────────────────────────────

describe('⑤ Rust 里所有 "err. 字面量都在字典里有键（S1）', () => {
  it('每个 err.* message_key 都能被 zh-CN 字典翻译', () => {
    // S1 的事故形态：后端抛 `{ code, message_key: "err.autotype_no_username" }`，
    // 字典里没有这个键 → 通用错误层原样印出裸键名。逐个字面量比对，缺一个即红。
    expect(Object.keys(rustModules).length, 'Rust 源码没扫到，glob 写错了').toBeGreaterThan(10);
    // 抽取函数与 ipc-contract 共用（source-scan.ts），两处不各写一份正则。
    const literals = extractMatches(
      Object.values(rustModules).join('\n'),
      /"(err\.[a-z0-9_]+)"/g,
    );
    // 防空跑：扫到的字面量太少说明正则或扫描面坏了（而不是"全都对上了"）。
    expect(literals.length, '扫到的 err.* 字面量太少，扫描面坏了').toBeGreaterThan(3);
    const missing = literals.filter((key) => !hasKey('zh-CN', key));
    expect(missing, `这些后端 message_key 在字典里没有键：${missing.join(', ')}`).toEqual([]);
  });
});
