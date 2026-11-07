// D-201（审计 S8）守门：两颗热键不许录成同一个组合。
//
// 背景：主热键与自动输入热键是**两颗独立的全局热键**，同一个组合在系统里只能注册一次。
// 用户把第二颗录成第一颗的值时，第二次 `on_shortcut` 必然失败，而失败信息被译成
// 「该热键已被其它程序占用」——占它的其实是本程序。用户按这句提示只会反复换**这颗**
// 组合，永远改不好，因为他真正要改的是另一颗。
//
// 本文件钉四件事（都是**结构/文案**守门，真机行为要主人在 Windows 上按一遍验）：
// ① `HotkeyReject` 有专门的 `SameAsOther`，且 `map_reject` 把它映射到**专用**文案键；
// ② 两个改键入口（`apply` / `apply_autotype`）都在 `register` **之前**就拒，
//    这样系统状态一个字节都不动、也不需要回滚，更不会被译成"被其它程序占用"；
// ③ 启动装载与回滚这两条不过前端的路上，重复的那颗要跳过并留痕，不能把主热键一起打死；
// ④ 四语言文案都在、都说得清"是另一颗在用"，且不许与 `hotkey_unavailable` 撞成同一句。

import { describe, expect, it } from 'vitest';

import { hasKey, LOCALES, translate } from './i18n/index';

const KEY = 'err.hotkey_same_as_other';
const TAKEN_KEY = 'err.hotkey_unavailable';

const modules = import.meta.glob(
  [
    '../../src-tauri/src/hotkey.rs',
    '../../src-tauri/src/prefs.rs',
    '../../src-tauri/src/commands/prefs_cmd.rs',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

const hotkeyRs = modules['../../src-tauri/src/hotkey.rs'] ?? '';
const prefsRs = modules['../../src-tauri/src/prefs.rs'] ?? '';
const prefsCmdRs = modules['../../src-tauri/src/commands/prefs_cmd.rs'] ?? '';

/** 从 `from` 切到下一个锚点 `to`（都按首次出现算）。切不出来就让用例红，别静默返回空串。 */
function slice(src: string, from: string, to: string, label: string): string {
  const a = src.indexOf(from);
  expect(a, `${label}：找不到起点 ${from}`).toBeGreaterThan(-1);
  const b = src.indexOf(to, a + from.length);
  expect(b, `${label}：找不到终点 ${to}（切不出函数体）`).toBeGreaterThan(a);
  return src.slice(a, b);
}

const applyBody = (): string =>
  slice(hotkeyRs, 'pub fn apply(app', '/// 当前生效的键位', 'apply');

const applyAutotypeBody = (): string =>
  slice(
    hotkeyRs,
    'pub fn apply_autotype(',
    '/// 回滚用的自动输入热键旧值',
    'apply_autotype',
  );

const registerBody = (): string =>
  slice(hotkeyRs, 'fn register(', 'fn capture_autotype_target', 'register');

describe('D-201：两颗热键不能是同一个组合', () => {
  it('守门别空跑：三个源文件都真的读到了', () => {
    expect(hotkeyRs.length, 'hotkey.rs 读成空串，下面的断言全是假绿').toBeGreaterThan(1000);
    expect(prefsRs.length, 'prefs.rs 读成空串').toBeGreaterThan(1000);
    expect(prefsCmdRs.length, 'prefs_cmd.rs 读成空串').toBeGreaterThan(1000);
  });

  it('拒因是独立变体，且有独立文案键（不许并进"被占用"）', () => {
    // 变体存在：并进 NoModifier / UnknownKey 都说不清"是另一颗在用"。
    expect(prefsRs).toContain('SameAsOther');
    // 判据是纯函数、可单测，而且明确要求"空串不算撞"。
    expect(prefsRs).toContain('pub fn hotkey_collides(');
    const arm = prefsCmdRs.match(/HotkeyReject::SameAsOther\s*=>\s*"([^"]+)"/);
    expect(arm, 'map_reject 里没有单独处理 SameAsOther 的分支').not.toBeNull();
    expect(arm?.[1], 'SameAsOther 被映射到别的文案键了').toBe(KEY);
    // 也不许被并进"被占用"那一支——那正是本缺陷的用户可见症状。
    const takenArm = prefsCmdRs.match(/HotkeyReject::[^=]*=>\s*"err\.hotkey_unavailable"/);
    expect(takenArm, 'SameAsOther 不许走 err.hotkey_unavailable').toBeNull();
  });

  it('两个改键入口都在 register 之前拒，系统状态不动、不用回滚', () => {
    for (const [label, body] of [
      ['apply', applyBody()],
      ['apply_autotype', applyAutotypeBody()],
    ] as const) {
      expect(body.length, `${label} 函数体切错了`).toBeGreaterThan(300);
      expect(body, `${label} 没调 same_pair 判撞车`).toContain('same_pair(');
      expect(body, `${label} 判出撞车后没有返回专用拒因`).toContain(
        'ApplyError::Invalid(HotkeyReject::SameAsOther)',
      );
      const check = body.indexOf('same_pair(');
      const reg = body.indexOf('register(app');
      expect(reg, `${label} 里没有 register 调用，扫描面不对`).toBeGreaterThan(-1);
      // 顺序是判据本身：排在 register 之后就等于"先注册失败、再编个理由"，用户会先看到
      // 一次多余的注销/回滚，日志里还会多出一条"被占用"。
      expect(check, `${label} 的撞车判断必须排在 register 之前`).toBeLessThan(reg);
    }
  });

  it('启动装载与回滚也要兜住：重复的那颗跳过并留痕，主热键照装', () => {
    // register 层：配置里两颗相同（手改 prefs.json / 更早版本留下的）时跳过自动输入键。
    const body = registerBody();
    expect(body).toContain('hotkey_collides(');
    expect(body).toContain('[hotkey] 自动输入热键与主热键撞成同一个组合');
    // 启动装载层：不能因为撞车连主热键都不装（它是唯一能把窗口唤回来的东西）。
    // `install_from_prefs` 是文件里最后一个函数，切到文件尾即可。
    const bootFrom = hotkeyRs.indexOf('pub fn install_from_prefs(');
    expect(bootFrom, 'hotkey.rs 里找不到 install_from_prefs').toBeGreaterThan(-1);
    const boot = hotkeyRs.slice(bootFrom);
    expect(boot.length, 'install_from_prefs 函数体切错了').toBeGreaterThan(200);
    expect(boot, 'install_from_prefs 没有单独处理撞车').toContain(
      'Err(ApplyError::Invalid(HotkeyReject::SameAsOther))',
    );
    expect(boot, '撞车时没有退一步只装主键').toContain('register(app, canonical.as_deref(), None)');
  });

  it('四语言都有这句提示，且说得清"是另一颗在用"', () => {
    const taken = new Map(LOCALES.map((l) => [l, translate(l, TAKEN_KEY)]));
    for (const locale of LOCALES) {
      expect(hasKey(locale, KEY), `${locale} 缺键 ${KEY}`).toBe(true);
      const text = translate(locale, KEY);
      expect(text, `${locale} 的 ${KEY} 原样返回了键名`).not.toBe(KEY);
      // 先比"撞不撞句"，再比长度：顺序反了的话，抄成一句更短的话会被长度门先拦下，
      // 看起来红得对、红的其实是另一条断言（变异验证时踩过一次）。
      expect(text, `${locale} 的 ${KEY} 与 ${TAKEN_KEY} 撞成同一句`).not.toBe(taken.get(locale));
      expect(text.length, `${locale} 的 ${KEY} 太短，解释不了任何事`).toBeGreaterThan(20);
    }
    // 简中必须点出"另一颗"，否则用户仍然只会在这一颗上来回换。
    expect(translate('zh-CN', KEY)).toContain('另一颗');
  });
});
