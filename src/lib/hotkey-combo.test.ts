// 热键键位的录制与显示（v1.4.10）。
//
// 这里守的是「前端录出来的串，后端一定认」——两边规则不一致的后果是最难查的那类
// 失败：界面看着录上了，保存却报"键位不合法"，用户完全不知道错在哪。

import { describe, expect, it } from 'vitest';
import { comboFromEvent, formatHotkey, isModifierCode, type ComboEvent } from './hotkey-combo';

/** 造一个只带必需字段的按键事件。 */
function ev(code: string, mods: Partial<ComboEvent> = {}): ComboEvent {
  return { code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods };
}

describe('comboFromEvent —— 从按键事件组装规范串', () => {
  it('组装成「小写修饰符 + W3C code」', () => {
    expect(comboFromEvent(ev('KeyP', { ctrlKey: true, altKey: true }))).toBe('ctrl+alt+KeyP');
    expect(comboFromEvent(ev('Digit1', { ctrlKey: true }))).toBe('ctrl+Digit1');
    expect(comboFromEvent(ev('F5', { altKey: true }))).toBe('alt+F5');
  });

  it('修饰符顺序固定 ctrl → alt → shift → super（与后端 canonical 一致）', () => {
    // 用户实际按键顺序无法控制，输出顺序必须稳定，否则"改顺序"会被当成改键。
    expect(comboFromEvent(ev('KeyA', { metaKey: true, shiftKey: true, altKey: true, ctrlKey: true }))).toBe(
      'ctrl+alt+shift+super+KeyA',
    );
    expect(comboFromEvent(ev('KeyA', { metaKey: true, ctrlKey: true }))).toBe('ctrl+super+KeyA');
  });

  it('只按下修饰键时返回 null（录制尚未完成，不该当成录到 ctrl+ctrl）', () => {
    expect(comboFromEvent(ev('ControlLeft', { ctrlKey: true }))).toBeNull();
    expect(comboFromEvent(ev('AltRight', { altKey: true }))).toBeNull();
    expect(comboFromEvent(ev('ShiftLeft', { shiftKey: true }))).toBeNull();
    expect(comboFromEvent(ev('MetaLeft', { metaKey: true }))).toBeNull();
  });

  it('没有 Ctrl/Alt/Super 时返回 null（纯 Shift 会和正常打字打架）', () => {
    expect(comboFromEvent(ev('KeyP'))).toBeNull();
    expect(comboFromEvent(ev('KeyP', { shiftKey: true }))).toBeNull();
  });

  it('空 code 返回 null（输入法合成事件等边界）', () => {
    expect(comboFromEvent(ev('', { ctrlKey: true }))).toBeNull();
  });

  it('修饰键判定表覆盖左右键与锁定键', () => {
    for (const code of ['ControlLeft', 'ControlRight', 'AltLeft', 'MetaRight', 'CapsLock', 'NumLock']) {
      expect(isModifierCode(code), code).toBe(true);
    }
    expect(isModifierCode('KeyP')).toBe(false);
    expect(isModifierCode('F1')).toBe(false);
  });
});

describe('formatHotkey —— 规范串翻成人看的文案', () => {
  it('字母键去掉 Key 前缀、数字键去掉 Digit 前缀', () => {
    expect(formatHotkey('ctrl+alt+KeyP')).toBe('Ctrl + Alt + P');
    expect(formatHotkey('alt+Digit1')).toBe('Alt + 1');
  });

  it('功能键原样、方向键用箭头、super 显示为 Win', () => {
    expect(formatHotkey('ctrl+F5')).toBe('Ctrl + F5');
    expect(formatHotkey('ctrl+ArrowLeft')).toBe('Ctrl + ←');
    expect(formatHotkey('super+KeyL')).toBe('Win + L');
  });

  it('小键盘与三段组合', () => {
    expect(formatHotkey('ctrl+alt+shift+KeyZ')).toBe('Ctrl + Alt + Shift + Z');
    expect(formatHotkey('ctrl+Numpad5')).toBe('Ctrl + Num 5');
  });

  it('空串（用户禁用热键）返回空串，由调用方显示"未设置"', () => {
    expect(formatHotkey('')).toBe('');
    expect(formatHotkey('  ')).toBe('');
  });
});
