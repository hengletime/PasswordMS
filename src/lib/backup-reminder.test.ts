// 「上次成功导出备份」记录（D-144 / D-146）的行为级测试。
//
// 为什么值得单独测：这段代码本身不复杂，但它记录的是**用户唯一的复查凭据**——
// 2026-10-31 的事故里，用户之所以相信备份成功，就是因为没有任何地方能让他
// 回头看一眼「上次到底写了多少字节」。这里要钉死三件事：
//   ① 只接受正字节数（0 / 负数 / NaN 一律清记录，不许留下「0 B 的最近备份」）；
//   ② 记录损坏时不抛异常（设置页不能因为一段坏 JSON 崩掉）；
//   ③ 超过 30 天或从未导出 → 该提醒。

import { describe, expect, it } from 'vitest';

import {
  BACKUP_LAST_KEY,
  BACKUP_STALE_DAYS,
  backupAgeDays,
  lastBackup,
  recordBackup,
  shouldPromptBackup,
  type BackupStorage,
} from './backup-reminder';

/** 内存版 storage，与 localStorage 同形。 */
function memoryStorage(): BackupStorage & { dump(): string | null } {
  const map = new Map<string, string>();
  return {
    getItem: (k: string): string | null => map.get(k) ?? null,
    setItem: (k: string, v: string): void => {
      map.set(k, v);
    },
    removeItem: (k: string): void => {
      map.delete(k);
    },
    dump: (): string | null => map.get(BACKUP_LAST_KEY) ?? null,
  };
}

describe('recordBackup —— 只记真备份', () => {
  it('正字节数：写入并可原样读回', () => {
    const s = memoryStorage();
    recordBackup(s, 8_208, 1_700_000_000_000);

    expect(lastBackup(s)).toEqual({ atMs: 1_700_000_000_000, bytes: 8_208 });
  });

  it('D-144：0 字节 / 负数 / NaN 一律不记（并清掉旧记录）', () => {
    const s = memoryStorage();
    recordBackup(s, 8_208, 1_700_000_000_000);
    expect(lastBackup(s)).not.toBeNull();

    // 0 字节是事故形态：宁可显示「从未导出」，也不能让用户看到一条「最近备份 0 B」。
    recordBackup(s, 0, 1_700_000_000_000);
    expect(lastBackup(s)).toBeNull();

    recordBackup(s, 8_208, 1_700_000_000_000);
    recordBackup(s, -1, 1_700_000_000_000);
    expect(lastBackup(s)).toBeNull();

    recordBackup(s, 8_208, 1_700_000_000_000);
    recordBackup(s, Number.NaN, 1_700_000_000_000);
    expect(lastBackup(s)).toBeNull();
  });

  it('时间戳非法时不记（防止 1970 年的假记录）', () => {
    const s = memoryStorage();
    recordBackup(s, 8_208, 0);
    expect(lastBackup(s)).toBeNull();
  });
});

describe('lastBackup —— 坏记录不炸设置页', () => {
  it('无记录返回 null', () => {
    expect(lastBackup(memoryStorage())).toBeNull();
  });

  it('非 JSON / 字段缺失 / 负值 一律 null，且不抛异常', () => {
    const s = memoryStorage();
    s.setItem(BACKUP_LAST_KEY, 'not json at all');
    expect(lastBackup(s)).toBeNull();

    s.setItem(BACKUP_LAST_KEY, '{"atMs":1700000000000}');
    expect(lastBackup(s)).toBeNull();

    s.setItem(BACKUP_LAST_KEY, '{"atMs":1700000000000,"bytes":-5}');
    expect(lastBackup(s)).toBeNull();

    s.setItem(BACKUP_LAST_KEY, 'null');
    expect(lastBackup(s)).toBeNull();
  });
});

describe('backupAgeDays / shouldPromptBackup —— 该提醒时提醒', () => {
  const day = 86_400_000;
  const base = 1_700_000_000_000;

  it('无记录 → 天数未知、需要提醒（从未导出是最该提醒的状态）', () => {
    expect(backupAgeDays(null, base)).toBeNull();
    expect(shouldPromptBackup(null, base)).toBe(true);
  });

  it('刚导出 → 0 天、不提醒', () => {
    const rec = { atMs: base, bytes: 8_208 };
    expect(backupAgeDays(rec, base + 1000)).toBe(0);
    expect(shouldPromptBackup(rec, base + 1000)).toBe(false);
  });

  it(`超过 ${BACKUP_STALE_DAYS} 天 → 提醒；未到 → 不提醒`, () => {
    const rec = { atMs: base, bytes: 8_208 };
    const justUnder = base + (BACKUP_STALE_DAYS - 1) * day;
    const justOver = base + BACKUP_STALE_DAYS * day;

    expect(shouldPromptBackup(rec, justUnder)).toBe(false);
    expect(shouldPromptBackup(rec, justOver)).toBe(true);
  });

  it('时钟回拨 → 0 天（不报负数天）', () => {
    const rec = { atMs: base, bytes: 8_208 };
    expect(backupAgeDays(rec, base - 10 * day)).toBe(0);
  });
});
