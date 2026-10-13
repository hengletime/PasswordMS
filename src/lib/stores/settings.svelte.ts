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

  /** 加载。 */
  async load(): Promise<void> {
    this.busy = true;
    try {
      this.value = await api.settings.get();
    } catch (err) {
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

  /** 重置为默认（锁定时调用）。 */
  reset(): void {
    this.value = defaultSettings();
  }
}

/** 单例。 */
export const settings = new SettingsStore();
