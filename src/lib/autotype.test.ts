// 自动输入浮层的键位映射单测（1.5.40）。
//
// 为什么把这段从组件里抽出来：1.5.39 的映射是 `onKeydown` 里一段嵌套三元，
// **没有任何测试**，于是「Alt+Enter 按不出来」这个事实在真机验收前谁都不知道
// ——代码里它看起来完全正确（`event.altKey ? 'url'`），坏的是 Windows 根本不会
// 把那个事件送进 WebView。抽成纯函数之后，映射本身可以被逐条钉住；至于"这个键在
// 真机上到不了"，靠选一个没人抢的键（Ctrl+数字）来避免，而不是靠测试。
//
// 键位照抄 KeePassXC 2.8.0（`AutoTypeSelectDialog.cpp:326-350`），**顺序不许改**：
// 这套东西是肌肉记忆，差一位就会让人按 Ctrl+1 以为只打用户名、实际把密码也打了。
// 本文件的第一个 describe 就是钉这件事。
//
// 本文件只测映射；浮层的其余守门在 `autotype-guard.test.ts`（源码形状那类）。

import { describe, expect, it } from 'vitest';

import { actionForKey, type KeyLike } from './autotype';

/** 造一个按键（默认不带任何修饰键）。 */
function key(k: string, mods: Partial<Omit<KeyLike, 'key'>> = {}): KeyLike {
  return { key: k, ctrlKey: false, shiftKey: false, altKey: false, ...mods };
}

describe('actionForKey：与 KeePassXC 逐位对齐的键位表', () => {
  it('Enter = 完整序列（用户名 + Tab + 密码 + 回车）', () => {
    expect(actionForKey(key('Enter'))).toBe('default');
  });

  it('Ctrl+1 仅用户名 / Ctrl+2 仅密码 / Ctrl+3 动态口令 / Ctrl+4 网址', () => {
    // ⚠️ 顺序是 KeePassXC 的，不是"顺手程度"的：别把 url 挪到 3。
    expect(actionForKey(key('1', { ctrlKey: true }))).toBe('username');
    expect(actionForKey(key('2', { ctrlKey: true }))).toBe('password');
    expect(actionForKey(key('3', { ctrlKey: true }))).toBe('totp');
    expect(actionForKey(key('4', { ctrlKey: true }))).toBe('url');
  });

  it('Ctrl+1..4 恰好是四个单字段动作，且互不重复', () => {
    // 少一个 = 某个动作没有键位；重复 = 两个键干同一件事（用户会以为坏了一个）。
    const mapped = ['1', '2', '3', '4'].map((d) => actionForKey(key(d, { ctrlKey: true })));
    expect(mapped).toEqual(['username', 'password', 'totp', 'url']);
    expect(new Set(mapped).size).toBe(4);
    // 完整序列只在 Enter 上，不许再占用一个 Ctrl 数字（KeePassXC 也没有）。
    expect(mapped).not.toContain('default');
  });
});

describe('actionForKey：不该触发的键一律返回 null', () => {
  it('**搜索框里打数字绝不能注入**（最重要的那条）', () => {
    // 用户想在搜索框里输入条目名里的数字，或网址里的端口号。没有 Ctrl 时
    // 数字只是普通字符——一旦这里返回动作，用户打字就会把密码打进别人窗口。
    for (const digit of ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0']) {
      expect(actionForKey(key(digit)), `无 Ctrl 的 ${digit}`).toBeNull();
    }
  });

  it('Ctrl+5 不绑（KeePassXC 那里是虚拟键盘，本应用没有该能力）', () => {
    // 硬绑一个"看起来能用"的动作比留空更坏：用户以为在打验证码，实际打了别的。
    expect(actionForKey(key('5', { ctrlKey: true }))).toBeNull();
    for (const digit of ['0', '6', '7', '8', '9']) {
      expect(actionForKey(key(digit, { ctrlKey: true }))).toBeNull();
    }
  });

  it('旧快捷键不再触发：Ctrl+Enter / Alt+Enter / Ctrl+Shift+Enter 都被拒绝', () => {
    // 1.5.39 的 Ctrl+Enter 是"仅密码"、Alt+Enter 是"网址"。现在换成数字键，
    // 旧键必须明确失效——否则用户按 Ctrl+Enter 以为在打密码，实际走了 default
    // （把密码和用户名一起打进表单），而他不会收到任何提示。
    expect(actionForKey(key('Enter', { ctrlKey: true }))).toBeNull();
    expect(actionForKey(key('Enter', { altKey: true }))).toBeNull();
    expect(actionForKey(key('Enter', { ctrlKey: true, shiftKey: true }))).toBeNull();
    expect(actionForKey(key('Enter', { shiftKey: true }))).not.toBeNull();
  });

  it('带 Alt 一律不动作（Alt 会被系统菜单先吃掉，判成动作只会误触发）', () => {
    expect(actionForKey(key('4', { ctrlKey: true, altKey: true }))).toBeNull();
    expect(actionForKey(key('1', { altKey: true }))).toBeNull();
  });

  it('Ctrl+Shift+数字仍然有效（Shift 不参与判定）', () => {
    // Shift 是纯粹的手滑（左手小指压在 Shift 上很常见）；判成"无效"会让用户
    // 觉得按键时灵时不灵。它不影响动作语义，就不该否决。
    expect(actionForKey(key('4', { ctrlKey: true, shiftKey: true }))).toBe('url');
    expect(actionForKey(key('1', { ctrlKey: true, shiftKey: true }))).toBe('username');
  });

  it('纯字母 / 方向键 / Esc 都不是动作键（由调用方各自处理）', () => {
    for (const k of ['a', 'ArrowDown', 'ArrowUp', 'Escape', 'Tab']) {
      expect(actionForKey(key(k)), k).toBeNull();
    }
  });
});
