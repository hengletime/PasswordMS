// TOTP 展示（PRD F-11）。
//
// 验证码由 Rust 侧 `totp_now`（HMAC-SHA1）计算；前端仅负责展示与周期倒计时，
// 不重复实现密码学。

import { TOTP_PERIOD_SECS } from './constants';
import { api } from './ipc';
import type { TotpDto } from '../types';

/** 取当前验证码。 */
export function currentTotp(secret: string): Promise<TotpDto> {
  return api.tools.totpNow(secret);
}

/**
 * 距下一周期的剩余秒数（与 Rust `totp::now_at` 一致：整除时返回完整周期）。
 */
export function secondsRemaining(nowMs: number, period: number = TOTP_PERIOD_SECS): number {
  const secs = Math.floor(nowMs / 1000);
  return period - (secs % period);
}

/** 归一化 Base32 密钥（去空白、去填充、转大写），用于本地校验。 */
export function normalizeBase32(secret: string): string {
  return secret.replace(/[\s=]/g, '').toUpperCase();
}

/** 判定是否为（粗略）合法的 Base32 密钥。 */
export function isValidBase32(secret: string): boolean {
  const normalized = normalizeBase32(secret);
  return normalized.length > 0 && /^[A-Z2-7]+$/.test(normalized);
}

/** 格式化为展示用分组（每 3 位一组，如 `123 456`）。 */
export function formatCode(code: string): string {
  const groups: string[] = [];
  for (let i = 0; i < code.length; i += 3) {
    groups.push(code.slice(i, i + 3));
  }
  return groups.join(' ');
}
