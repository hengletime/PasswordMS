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

  it('foot_hint 说清四个数字键与 Esc（每语言各查动作词）', () => {
    // 键值必须是**整句**（D-185：不许把字典片段拼起来），所以这里按语言查整句里
    // 该出现的动作词——少一种动作就是用户按了不知道会发生什么。
    // 键位串本身不翻译（Ctrl+1 在各语言里都是 Ctrl+1），逐个断言出现：
    const keyTokens = ['Ctrl+1', 'Ctrl+2', 'Ctrl+3', 'Ctrl+4', 'Esc'];
    // 动作词逐语言登记（1.5.40 起键位与 KeePassXC 对齐：1=用户名 2=密码 3=动态口令 4=网址）。
    const actionWords: Record<string, string[]> = {
      'zh-CN': ['仅用户名', '仅密码', '动态口令', '网址'],
      'zh-TW': ['僅使用者名稱', '僅密碼', '動態密碼', '網址'],
      'en-US': ['username only', 'password only', 'one-time', 'URL'],
      th: ['เฉพาะชื่อผู้ใช้', 'เฉพาะรหัสผ่าน', 'รหัส OTP', 'URL'],
    };
    for (const [name, src] of dicts) {
      const text = dictValueOf(src, 'autotype.foot_hint') ?? '';
      for (const token of keyTokens) {
        expect(text, `${name} 的 foot_hint 缺键位「${token}」`).toContain(token);
      }
      const words = actionWords[name] ?? [];
      expect(words.length, `${name} 没登记 foot_hint 的动作词`).toBeGreaterThan(0);
      for (const word of words) {
        expect(text, `${name} 的 foot_hint 缺「${word}」`).toContain(word);
      }
      // 反向：旧修饰键位已废，别回流（回流 = 提示里写着按不出来的键）。
      expect(text, `${name} 的 foot_hint 还残留旧键位 Shift/Alt`).not.toMatch(/Shift|Alt\b/);
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

  it('五种动作齐全，且键位映射只有一份（在 autotype.ts，不在组件里）', () => {
    // 契约：default / username / password / url / totp 五种取值逐字一致。
    for (const action of ['default', 'username', 'password', 'url', 'totp']) {
      expect(autotypeTs, `autotype.ts 缺少动作 ${action}`).toContain(`'${action}'`);
    }
    const pickerCode = codeOnly(picker);
    // 浮层必须走 actionForKey——这一句在，才说明映射只有一份（能单测的那一份）。
    expect(pickerCode, '浮层必须调用 actionForKey(event)').toContain('actionForKey(event)');
    // 组件里自己判修饰键 = 又出现一份映射。1.5.39 的 Alt+Enter 就是这么坏的：
    // 映射藏在组件的嵌套三元里，既没法单测，真机上"事件到不了"也没人知道，
    // 于是那个键在验收前一直是坏的、而代码看起来完全正确。
    expect(
      pickerCode,
      '浮层不许自己判修饰键（映射只许在 autotype.ts::actionForKey 一处）',
    ).not.toMatch(/event\.(ctrlKey|altKey|shiftKey)/);
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

// ── ⑥ 命令层两条"早退"绊线（安全审查 M4 的机器判据） ─────────────────────────
//
// 为什么不放在 Rust 单测里：`src-tauri` 的 `#[cfg(test)]` **不进任何门禁**
// （gate-all 的 Rust 全量只跑 `-p passwordms-core`），写在那里等于白写。
// `commands/` 也没有测试台，而"这条命令里有没有闸门 / 有没有偷偷早退"本质上是
// **源码形状**问题，不是行为问题——前端门禁顺手就能看住。
//
// 两条判据针对的都是"能编译、能跑、真机上以最难看的形态失败"的写法：
// - 少一道 `session_unlocked()`：锁着库也能列出条目（M4 同族的入口问题）；
// - `get_item(` 与 `wipe_plaintext()` 之间夹一个 `?`：解密出来的整条明文
//   （含密码、备注、TOTP 密钥、附件文件名）在堆上留到自然回收。`?` 是**最隐蔽**
//   的早退形态——`return` 一眼能看出来，`?` 藏在赋值语句末尾。

/** 取某个顶层 `fn` 的函数体（从函数头到与之齐平的收尾 `}`）。 */
function rustFnBody(src: string, head: string): string {
  const at = src.indexOf(head);
  if (at < 0) {
    return '';
  }
  const rest = src.slice(at);
  const close = /^}/m.exec(rest);
  return close ? rest.slice(0, close.index) : rest;
}

/** 把注释剥掉再做"有没有这句代码"的判断（注释里写 `?` 是为了讲解，不算早退）。 */
function rustCodeOnly(src: string): string {
  return src
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .split('\n')
    .filter((l) => !l.trim().startsWith('//'))
    .join('\n');
}

describe('⑥ 命令层的闸门与早退（源码形状）', () => {
  const cmd = rustModules['../../src-tauri/src/commands/autotype_cmd.rs'] ?? '';

  it('命令文件读到了（否则下面三条恒真）', () => {
    expect(cmd.length, 'autotype_cmd.rs 没读到').toBeGreaterThan(10000);
  });

  it('autotype_candidates 函数体内有 session_unlocked 闸门', () => {
    const body = rustCodeOnly(rustFnBody(cmd, 'pub fn autotype_candidates('));
    // 防空跑：函数体取不到时下面那句 not 会恒真。
    expect(body.length, 'autotype_candidates 函数体没取到').toBeGreaterThan(300);
    expect(body, '候选列表没有解锁闸门').toContain('session_unlocked()');
  });

  it('auto_type_fill：取到明文之后到擦除之间不得有任何 `?`', () => {
    const body = rustCodeOnly(rustFnBody(cmd, 'pub async fn auto_type_fill('));
    expect(body.length, 'auto_type_fill 函数体没取到').toBeGreaterThan(2000);
    const get = body.indexOf('get_item(');
    const wipe = body.indexOf('wipe_plaintext()');
    expect(get, 'auto_type_fill 里没有 get_item(').toBeGreaterThan(-1);
    expect(wipe, 'auto_type_fill 里没有 wipe_plaintext() —— 明文没擦').toBeGreaterThan(get);
    // 起点要从**取明文那句语句结束之后**算：`get_item(&item_id)?` 里的 `?` 是取明文
    // 动作本身，它失败时手上还没有明文，谈不上漏擦（判据把它算进去会误报，实测过一次）。
    // 风险从"明文到手"那一刻开始，即那条 `let ... = ...?;` 的分号之后。
    const stmtEnd = body.indexOf(';', get);
    expect(stmtEnd, '取明文那句没取到分号').toBeGreaterThan(get);
    expect(stmtEnd, '语句结束位置落在擦除之后，切片写反了').toBeLessThan(wipe);
    const between = body.slice(stmtEnd, wipe);
    expect(
      between.includes('?'),
      `明文到手后到擦除之间有 ?，存在跳过擦除的早退路径：\n${between}`,
    ).toBe(false);
  });

  it('autotype_candidates：列条目到擦除之间不得有任何 `?`', () => {
    const body = rustCodeOnly(rustFnBody(cmd, 'pub fn autotype_candidates('));
    const list = body.indexOf('list_items(');
    const wipe = body.indexOf('wipe_plaintext()');
    expect(list, 'autotype_candidates 里没有 list_items(').toBeGreaterThan(-1);
    expect(wipe, 'autotype_candidates 里没有 wipe_plaintext()').toBeGreaterThan(list);
    const between = body.slice(list, wipe);
    expect(
      between.includes('?'),
      `list_items( 与 wipe_plaintext() 之间有 ?：\n${between}`,
    ).toBe(false);
  });

  it('auto_type_fill 失败分支必须 reveal 浮层（1.5.40：否则失败提示没人看得见）', () => {
    // 真机症状（主人 2026-11-04，7.9）：管理员权限程序里按热键选条目，
    // 「按了没反应，也没提示」。原因不是没报错——`ERR_AUTOTYPE_BLOCKED` 产生了，
    // 但注入前浮层已被 hide，前端的失败文案写进了一个**已隐藏的窗口**。
    // 判据：`result.is_ok()` 分叉之后必须出现 `reveal(&app)`。
    const body = rustCodeOnly(rustFnBody(cmd, 'pub async fn auto_type_fill('));
    const okAt = body.indexOf('result.is_ok()');
    expect(okAt, '缺 result.is_ok() 分叉').toBeGreaterThan(-1);
    const revealAt = body.indexOf('reveal(&app)');
    expect(revealAt, '失败分支没有 reveal —— 失败提示会写进已隐藏的窗口，用户看不到').toBeGreaterThan(
      okAt,
    );
    // 反过来钉住时序：reveal 不许出现在注入之前（那时还没失败，露出来只是闪一下）。
    const injectAt = body.indexOf('spawn_blocking');
    expect(injectAt, '找不到 spawn_blocking，判据失效').toBeGreaterThan(-1);
    expect(revealAt, 'reveal 必须在注入之后（失败才有意义）').toBeGreaterThan(injectAt);
  });
});
