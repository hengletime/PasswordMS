// 详情页「剪贴板里现在是什么」的字段与标签（D-166）。
//
// 抽成独立模块的唯一理由是**可测**：这张映射一旦漏项或错位，界面会静默地把动态口令
// 说成「密码」（D-146 花整轮修的就是「提示行不点名字段」）。写在组件模板里的三元表达式
// 只能靠字符串切片断言，测不出「映射本身对不对」。

/** 详情页可一键复制的三个字段。 */
export type CopiedField = 'username' | 'password' | 'totp';

/**
 * 字段 → 提示行标签的 i18n 键。
 *
 * `satisfies Record<CopiedField, string>` 保证少写一个字段时**编译期**就报错，
 * 而不是运行时 undefined → `ui.t(undefined)` 显示成 `undefined`。
 */
const LABEL_KEYS = {
  username: 'editor.field_username',
  password: 'editor.field_password',
  // 动态口令复用编辑器里那一句「动态口令」，不另造近义词——同一概念两个说法，
  // 用户会以为是两个东西。
  totp: 'editor.totp_now',
} as const satisfies Record<CopiedField, string>;

/** 取提示行要显示的字段名（i18n 键）。 */
export function copiedFieldLabelKey(field: CopiedField): string {
  return LABEL_KEYS[field];
}

/** 复制提示行的可见性输入（D-168）。 */
export interface CopyHintState {
  /** 剩余秒数。 */
  countdown: number;
  /** 剪贴板里是哪个字段；null = 无人在等提示。 */
  field: CopiedField | null;
  /** 复制发生时所在的条目；null = 不知道（例如复制时详情页没有条目）。 */
  copiedItemId: string | null;
  /** 当前详情页展示的条目。 */
  currentItemId: string | null;
}

/**
 * 提示行该不该显示（D-168）。
 *
 * 剪贴板是**全局**的，但提示行是**条目详情页的一部分**。原先只要倒计时没走完就渲染，
 * 于是复制了 A 的用户名后切到 B，B 的详情页顶上挂着「已复制用户名，剪贴板将在 23 秒后
 * 清空」——用户看到的是「我明明没在这个条目上复制过」。判据必须是「这次复制属于哪个条目」。
 *
 * `copiedItemId !== null` 那条不能省：详情页没有条目时 `currentItemId` 也是 null，
 * 两者相等，单靠相等判断会在空详情页上渲染提示。
 */
export function shouldShowCopyHint(state: CopyHintState): boolean {
  return (
    state.countdown > 0 &&
    state.field !== null &&
    state.copiedItemId !== null &&
    state.copiedItemId === state.currentItemId
  );
}
