// 「上次成功导出备份」的记录（D-144 / D-146）。
//
// 为什么要有这个：2026-10-31 真机事故的完整链路是 ——
//   ① Android 上导出保险库静默失败，SAF 只留下 0 字节占位文件；
//   ② 界面只有一句「确定」，用户以为备份好了；
//   ③ 卸载应用 → `allowBackup=false` 使 /data/data 整体消失 → 数据全没；
//   ④ 拿那份 0 字节"备份"恢复 → 「输入无效」→ 到此才暴露。
//
// ①③④ 各有对应的代码修复（content:// 分支 + 回读校验 + 系统卸载提示 + 文案分叉），
// 但②这一环是「用户认知」问题：**没有任何地方能让他复查「我上一次备份成功了吗、多大」**。
// 本模块把这件事记下来，设置页据此显示一行事实（时间 + 体积），而不是让用户凭记忆。
//
// 存 localStorage 而不是库文件：这是**关于备份的元信息**，不是秘密数据；
// 且卸载后本来就该跟着消失——用户重装后看到「从未导出备份」，正是想让他看到的事实。
//
// 纯逻辑、不碰 window（storage 由调用方注入），Node 测试环境可直接跑。

const KEY_LAST_BACKUP = 'pms_backup_last_v1';

/** 一次成功导出的记录。 */
export interface BackupRecord {
  /** 写入完成时刻（Unix ms）。 */
  atMs: number;
  /** 实际写入字节数（D-144 起由后端回报）。 */
  bytes: number;
}

/** 与 `localStorage` 同形的最小接口（测试可注入内存实现）。 */
export interface BackupStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/**
 * 记一次成功导出。
 *
 * **只接受正字节数**：0 字节在 `vault-export.ts` 里已被判为失败，走不到这里；
 * 这里的守卫是第二道——若将来有人把 0 也当成功传进来，宁可清掉旧记录，
 * 也不让一个「0 B 的最近备份」误导用户。
 */
export function recordBackup(storage: BackupStorage, bytes: number, atMs: number): void {
  if (!Number.isFinite(bytes) || bytes <= 0 || !Number.isFinite(atMs) || atMs <= 0) {
    storage.removeItem(KEY_LAST_BACKUP);
    return;
  }
  const record: BackupRecord = { atMs, bytes };
  storage.setItem(KEY_LAST_BACKUP, JSON.stringify(record));
}

/**
 * 读取上次成功导出的记录；无记录或记录损坏一律返回 `null`。
 *
 * 不抛异常：这是设置页的一行辅助信息，**读不出来就不显示**，
 * 不该让整个设置页因为一段坏 JSON 崩掉。
 */
export function lastBackup(storage: BackupStorage): BackupRecord | null {
  const raw = storage.getItem(KEY_LAST_BACKUP);
  if (raw === null) {
    return null;
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== 'object' || parsed === null) {
      return null;
    }
    const v = parsed as Record<string, unknown>;
    const atMs = v.atMs;
    const bytes = v.bytes;
    if (typeof atMs !== 'number' || typeof bytes !== 'number') {
      return null;
    }
    if (!Number.isFinite(atMs) || atMs <= 0 || !Number.isFinite(bytes) || bytes <= 0) {
      return null;
    }
    return { atMs, bytes };
  } catch {
    return null;
  }
}

/**
 * 距上次成功导出的天数（向下取整）；无记录返回 `null`。
 *
 * `nowMs` 由调用方传入（不在这里读时钟），使测试完全确定。
 * 时钟回拨（`nowMs < atMs`）按 0 天处理——宁可少报，不报负数天。
 */
export function backupAgeDays(record: BackupRecord | null, nowMs: number): number | null {
  if (record === null) {
    return null;
  }
  const delta = nowMs - record.atMs;
  if (delta <= 0) {
    return 0;
  }
  return Math.floor(delta / 86_400_000);
}

/** 超过这个天数没导出，就把那行提示升级为「建议马上备份」。 */
export const BACKUP_STALE_DAYS = 30;

/** 是否该提醒用户重新备份（从未导出 / 已超过 {@link BACKUP_STALE_DAYS} 天）。 */
export function shouldPromptBackup(record: BackupRecord | null, nowMs: number): boolean {
  const days = backupAgeDays(record, nowMs);
  return days === null || days >= BACKUP_STALE_DAYS;
}

/** 仅测试使用：清掉记录键名，避免用例之间相互污染时写错字符串。 */
export const BACKUP_LAST_KEY = KEY_LAST_BACKUP;
