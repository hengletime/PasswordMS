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
