// D-200 守门：换热键时，本程序自己泄漏在系统里的旧键位必须能被收回；录制没反应必须出句话。
//
// 背景（主人真机 2026-11-06，公司台式机）：
// ① 想把「自动输入快捷键」改成 Ctrl+Alt+L，反复提示「该热键已被其它程序占用」，换成别的
//    键也一样；
// ② 改过上面的「全局热键」之后，自动输入键又能改了；可这时想把全局热键改回去，**什么
//    提示都没有**。
//
// 两件事同源：`tauri-plugin-global-shortcut` 的 `unregister_all` 是「先把内部表
// `mem::take` 清空，再逐条对系统 `UnregisterHotKey`」，底层的 Windows 实现遇到第一条错误
// 就 `return`。只要那一步错一次，那条键就**留在系统里、而插件已经忘了自己持有它**。
// 之后凡是包含这颗键的注册都回「已被注册」，我们把它译成「被其它程序占用」——占它的
// 其实是本程序自己；而用户按这颗键想让录制器接住时，系统把它拦走了，窗口收不到 `keydown`，
// 于是「什么提示都没有」。
//
// 本文件钉两件事（后端一半靠 `hotkey.rs` 的源码结构，前端一半靠字典 + 调用点）：
// ① `register` 必须先注销（`unregister_all` 失败要留痕）**再按本次键位定向撤一次**，
//    才允许注册；顺序错了或者把错误吞掉，用例就红；
// ② 录制态静置必须主动解释一句：四语言键真实存在，且两个录制入口都挂上了计时器。
//
// 注：这两条是**结构守门**，不是行为等价物——真机行为要主人在 Windows 上按一遍验。

import { describe, expect, it } from 'vitest';

import { hasKey, LOCALES, translate } from './i18n/index';

const KEY = 'settings.hotkey_no_response';

const modules = import.meta.glob(['../components/Settings.svelte', '../../src-tauri/src/hotkey.rs'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const hotkeyRs = modules['../../src-tauri/src/hotkey.rs'] ?? '';
const settingsSvelte = modules['../components/Settings.svelte'] ?? '';

/** 取出 `register` 函数体：从 `fn register(` 到下一个顶层项（`capture_autotype_target`）。 */
function registerBody(): string {
  const from = hotkeyRs.indexOf('fn register(');
  expect(from, 'hotkey.rs 里找不到 fn register(').toBeGreaterThan(-1);
  const to = hotkeyRs.indexOf('fn capture_autotype_target', from);
  expect(to, '找不到 register 之后的锚点，切不出函数体').toBeGreaterThan(from);
  return hotkeyRs.slice(from, to);
}

describe('D-200：改热键前必须先把本程序自己的旧键位撤干净', () => {
  it('守门别空跑：两个源文件都真的读到了', () => {
    expect(hotkeyRs.length, 'hotkey.rs 读成空串，下面的断言全是假绿').toBeGreaterThan(1000);
    expect(settingsSvelte.length, 'Settings.svelte 读成空串').toBeGreaterThan(1000);
  });

  it('unregister_all 的错误不许再被 `let _ =` 吞掉（它失败正是残留的来源）', () => {
    expect(hotkeyRs).not.toContain('let _ = gs.unregister_all()');
    // 失败必须落到日志，否则下次再出同样的事仍然只有"被其它程序占用"一句话。
    expect(registerBody()).toContain('if let Err(e) = gs.unregister_all()');
    expect(registerBody()).toContain('[hotkey] 注销旧键位失败');
  });

  it('注册前要对本次两个键位各定向注销一次，且排在第一个 on_shortcut 之前', () => {
    const body = registerBody();
    // 定向注销（`gs.unregister(shortcut)`）与无差别注销（`gs.unregister_all()`）是两个东西，
    // 这里必须看到前者——它是"插件已经忘了、系统还记着"那颗键的唯一收回手段。
    expect(body).toContain('gs.unregister(shortcut)');
    const targeted = body.indexOf('gs.unregister(shortcut)');
    const firstRegister = body.indexOf('on_shortcut(');
    expect(targeted, '没有 on_shortcut 可比较，函数体切错了').toBeGreaterThan(-1);
    expect(firstRegister).toBeGreaterThan(-1);
    expect(targeted, '定向注销必须发生在注册之前，否则系统仍然回「已被注册」').toBeLessThan(
      firstRegister,
    );
    // 两颗键都要覆盖：写成只撤主键的话，改自动输入键时照样卡住。
    expect(body).toContain('for spec in [main, autotype].into_iter().flatten()');
  });

  it('录制态静置要主动说话：两个录制入口都挂上了计时器', () => {
    // 定义 + 两处调用：`const stall = startStallHint();` 必须出现两次（主键 / 自动输入键）。
    const calls = settingsSvelte.match(/const stall = startStallHint\(\);/g) ?? [];
    expect(calls.length, '两个录制入口没有都挂上静置提示').toBe(2);
    // 录到东西就要撤掉计时器（否则会一边报错一边说"按了没反应"）：两处 onKeydown + 两处 cleanup。
    const cleared = settingsSvelte.match(/window\.clearTimeout\(stall\)/g) ?? [];
    expect(cleared.length, '静置计时器没有在录入/销毁时撤掉').toBe(4);
    expect(settingsSvelte).toContain(`'${KEY}'`);
  });

  it('四种语言都有这句提示，且不是空话', () => {
    for (const locale of LOCALES) {
      expect(hasKey(locale, KEY), `${locale} 缺键 ${KEY}`).toBe(true);
      const text = translate(locale, KEY);
      expect(text, `${locale} 的 ${KEY} 原样返回了键名`).not.toBe(KEY);
      expect(text.length, `${locale} 的 ${KEY} 太短，解释不了任何事`).toBeGreaterThan(20);
    }
    // 简中必须点出"是全局热键把它拦下来了"，否则用户只会以为程序卡了。
    expect(translate('zh-CN', KEY)).toContain('全局热键');
  });
});
