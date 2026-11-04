// D-184 守门：备份卡片默认收起，但**异常提示不许跟着收起**。
//
// 主人 2026-11-04：「备份这个卡片在设置里占了很大地盘，能不能平常不用时是缩在上面，
// 只要有几行提示……用户需要时可点击按钮向下展开。」
//
// 这里守三条可判定的结构事实（本机没有 jsdom，跑不了真实点击）：
//   1. 默认收起：`backupExpanded` 初值 false，且有 `aria-expanded` 的切换按钮；
//   2. 收起后仍有信息：摘要行必须带「其中几份要用保存时的密码」这一截
//      ——它是用户唯一无法从"共 N 份"里推出来、却决定"退回去要输哪把密码"的信息；
//   3. 折叠块里**不含** `backupHints`：那是"盘上有文件但界面删不了"的异常态，
//      收起来等于假装没有——省地方的动机不能顺手吃掉告警。
import { describe, expect, it } from 'vitest';

const sources = import.meta.glob(['../components/Settings.svelte'], {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

const SRC: string = sources['../components/Settings.svelte'] ?? '';

/**
 * 从 `startMarker` 起到**配平**的结尾，返回块内文本。
 *
 * Svelte 模板的开块是 `{#if` / `{#each` / `{#key` / `{#await`，闭块统一 `{/...}`；
 * 这里只数 `{#` 与 `{/`，足够配平（块内不会有语法糖之外的裸花括号成对出现）。
 */
function blockFrom(startMarker: string): string | null {
  const at = SRC.indexOf(startMarker);
  if (at < 0) {
    return null;
  }
  let depth = 0;
  let i = at;
  while (i < SRC.length) {
    if (SRC.startsWith('{#', i)) {
      depth += 1;
      i += 2;
    } else if (SRC.startsWith('{/', i)) {
      depth -= 1;
      i += 2;
      if (depth === 0) {
        return SRC.slice(at, i);
      }
    } else {
      i += 1;
    }
  }
  return null;
}

describe('D-184 备份卡片默认收起', () => {
  it('守门本身没空跑：能读到组件且拿到展开块', () => {
    expect(SRC.length).toBeGreaterThan(10000);
    expect(blockFrom('{#if backupExpanded}')).not.toBeNull();
  });

  it('默认收起，且有一个可点击的展开/收起按钮', () => {
    expect(SRC).toContain('let backupExpanded = $state(false)');
    expect(SRC).toContain('aria-expanded={backupExpanded}');
    expect(SRC).toContain("backupExpanded\n                ? 'settings.restore_point_collapse'");
  });

  it('收起态摘要必须说出"其中几份要用保存时的密码"', () => {
    expect(SRC).toContain('restore_point_summary_old');
    // 判据来自后端给的 `needs_old_password`，不许在这层用文件名/目录再推一遍。
    expect(SRC).toContain('p.needs_old_password');
  });

  it('两张表与自动清理开关在折叠块内（收起时确实省掉地方）', () => {
    const block = blockFrom('{#if backupExpanded}') ?? '';
    expect(block).toContain('set-group-head');
    expect(block).toContain('set-retention');
    expect(block).toContain('restore_point_table_hint');
  });

  it('"盘上有但删不掉"的告警留在折叠外（异常态不许藏）', () => {
    const block = blockFrom('{#if backupExpanded}') ?? '';
    expect(block).not.toContain('backupHints(');
    // 反面证据：它必须还在组件里（不是被顺手删掉）。
    expect(SRC).toContain('backupHints(backupStatus)');
  });

  it('摘要用独立类，不寄生在 .set-row 的变体上（否则窄屏守门读数错位）', () => {
    expect(SRC).toContain('set-backup-summary');
    expect(SRC).not.toContain('set-row is-actions set-backup-summary');
  });
});
