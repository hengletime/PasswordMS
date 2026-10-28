// 会话状态（Svelte 5 runes）：锁定 / 解锁 / 创建。

import type { StatusDto } from '../../types';
import type { BiometricStatusDto } from '../../types';
import { ErrorCode, toAppError } from '../errors';
import { api } from '../ipc';
import { bioPlatformOf, localizeBioMessageKey } from '../bio-platform';
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
  /**
   * 快速解锁状态（`docs/08` P2）。`null` = 尚未查询或查不到。
   *
   * 锁定态也可查询（后端只读容器头部 + `biometric.json`），解锁页据此决定显示什么。
   * **查不到一律当不可用**——不能因为一个旁路文件读失败就让用户进不了主密码输入框。
   */
  biometric = $state<BiometricStatusDto | null>(null);

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

  /** 刷新快速解锁状态（解锁页显示前、设置页进入时、开启/关闭后各调一次）。 */
  async refreshBiometric(): Promise<void> {
    try {
      this.biometric = await api.vault.biometricStatus();
    } catch {
      // 读不到就当没有：**不弹提示**。解锁页的可用性不该依赖这个旁路文件。
      this.biometric = null;
    }
  }

  /**
   * 用快速解锁打开保险库。
   *
   * 失败分类**必须分开处理**（`docs/08` §10）：
   * - 取消 → 什么都不弹（用户自己关的，弹一句等于责怪他）；
   * - `ERR_VALIDATION`（`biometric_busy`）→ 静默，用户本来就在等前一次；
   * - 已失效 → 弹具体原因 + **重查状态**（按钮要从「可用」变回「未开启」）；
   * - 需要主密码 → 引导语，且刻意**不**设置 `unlockError`（那是密码错的黑话）。
   */
  async unlockWithBiometric(): Promise<boolean> {
    this.busy = true;
    this.unlockError = false;
    try {
      this.status = await api.vault.biometricUnlock();
      return true;
    } catch (err) {
      const appErr = toAppError(err);
      // D-120：失效原因里的平台名词要跟着平台走（安卓不认「Windows Hello」）。
      const messageKey = localizeBioMessageKey(appErr.messageKey, bioPlatformOf(this.biometric));
      switch (appErr.code) {
        case ErrorCode.ERR_BIOMETRIC_CANCELED:
        case ErrorCode.ERR_VALIDATION:
          break;
        case ErrorCode.ERR_BIOMETRIC_INVALIDATED:
          ui.toast('error', messageKey);
          await this.refreshBiometric();
          break;
        default:
          ui.toast('error', messageKey);
          break;
      }
      return false;
    } finally {
      this.busy = false;
    }
  }
}

/** 单例。 */
export const session = new SessionStore();
