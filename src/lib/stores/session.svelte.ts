// 会话状态（Svelte 5 runes）：锁定 / 解锁 / 创建。

import type { StatusDto } from '../../types';
import { ErrorCode, toAppError } from '../errors';
import { api } from '../ipc';
import { ui } from './ui.svelte';

/** 会话 store。 */
class SessionStore {
  /** 当前状态。 */
  status = $state<StatusDto>({ has_vault: false, locked: true });
  /** 是否有进行中的操作。 */
  busy = $state(false);
  /** 解锁失败统一提示（不区分原因，A-05）。 */
  unlockError = $state(false);

  /** 刷新状态。 */
  async refresh(): Promise<void> {
    try {
      this.status = await api.system.getStatus();
    } catch {
      // 启动阶段的状态查询失败不应阻塞界面。
      this.status = { has_vault: false, locked: true };
    }
  }

  /** 创建保险库。 */
  async create(masterPassword: string, strengthConfirmed: boolean): Promise<boolean> {
    this.busy = true;
    try {
      this.status = await api.vault.create(masterPassword, strengthConfirmed);
      return true;
    } catch (err) {
      const appErr = toAppError(err);
      ui.toast('error', appErr.messageKey);
      return false;
    } finally {
      this.busy = false;
    }
  }

  /**
   * 解锁。
   *
   * 失败时统一展示同一句提示，**不区分**「密码错误」与「数据损坏」（A-05）。
   */
  async unlock(masterPassword: string): Promise<boolean> {
    this.busy = true;
    this.unlockError = false;
    try {
      this.status = await api.vault.unlock(masterPassword);
      return true;
    } catch (err) {
      const appErr = toAppError(err);
      // 除「无保险库」外，一律使用统一文案，避免侧信道。
      if (appErr.code === ErrorCode.ERR_NO_VAULT) {
        ui.toast('error', appErr.messageKey);
      } else {
        this.unlockError = true;
      }
      return false;
    } finally {
      this.busy = false;
    }
  }

  /** 锁定。 */
  async lock(): Promise<void> {
    this.busy = true;
    try {
      await api.vault.lock();
      this.status = { has_vault: true, locked: true };
      this.unlockError = false;
    } catch (err) {
      const appErr = toAppError(err);
      ui.toast('error', appErr.messageKey);
    } finally {
      this.busy = false;
    }
  }
}

/** 单例。 */
export const session = new SessionStore();
