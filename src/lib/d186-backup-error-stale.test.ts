// D-186 守门：备份没存成的那条提示，**不许既过时又下不去**。
//
// 主人 2026-11-04 的截图：设置页同时显示
//   「上次备份：2026-11-04 10:30」
//   「最近一次备份没存成（磁盘空间或配额不足），时间 2026-11-04 10:28」
// 并问「好像备份不了了」。现场 `.state` 是 `quota_exceeded`，10:30 那次其实成功了
// ——是「粘性失败记录 + 没有清除出口」把一句过时的话说成了"现在坏了"。
//
// 另有一处纯格式错误：收起态摘要印出「共 4 份（519484）」，而同一张卡片表头写着
// 「507.3 KB」。`restore_point_summary` 的 `{size}` 要的是人看的体积，这里塞了字节数。
//
// 五条可判定事实（本机没有 jsdom，只读源码）：
//   1. 摘要体积必须过 `formatBytes`
//   2. 之后成功过 → 用「有一次…之后已恢复」那句，不再用"最近一次"
//   3. 有「知道了」按钮，且它真的调 `backup_clear_error`
//   4. 磁盘满与额度满必须分成两种原因（`no_space` / `quota_exceeded`）
//   5. 额度满要给出"该做什么"（删几份旧备份），别的原因不许瞎给
import { describe, expect, it } from 'vitest';

const sources = import.meta.glob(
  ['../components/Settings.svelte', './i18n/zh-CN.ts', './i18n/zh-TW.ts', './i18n/en-US.ts', './i18n/th.ts', './ipc.ts'],
  { query: '?raw', eager: true, import: 'default' },
) as Record<string, string>;

const src = (name: string): string => {
  const hit = Object.entries(sources).find(([k]) => k.endsWith('/' + name));
  return hit ? hit[1] : '';
};

const SETTINGS = src('Settings.svelte');
const IPC = src('ipc.ts');

describe('D-186 备份失败提示不再永久误报', () => {
  it('摘要里的体积必须格式化，不能直接印字节数', () => {
    expect(SETTINGS.length).toBeGreaterThan(5000);
    // 摘要在 `$derived.by` 里组装；只看 `size:` 那一行
    const line = SETTINGS.split('\n').find((l) => l.trim().startsWith('size:'));
    expect(line, '找不到摘要的 size 字段').toBeTruthy();
    expect(line).toContain('formatBytes(');
    // 反向：裸的 sumBytes 直接当 size 就是这一条的原始错误写法
    expect(line).not.toMatch(/size:\s*sumBytes\(/);
  });

  it('之后成功过就用「有一次…已恢复」的说法，而不是「最近一次」', () => {
    // 判据必须是两个时间戳比较：谁更晚决定"现在到底坏没坏"
    expect(SETTINGS).toMatch(
      /backupStatus\.last_ms\s*>\s*backupStatus\.last_error_ms/,
    );
    expect(SETTINGS).toContain("settings.restore_point_error_recovered");
    // 「最近一次」那句只在没有更晚的成功时才允许出现
    const tpl = SETTINGS.slice(SETTINGS.indexOf('set-backup-error'));
    const recoveredAt = tpl.indexOf('restore_point_error_recovered');
    const latestAt = tpl.indexOf("settings.restore_point_error'");
    expect(recoveredAt).toBeGreaterThan(-1);
    expect(latestAt).toBeGreaterThan(recoveredAt);
  });

  it('有一个「知道了」按钮，并且它真的去清后端记录', () => {
    expect(SETTINGS).toContain('dismissBackupError');
    expect(SETTINGS).toContain('onclick={dismissBackupError}');
    expect(SETTINGS).toContain('api.backups.clearError()');
    // 前端调用的名字必须与后端命令一致
    expect(IPC).toContain("invoke<void>('backup_clear_error')");
  });

  it('磁盘满与额度满分开说，且额度满给出该做什么', () => {
    expect(SETTINGS).toContain("case 'no_space':");
    expect(SETTINGS).toContain("case 'quota_exceeded':");
    expect(SETTINGS).toContain('restore_point_reason_no_space');
    expect(SETTINGS).toContain('restore_point_reason_quota');
    // 合并成一句「磁盘空间或配额不足」就是本条的原始错误写法
    expect(SETTINGS).not.toContain('restore_point_reason_space');
    expect(SETTINGS).toContain('restore_point_quota_hint');
  });

  it('四语言都有这四个新键，且旧的合并键已删干净', () => {
    const keys = [
      'restore_point_error_recovered',
      'restore_point_error_dismiss',
      'restore_point_reason_no_space',
      'restore_point_reason_quota',
      'restore_point_quota_hint',
    ];
    for (const lang of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
      const dict = src(lang + '.ts');
      expect(dict.length, lang + ' 字典读不到').toBeGreaterThan(1000);
      for (const key of keys) {
        expect(dict, `${lang} 缺 ${key}`).toContain(key + ':');
      }
      expect(dict, `${lang} 还留着合并键`).not.toContain('restore_point_reason_space:');
    }
  });
});
