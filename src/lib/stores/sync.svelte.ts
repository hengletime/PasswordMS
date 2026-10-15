// 同步状态（Svelte 5 runes）：配对、设备、状态与冲突仲裁。

import type { ConflictChoice, ConflictDto, DeviceDto, PairingDto, SyncStatusDto } from '../../types';
import { listen } from '@tauri-apps/api/event';
import { DEFAULT_CONFLICT_CHOICE } from '../conflict';
import { ErrorCode, toAppError } from '../errors';
import { api } from '../ipc';
import { vault } from './vault.svelte';
import { ui } from './ui.svelte';

/** 同步 store。 */
class SyncStore {
  /** 当前 Host 配对信息（null = 未在配对）。 */
  pairing = $state<PairingDto | null>(null);
  /** 配对会话已被后端终止（D-44）：提示用户重新生成二维码。 */
  pairEnded = $state<boolean>(false);
  /** 已配对设备。 */
  devices = $state<DeviceDto[]>([]);
  /** 同步状态。 */
  status = $state<SyncStatusDto>({ last_sync_at: null, pending_changes: 0, device_count: 0 });
  /** 未仲裁冲突。 */
  conflicts = $state<ConflictDto[]>([]);
  /** 每个冲突的当前选择（默认 null，**禁止自动选择**）。 */
  choices = $state<Record<string, ConflictChoice | null>>({});
  /** 是否进行中。 */
  busy = $state(false);

  /** 发起配对（生成二维码）。 */
  async startPairing(): Promise<void> {
    this.busy = true;
    this.pairEnded = false;
    try {
      this.pairing = await api.sync.startPairing();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    } finally {
      this.busy = false;
    }
  }

  /** 取消配对。 */
  async cancelPairing(): Promise<void> {
    try {
      await api.sync.cancelPairing();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    } finally {
      this.pairing = null;
    }
  }

  /** 加入配对（扫码文本或手动输入）。 */
  async join(qrText: string): Promise<boolean> {
    this.busy = true;
    try {
      const result = await api.sync.join(qrText);
      ui.toast('success', 'sync.paired');
      if (result.applied > 0) {
        ui.toast('info', 'sync.result_applied', { n: result.applied });
      }
      if (result.conflicts.length > 0) {
        ui.toast('info', 'sync.result_conflicts', { n: result.conflicts.length });
      }
      // D-47：同步进来的数据立即上屏（否则要切页才刷新）。
      await vault.load();
      await this.loadDevices();
      await this.loadConflicts();
      return true;
    } catch (err) {
      const appErr = toAppError(err);
      if (appErr.code === ErrorCode.ERR_VK_ADOPT_REQUIRES_EMPTY) {
        ui.toast('error', 'sync.adopt_requires_empty', { n: appErr.recordCount ?? 0 });
      } else {
        ui.toast('error', appErr.messageKey);
      }
      return false;
    } finally {
      this.busy = false;
    }
  }

  /** 加载设备列表。 */
  async loadDevices(): Promise<void> {
    try {
      this.devices = await api.sync.listDevices();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 加载同步状态。 */
  async loadStatus(): Promise<void> {
    try {
      this.status = await api.sync.status();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 加载冲突。 */
  async loadConflicts(): Promise<void> {
    try {
      this.conflicts = await api.sync.listConflicts();
      // 为新出现的冲突初始化未选择状态。
      const next: Record<string, ConflictChoice | null> = {};
      for (const c of this.conflicts) {
        next[c.conflict_id] = this.choices[c.conflict_id] ?? DEFAULT_CONFLICT_CHOICE;
      }
      this.choices = next;
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 解除配对。 */
  async unpair(deviceId: string): Promise<void> {
    try {
      await api.sync.unpair(deviceId);
      await this.loadDevices();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 立即同步（直连重同步）。 */
  async syncNow(deviceId: string): Promise<void> {
    this.busy = true;
    try {
      const result = await api.sync.now(deviceId);
      ui.toast('success', 'sync.result_applied', { n: result.applied });
      if (result.conflicts.length > 0) {
        ui.toast('info', 'sync.result_conflicts', { n: result.conflicts.length });
      }
      // D-47：同步进来的数据立即上屏。
      await vault.load();
      await this.loadStatus();
      await this.loadConflicts();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    } finally {
      this.busy = false;
    }
  }

  /** 读取某冲突的当前选择。 */
  choiceOf(conflictId: string): ConflictChoice | null {
    return this.choices[conflictId] ?? DEFAULT_CONFLICT_CHOICE;
  }

  /** 设置某冲突的选择。 */
  setChoice(conflictId: string, choice: ConflictChoice): void {
    this.choices = { ...this.choices, [conflictId]: choice };
  }

  /** 提交仲裁（未选择则不提交，返回 false）。 */
  async resolve(conflictId: string): Promise<boolean> {
    const choice = this.choiceOf(conflictId);
    if (choice === null) {
      ui.toast('error', 'conflict.choose');
      return false;
    }
    try {
      await api.sync.resolveConflict(conflictId, choice);
      await this.loadConflicts();
      return true;
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
      return false;
    }
  }

  /** 重置（锁定时调用）。 */
  reset(): void {
    this.pairing = null;
    this.pairEnded = false;
    this.devices = [];
    this.status = { last_sync_at: null, pending_changes: 0, device_count: 0 };
    this.conflicts = [];
    this.choices = {};
  }
}

/** 单例。 */
export const sync = new SyncStore();

// D-44：配对会话结束后端主动通知（worker 在 completed/aborted 时清理 pairing 并 emit）。
// 前端收回二维码，aborted 时在配对页提示「请重新生成二维码」。
// 仅在 Tauri WebView 环境注册监听（Node 测试环境无 window/@tauri-apps API）。
if (typeof window !== 'undefined') {
  void listen('pairing://ended', (ev) => {
    sync.pairing = null;
    if (ev.payload === 'aborted') {
      sync.pairEnded = true;
    }
  });
}
