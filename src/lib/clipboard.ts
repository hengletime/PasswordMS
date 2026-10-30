// 剪贴板：复制 + 30 秒自动清空（PRD F-06）。
//
// 倒计时逻辑抽为纯函数（[`startCountdown`]），便于单测；Rust 侧 `copy_secret` 只负责写入，
// 不常驻任何明文。

import { CLIPBOARD_TTL_SECS } from './constants';
import { api } from './ipc';
import { ui } from './stores/ui.svelte';

/** 可取消的倒计时句柄。 */
export interface CountdownTimer {
  /** 取消倒计时（不再触发 tick / done）。 */
  cancel(): void;
}

/**
 * 启动秒级倒计时。
 *
 * - 立即以当前剩余秒数触发一次 `onTick`；
 * - 每秒递减并触发 `onTick`；
 * - 归零时触发 `onDone` 并自动停止；
 * - `ttlSecs <= 0` 时立即触发 `onDone`。
 */
export function startCountdown(
  ttlSecs: number,
  onTick: (remaining: number) => void,
  onDone: () => void,
): CountdownTimer {
  let remaining = Math.max(0, Math.floor(ttlSecs));
  onTick(remaining);
  if (remaining === 0) {
    onDone();
    return { cancel: (): void => undefined };
  }
  const handle = setInterval((): void => {
    remaining -= 1;
    if (remaining <= 0) {
      clearInterval(handle);
      onTick(0);
      onDone();
    } else {
      onTick(remaining);
    }
  }, 1000);
  return {
    cancel: (): void => clearInterval(handle),
  };
}

/**
 * 复制敏感文本并启动自动清空（默认 30 秒）。
 *
 * 清空策略（A-03）：到期后调用 Rust 侧 `clear_secret_if_own`，仅当剪贴板内容**仍为
 * 本应用写入的内容**时才清空；若用户期间已复制了别的内容，则**不覆盖**。比对在 Rust
 * 侧完成，避免把剪贴板明文回传 JS。
 *
 * 【D-135】必须带上本次写入的**代际号** `gen`：清空是「按代」的。没有它，复制密码 A
 * 后 10 秒又复制用户名 B，A 的定时器在 30 秒到期时会把 B（当时剪贴板里的内容）清成
 * 空串——用户看到的就是「明明提示已复制，粘出来是空的」。
 */
export async function copySecretWithAutoClear(
  text: string,
  ttlSecs: number = CLIPBOARD_TTL_SECS,
  onTick?: (remaining: number) => void,
): Promise<CountdownTimer> {
  const gen = await api.system.copySecret(text, ttlSecs);
  return startCountdown(
    ttlSecs,
    (remaining: number): void => {
      onTick?.(remaining);
    },
    (): void => {
      // D-106（豆包审查 3.5-3）：清空 IPC 失败不得静默——密码可能仍留在剪贴板，
      // 用户必须被告知手动清理。
      api.system.clearSecretIfOwn(gen).catch((): void => {
        ui.toast('error', 'err.clipboard_clear_failed');
      });
    },
  );
}
