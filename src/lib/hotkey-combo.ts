// 全局热键的键位组合处理（v1.4.10）。
//
// 后端 `prefs::canonical_hotkey` 是**最终裁判**（它用注册器同一套解析器校验）。
// 这里只做两件事，别在这里加"更聪明"的规则：
//
//   1. 从 `KeyboardEvent` 组装规范串：修饰符小写 + W3C `event.code`
//      （`ctrl+alt+KeyP`、`ctrl+shift+Digit1`）；
//   2. 把规范串翻成人看得懂的文案（`Ctrl + Alt + P`）。
//
// 两边规则必须一致，否则会出现「录制看着成功，保存报错」这种最让人困惑的失败。

/** 组装所需的按键信息（从 `KeyboardEvent` 取这五个字段即可，纯数据便于测试）。 */
export interface ComboEvent {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

/**
 * 纯修饰键的 `code`。
 *
 * 用户按下 Ctrl 的**那一瞬间**就会触发 keydown，此时组合还没成型。收录这几个是为了
 * 在录制过程中"吃掉"它们而不是当成录到了 `ctrl+ctrl`。
 */
const MODIFIER_CODES: ReadonlySet<string> = new Set([
  'ControlLeft',
  'ControlRight',
  'AltLeft',
  'AltRight',
  'ShiftLeft',
  'ShiftRight',
  'MetaLeft',
  'MetaRight',
  // 少数键盘把 Win 键报成这两个
  'OSLeft',
  'OSRight',
  'CapsLock',
  'NumLock',
  'ScrollLock',
]);

/** 是否为"单独按下不算组合"的修饰键。 */
export function isModifierCode(code: string): boolean {
  return MODIFIER_CODES.has(code);
}

/**
 * 从按键事件组装规范串；`null` = 这一下还不构成合法组合（继续等用户按完）。
 *
 * 拒绝条件与后端一致：
 * * 只有修饰键（还在录，没按到主键）；
 * * 没有任何 Ctrl/Alt/Win（纯字母键会抢走正常输入，纯 Shift 同理）。
 */
export function comboFromEvent(event: ComboEvent): string | null {
  if (isModifierCode(event.code) || event.code === '') {
    return null;
  }
  const mods: string[] = [];
  if (event.ctrlKey) {
    mods.push('ctrl');
  }
  if (event.altKey) {
    mods.push('alt');
  }
  if (event.shiftKey) {
    mods.push('shift');
  }
  if (event.metaKey) {
    mods.push('super');
  }
  // 后端要求至少一个主修饰键（Ctrl/Alt/Super），这里先挡掉无谓的 IPC 往返。
  if (!mods.some((m) => m === 'ctrl' || m === 'alt' || m === 'super')) {
    return null;
  }
  // 顺序与后端 canonical 一致：ctrl → alt → shift → super。
  return [...mods, event.code].join('+');
}

/** 单个键名的显示文案。 */
function keyLabel(code: string): string {
  if (code.startsWith('Key') && code.length === 4) {
    return code.slice(3);
  }
  if (code.startsWith('Digit') && code.length === 6) {
    return code.slice(5);
  }
  if (code.startsWith('Numpad') && code.length > 6) {
    return `Num ${code.slice(6)}`;
  }
  const arrows: Record<string, string> = {
    ArrowLeft: '←',
    ArrowRight: '→',
    ArrowUp: '↑',
    ArrowDown: '↓',
  };
  if (code in arrows) {
    return arrows[code];
  }
  return code;
}

/**
 * 规范串 → 显示文案（`ctrl+alt+KeyP` → `Ctrl + Alt + P`）。
 *
 * `super` 显示成 `Win`：这个应用的桌面端只有 Windows 正式发布，用户心智里那叫 Win 键。
 * 空串（用户禁用了热键）返回空串，由调用方决定显示"未设置"。
 */
export function formatHotkey(spec: string): string {
  const parts = spec
    .split('+')
    .map((p) => p.trim())
    .filter((p) => p.length > 0);
  if (parts.length === 0) {
    return '';
  }
  const modNames: Record<string, string> = {
    ctrl: 'Ctrl',
    alt: 'Alt',
    shift: 'Shift',
    super: 'Win',
  };
  const key = parts[parts.length - 1] ?? '';
  const mods = parts.slice(0, -1).map((m) => modNames[m] ?? m);
  return [...mods, keyLabel(key)].join(' + ');
}
