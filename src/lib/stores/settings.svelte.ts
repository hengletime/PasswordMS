// 设置状态（Svelte 5 runes）。

import type { VaultSettings } from '../../types';
import { toAppError } from '../errors';
import { api } from '../ipc';
import { ui } from './ui.svelte';

/** 缺省设置（与 Rust `VaultSettings::default()` 对齐）。 */
export function defaultSettings(): VaultSettings {
  return {
    generator: {
      length: 16,
      uppercase: true,
      lowercase: true,
      digits: true,
      symbols: true,
      exclude_ambiguous: true,
      // D-52：与 Rust `GeneratorOptions::default()`（`Some(CHARSET_SYMBOLS)`）对齐，
      // 保证序列化出去的设置自描述，前端初始符号选择也能取到非空集。
      symbol_set: '+-_.,:',
    },
    health: { weak_max_len: 8, stale_days: 180 },
    clipboard_clear_secs: 30,
    auto_sync: false,
    sync_on_start: false,
  };
}

/** 设置 store。 */
class SettingsStore {
  /** 当前设置。 */
  value = $state<VaultSettings>(defaultSettings());
  /** 是否加载中。 */
  busy = $state(false);
  /**
   * 静默保存进行中（D-108 豆包审查 3.5-8）：原实现复用 `busy`——生成器滑块
   * 触发的 `saveQuiet` 会让绑定 `disabled={busy}` 的按钮偶发置灰。独立标志互不干扰。
   */
  quietBusy = $state(false);

  /** 加载。`silent = true` 时失败不弹 toast 而是上抛（D-108 3.5-10 合并提示用）。 */
  async load(silent = false): Promise<void> {
    this.busy = true;
    try {
      this.value = await api.settings.get();
    } catch (err) {
      if (silent) {
        throw err;
      }
      ui.toast('error', toAppError(err).messageKey);
    } finally {
      this.busy = false;
    }
  }

  /** 保存。 */
  async save(next: VaultSettings): Promise<boolean> {
    this.busy = true;
    try {
      this.value = await api.settings.update(next);
      ui.toast('success', 'settings.saved');
      return true;
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
      return false;
    } finally {
      this.busy = false;
    }
  }

  /**
   * 静默保存（D-52）：与 `save` 同逻辑但**不弹任何 toast**——供生成器参数变更后即时持久化，
   * 避免拖滑块 / 点符号芯片时被连续的成功/失败提示刷屏。失败不提示，下一次变更会再次尝试。
   */
  async saveQuiet(next: VaultSettings): Promise<boolean> {
    this.quietBusy = true;
    try {
      this.value = await api.settings.update(next);
      return true;
    } catch {
      return false;
    } finally {
      this.quietBusy = false;
    }
  }

  /** 重置为默认（锁定时调用）。 */
  reset(): void {
    this.value = defaultSettings();
  }
}

/** 单例。 */
export const settings = new SettingsStore();
