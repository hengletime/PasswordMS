// 设置页数字输入的钳制（D-169）。
//
// 抽成纯函数的理由只有一个：**能喂数据断言边界**。这段逻辑最容易错的地方不是范围本身，
// 而是三种「看起来像数字、其实不是」的输入——
//   ① 空串：`Number('')` 是 **0**，不是 NaN，直接 Math.floor 会得到 0（剪贴板 0 秒 = 复制即清空）；
//   ② 小数 / 文本：要回落到默认值，而不是写入 NaN；
//   ③ 越界：要夹紧，否则「界面显示 200、库里存 120」的错位没人发现。
// 写在组件模板的 onchange 里就只能靠字符串切片猜，测不出这三条。

import {
  CLIPBOARD_TTL_MAX,
  CLIPBOARD_TTL_MIN,
  CLIPBOARD_TTL_SECS,
  HEALTH_STALE_DAYS,
  HEALTH_STALE_MAX,
  HEALTH_STALE_MIN,
  HEALTH_WEAK_MAX,
  HEALTH_WEAK_MAX_LEN,
  HEALTH_WEAK_MIN,
} from './constants';

/**
 * 把输入框的值夹进 `[min, max]`；空串 / 非数字回落到 `fallback`。
 *
 * HTML 的 `min`/`max` 只约束步进按钮与表单校验，**不阻止键入**。
 */
export function clampInt(raw: unknown, min: number, max: number, fallback: number): number {
  const text = String(raw ?? '').trim();
  if (text.length === 0) {
    return fallback;
  }
  const n = Math.floor(Number(text));
  if (!Number.isFinite(n)) {
    return fallback;
  }
  return Math.min(max, Math.max(min, n));
}

/** 剪贴板 TTL（D-106 豆包审查 3.5-4：0/负值会导致复制即清空，极大值让密码长期残留）。 */
export function clampClipboardTtl(raw: unknown): number {
  return clampInt(raw, CLIPBOARD_TTL_MIN, CLIPBOARD_TTL_MAX, CLIPBOARD_TTL_SECS);
}

/** 体检两项阈值。 */
export function clampHealthSettings(health: {
  weak_max_len: number;
  stale_days: number;
}): { weak_max_len: number; stale_days: number } {
  return {
    weak_max_len: clampInt(health.weak_max_len, HEALTH_WEAK_MIN, HEALTH_WEAK_MAX, HEALTH_WEAK_MAX_LEN),
    stale_days: clampInt(health.stale_days, HEALTH_STALE_MIN, HEALTH_STALE_MAX, HEALTH_STALE_DAYS),
  };
}
