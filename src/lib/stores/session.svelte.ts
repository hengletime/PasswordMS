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
  /** D-106（豆包审查 3.5-5）：启动状态查询失败——不得伪装成「无库」误导向创建向导。 */
  statusError = $state(false);

  /** 刷新状态。 */
  async refresh(): Promise<void> {
    try {
      this.status = await api.system.getStatus();
      this.statusError = false;
    } catch {
      // 启动瞬态 IPC 故障：保持上一次状态，置错误位由路由层显式处理——
      // 归一为 has_vault:false 会把用户误导向「创建保险库」向导（有写库路径）。
      this.statusError = true;
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

  /**
   * 锁定。返回是否**真正**锁定成功（D-91 审查 O3：失败时调用方不得 reset
   * store / 切锁屏——否则界面谎报已锁、后端会话仍解锁、明文仍在内存）。
   */
  async lock(): Promise<boolean> {
    this.busy = true;
    try {
      await api.vault.lock();
      this.status = { has_vault: true, locked: true };
      this.unlockError = false;
      return true;
    } catch (err) {
      const appErr = toAppError(err);
      ui.toast('error', appErr.messageKey);
      return false;
    } finally {
      this.busy = false;
    }
  }
}

/** 单例。 */
export const session = new SessionStore();
