// D-166 守门：详情页的动态口令必须能**一键复制**，且复制后提示行说的字段是对的。
//
// 主人的原话是「TOTP 口令不能一键复制，下个功能版顺手加上」。这个缺陷本身很小，
// 但复制这条链路在 1.5.x 上已经出过两次事故（D-135 把 TTL=0 解释成「立刻清空」、
// D-146 提示行不点名字段），所以这里既不测「按钮存在」也不测「文案写着」，
// 而是测三件会真出错的事：
//
// 1. 字段 → 标签的映射表本身（纯函数，可喂数据；写错会静默说成「密码」）；
// 2. 复制调用**带字段名**、且空口令不允许复制（否则复制一个空串还弹「已复制」）；
// 3. 动态口令走的是与密码**同一条** TTL 清空路径，不是另写一份「复制但不清理」。

import { describe, expect, it } from 'vitest';

import { copiedFieldLabelKey, type CopiedField } from './copied-field';
import { enUS } from './i18n/en-US';
import { th } from './i18n/th';
import { zhCN } from './i18n/zh-CN';
import { zhTW } from './i18n/zh-TW';

const components = import.meta.glob('../components/ItemDetail.svelte', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const detail = Object.values(components)[0] ?? '';

/** 取 `[from, to)` 之间的片段；找不到标记直接失败（而不是悄悄返回空串）。 */
function section(source: string, from: string, to: string, label: string): string {
  const start = source.indexOf(from);
  expect(start, `${label}：起点标记 ${from} 不存在`).toBeGreaterThanOrEqual(0);
  const end = source.indexOf(to, start);
  expect(end, `${label}：终点标记 ${to} 不存在`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe('D-166 字段标签映射（行为判据）', () => {
  it('三个字段各自指向自己的标签，不能互相串', () => {
    const fields: CopiedField[] = ['username', 'password', 'totp'];
    const keys = fields.map((f) => copiedFieldLabelKey(f));
    expect(keys).toEqual(['editor.field_username', 'editor.field_password', 'editor.totp_now']);
    // 互不相同是核心：任意两个相同 ⇒ 界面上必然有一句是错的。
    expect(new Set(keys).size).toBe(3);
  });

  it('动态口令不得复用「密码」的标签键', () => {
    // 这一条单独写，是因为把 totp 映射到 editor.field_password 是最省事、也最像
    // 「顺手复制一行」的错法：用户会看到「已复制密码」，而剪贴板里是 6 位口令。
    expect(copiedFieldLabelKey('totp')).not.toBe(copiedFieldLabelKey('password'));
    expect(copiedFieldLabelKey('totp')).toBe('editor.totp_now');
  });
});

describe('D-166 组件接线', () => {
  it('动态口令行里有复制按钮，并调用 copyText(码, "totp")', () => {
    // 区间取「TOTP 行起点 → 剪贴板提示行起点」，整段口令行都在里面（按钮的注释
    // 就在按钮上方，所以不能用注释当终点标记，否则切不到按钮本身）。
    const totpBlock = section(detail, '{#if totpSecret}', '{#if copyHintVisible}', 'TOTP 行');
    // 复制按钮的 aria-label 用的是新增的 item.copy_totp。
    expect(totpBlock).toContain("ui.t('item.copy_totp')");
    // 必须把**当前这一屏上的码**交给 copyText，且字段名是 totp。
    expect(totpBlock).toContain("copyText(totpCode, 'totp')");
  });

  it('码还没算出来（空串）时按钮禁用，不能复制空气还报「已复制」', () => {
    const totpBlock = section(detail, '{#if totpSecret}', '{#if copyHintVisible}', 'TOTP 行');
    expect(totpBlock).toContain('disabled={totpCode.length === 0}');
  });

  it('口令复制走 copyText 这条统一路径（= TTL 自动清空 + 使用埋点）', () => {
    // copyText 内部必须是那一份带 TTL 的实现；它一旦被拆成两份，「清空」就会漏在
    // 某一支上——那正是 D-135 的形态（同一参数两种解释）。
    const copyFn = section(
      detail,
      'async function copyText(',
      'async function openUrl(',
      'copyText',
    );
    expect(copyFn).toContain('copySecretWithAutoClear(text, ttl');
    expect(copyFn).toContain('copiedField = field');
    // 动态口令按钮只有一个出口：全文里 copyText 的调用共 3 处（用户名/密码/口令）。
    expect(detail.match(/void copyText\(/g) ?? []).toHaveLength(3);
  });

  it('提示行按**实际复制过的字段**取标签（不是写死「密码」）', () => {
    // 映射表对了、模板却写死一个键，界面上照样永远说「密码」。所以这条断言的是
    // 「拿 copiedField 去查表」这个动作本身。
    expect(detail).toContain('copiedFieldLabelKey(copiedField)');
  });

  it('提示行排在所有可复制字段之后（口令复制后提示不能跑到它上面）', () => {
    const hintAt = detail.indexOf('{#if copyHintVisible}');
    const totpAt = detail.indexOf('{#if totpSecret}');
    expect(hintAt).toBeGreaterThan(0);
    expect(totpAt).toBeGreaterThan(0);
    expect(hintAt).toBeGreaterThan(totpAt);
  });
});

describe('D-166 四种语言的按钮文案', () => {
  it('item.copy_totp 在四个字典里都存在且非空', () => {
    for (const [name, dict] of [
      ['zh-CN', zhCN],
      ['zh-TW', zhTW],
      ['en-US', enUS],
      ['th', th],
    ] as const) {
      const label = (dict as unknown as { item: { copy_totp?: string } }).item.copy_totp;
      expect(label, `${name}:item.copy_totp`).toBeTruthy();
      expect((label ?? '').trim().length, `${name}:item.copy_totp`).toBeGreaterThan(0);
    }
  });
});
