// PasswordMS 前端常量（架构文档 §9.3）。
//
// ⚠️ 本文件与 `crates/passwordms-core/src/constants.rs` **同名同值**，修改必须同步。

// ===== 二维码 / 协议 =====
/** 二维码文本前缀。 */
export const QR_PREFIX = 'PMS1:';

// ===== 主密码 =====
/** 主密码最小长度（U6 裁定：< 8 位拦截）。 */
export const MIN_MASTER_PASSWORD_LEN = 8;

// ===== 生成器 =====
export const GENERATOR_MIN_LEN = 8;
export const GENERATOR_MAX_LEN = 64;
export const GENERATOR_DEFAULT_LEN = 16;

// ===== 体积 / 容量上限 =====
/** 单附件上限 20 MB。 */
export const MAX_ATTACHMENT_BYTES = 20 * 1024 * 1024;
/** 库总容量上限 200 MB。 */
export const MAX_VAULT_BYTES = 200 * 1024 * 1024;
/** 安全笔记正文上限 256 KiB。 */
export const MAX_NOTE_BYTES = 256 * 1024;

// ===== 剪贴板 / 自动锁 =====
/** 剪贴板自动清空秒数。 */
export const CLIPBOARD_TTL_SECS = 30;
/** 自动锁默认分钟数。 */
export const AUTO_LOCK_MINUTES = 5;
/** 自动锁合法档位。 */
export const AUTO_LOCK_OPTIONS: readonly number[] = [1, 5, 15];

// ===== 同步 =====
/** 同步时间窗（秒，正负）。 */
export const SYNC_TIME_WINDOW_SECS = 120;

// ===== 回收站 =====
/** 回收站保留天数。 */
export const TRASH_RETENTION_DAYS = 30;
/** 每日毫秒数。 */
export const MS_PER_DAY = 24 * 60 * 60 * 1000;

// ===== 体检 =====
/** 弱口令长度阈值。 */
export const HEALTH_WEAK_MAX_LEN = 8;
/** 超期未改天数阈值。 */
export const HEALTH_STALE_DAYS = 180;

// ===== TOTP =====
export const TOTP_PERIOD_SECS = 30;
export const TOTP_DIGITS = 6;

// ===== 单位换算 =====
export const KB = 1024;
export const MB = 1024 * 1024;

/** 人类可读体积（1 位小数）。 */
export function formatBytes(bytes: number): string {
  if (bytes < KB) {
    return `${bytes} B`;
  }
  if (bytes < MB) {
    return `${(bytes / KB).toFixed(1)} KB`;
  }
  return `${(bytes / MB).toFixed(1)} MB`;
}

/** 本地时间格式化（不引入 dayjs/moment）。 */
export function formatDateTime(ms: number, locale: string): string {
  if (!ms) {
    return '';
  }
  const d = new Date(ms);
  const two = (n: number): string => (n < 10 ? `0${n}` : `${n}`);
  const y = d.getFullYear();
  const mo = two(d.getMonth() + 1);
  const da = two(d.getDate());
  const h = two(d.getHours());
  const mi = two(d.getMinutes());
  // D-106（豆包审查 5.2-9）：泰文用佛历（佛历年 = 公历年 + 543）+ DD/MM/YYYY，
  // 与泰国日常读法一致；其余非中文沿用 MM/DD/YYYY 西元。
  if (locale === 'th') {
    return `${da}/${mo}/${y + 543} ${h}:${mi}`;
  }
  return locale === 'zh-CN' ? `${y}-${mo}-${da} ${h}:${mi}` : `${mo}/${da}/${y} ${h}:${mi}`;
}
