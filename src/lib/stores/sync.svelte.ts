// 同步状态（Svelte 5 runes）：配对、设备、状态与冲突仲裁。

import type { ConflictChoice, ConflictDto, DeviceDto, PairingDto, SyncStatusDto } from '../../types';
import { listen } from '@tauri-apps/api/event';
import { DEFAULT_CONFLICT_CHOICE } from '../conflict';
import { ErrorCode, toAppError } from '../errors';
import { api } from '../ipc';
import { nextPairingState, PAIRING_ENDED_EVENT } from '../pairing-state';
import { PEER_SYNC_APPLIED_EVENT, shouldReloadAfterPeerSync } from '../sync-applied';
import { vault } from './vault.svelte';
import { ui } from './ui.svelte';

/** 同步 store。 */
class SyncStore {
  /** 当前 Host 配对信息（null = 未在配对）。 */
  pairing = $state<PairingDto | null>(null);
  /** 配对会话已被后端终止（D-44）：提示用户重新生成二维码。 */
  pairEnded = $state<boolean>(false);
  /**
   * D-75：配对**成功**（后端 `pairing://ended` 载荷为 `completed`），等待人工核对会话码。
   *
   * 为什么需要这个状态：D-69 让两屏显示同一次会话的会话码供核对，但「会话结束」事件
   * 一到就把 `pairing` 清空了 —— 而手机端要核对的那块**恰好在这一瞬间才出现**，
   * 电脑屏的码已经消失。核对动作的前提被事件本身删掉，功能从上线起不可能被用上。
   *
   * 语义边界：`pairCompleted === true` 时 **`pairing` 必须仍在**（会话码在里面），
   * 由用户在核对完毕后点「完成」→ `endPairing()` 才清屏。这两者在
   * `src/lib/pairing-state.ts::nextPairingState` 里成组返回，不许分开改。
   */
  pairCompleted = $state<boolean>(false);
  /** 已配对设备。 */
  devices = $state<DeviceDto[]>([]);
  /** 同步状态。 */
  status = $state<SyncStatusDto>({ last_sync_at: null, pending_changes: 0, device_count: 0 });
  /** 未仲裁冲突。 */
  conflicts = $state<ConflictDto[]>([]);
  /** 每个冲突的当前选择（默认 null，**禁止自动选择**）。 */
  choices = $state<Record<string, ConflictChoice | null>>({});
  /**
   * 是否进行中。
   *
   * **D-66 语义收窄**：仅表示「**配对流程**进行中」（`PairScan.svelte` / `PairShow.svelte` 用）。
   * 「立即同步」（`syncNow`）**不再**碰这个标志——原实现复用它当全局忙碌态，
   * 导致设备列表里点一台、所有设备行同时显示「正在同步…」（详见 `syncingDeviceId`）。
   */
  busy = $state(false);
  /**
   * D-66：正在「立即同步」的设备 id（null = 空闲）。
   *
   * 为什么必须按设备隔离：同步按钮是**每台设备行一个**，而原 `busy` 是 store 级**单一**
   * 布尔，一台在同步则 `{#each}` 里所有行都命中 `{#if sync.busy}` → 两台手机同时转圈，
   * 主人分不清在等哪一台。占用态收敛到「哪一台」这一个字符串后，行为判定交给纯函数
   * `syncButtonState()`（`src/lib/device-label.ts`，有单测守门）。
   */
  syncingDeviceId = $state<string | null>(null);
  /**
   * D-69：上一次 join 成功时拿到的**会话码**，供 `PairScan` 与电脑屏逐字核对。
   *
   * 为什么从「指纹」改成「会话码」（主人真机实测得出的结论）：原实现里电脑屏显示的是
   * `identity.fingerprint()`（**长期身份密钥**的指纹），手机屏显示的是握手时那把
   * **每次配对重新生成的临时 X25519 公钥**的指纹 —— 两个不同的量，两屏必然不等。
   * 用户照着核对，每次都会看到「不一致」，要么误判被中间人劫持，要么从此不再核对。
   *
   * 会话码 = `sha256(本次临时公钥)` 前 8 字节的小写 hex，两屏算的是**同一把临时公钥**，
   * 构造上必然相等，这才是有意义的核对量。方向仍然固定：手机屏显示的是
   * 「我要连的那台电脑」的会话码，不是手机自己的。
   *
   * 代价必须说清：会话码只证明「本次握手两端看到同一把临时公钥」（防本次中间人），
   * **不证明**「对端是上次那台电脑」—— 后者要靠 `DeviceDto.fingerprint`（长期指纹）。
   */
  lastPairSessionCode = $state<string | null>(null);
  /** D-68：上一次 join 成功时的对端设备名（与会话码一起显示，便于用户确认是哪台）。 */
  lastPairDeviceName = $state<string>('');

  /** 发起配对（生成二维码）。 */
  async startPairing(): Promise<void> {
    this.busy = true;
    this.pairEnded = false;
    // D-75：新一轮配对必须清掉上一轮的「成功待核对」态，否则新二维码旁边还挂着旧的成功提示。
    this.pairCompleted = false;
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
      // D-75：主动取消也算「这一轮结束了」，成功待核对态一并复位。
      this.pairCompleted = false;
    }
  }

  /**
   * D-75：结束配对成功态（「完成」按钮）。
   *
   * 只清内存状态、**不打 IPC** —— 后端在发 `completed` 之前已经
   * `*state.pairing = None` 并 `break` 了 worker，会话本就不存在了。
   */
  endPairing(): void {
    this.pairing = null;
    this.pairCompleted = false;
    this.pairEnded = false;
  }

  /** 加入配对（扫码文本或手动输入）。 */
  async join(qrText: string): Promise<boolean> {
    this.busy = true;
    // D-69：先清掉上一次的核对结果，避免把「上一台电脑」的会话码显示成「这一台」的。
    this.lastPairSessionCode = null;
    this.lastPairDeviceName = '';
    try {
      const result = await api.sync.join(qrText);
      // D-69：留 join 结果里的**本次会话码**（`PairResultDto.session_code`，与电脑屏同值）。
      // 失败路径不写（`return false` 前不碰），所以「上一台的码」不会残留到这一台。
      // 只写状态、不 toast —— 核对是用户动作，不是通知。
      this.lastPairSessionCode = result.session_code ?? null;
      this.lastPairDeviceName = result.device_name;
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

  /**
   * D-66：重命名已配对设备（本地昵称，不参与同步）。
   *
   * 语义：昵称只存在**本机**的设备表里（不回写对端、不进同步载荷），仅用于设备列表可读性。
   * 成功后 `loadDevices()` 重新拉取（避免本地拼装脏数据），失败按错误码统一 toast。
   *
   * @param deviceId 目标设备 id（`DeviceDto.device_id`）。
   * @param name 新昵称（前后空白会被裁掉；裁完为空视为无效，不发 IPC）。
   * @returns 是否保存成功（供 UI 决定是否退出编辑态）。
   */
  async renameDevice(deviceId: string, name: string): Promise<boolean> {
    const trimmed = name.trim();
    // 空名无意义（列表会变成一行空白），前端直接拦下、不打 IPC。
    if (trimmed.length === 0) {
      return false;
    }
    try {
      await api.sync.renameDevice(deviceId, trimmed);
      await this.loadDevices();
      ui.toast('success', 'sync.renamed');
      return true;
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
      return false;
    }
  }

  /** 立即同步（直连重同步）。 */
  async syncNow(deviceId: string): Promise<void> {
    // D-66：防重入——一次只允许一台设备同步；期间其它设备行的按钮由
    // `syncButtonState()` 置灰（不转圈），不靠抛错打断用户。
    if (this.syncingDeviceId !== null) {
      return;
    }
    this.syncingDeviceId = deviceId;
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
      this.syncingDeviceId = null;
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
    // D-75：锁定后重进同步页应从「生成配对二维码」开始，不留上一轮的成功待核对态。
    this.pairCompleted = false;
    this.devices = [];
    this.status = { last_sync_at: null, pending_changes: 0, device_count: 0 };
    this.conflicts = [];
    this.choices = {};
    // D-66：锁定会中断所有在飞的同步，忙碌态必须一并复位（否则解锁后按钮永久置灰）。
    this.syncingDeviceId = null;
    // D-69：上一次配对的会话码不该跨解锁残留（下次解锁看到的应是空白，而非旧数据）。
    this.lastPairSessionCode = null;
    this.lastPairDeviceName = '';
  }
}

/** 单例。 */
export const sync = new SyncStore();

// D-44：配对会话结束后端主动通知（worker 在 completed/aborted 时清理 pairing 并 emit）。
// 前端收回二维码，aborted 时在配对页提示「请重新生成二维码」。
// D-75：**必须按载荷分流**（判定收在纯函数 `nextPairingState` 里，有单测）：
//   `completed` → **保留** `pairing`（会话码还在里面，手机端此刻才弹出核对提示），置成功态；
//   `aborted`   → 清屏 + 提示重新生成。
// ⚠️ 不许改回「无条件 `sync.pairing = null`」—— 那正是 D-75 的成因：会话结束事件
//    把「核对会话码」这个动作的前提一起删掉了，功能看着有、实际用不上。
// 仅在 Tauri WebView 环境注册监听（Node 测试环境无 window/@tauri-apps API）。
if (typeof window !== 'undefined') {
  void listen(PAIRING_ENDED_EVENT, (ev) => {
    const next = nextPairingState(ev.payload);
    if (next.clearPairing) {
      sync.pairing = null;
    }
    sync.pairCompleted = next.completed;
    sync.pairEnded = next.ended;
  });

  // v1.2.4 根因 A：**对端（手机）发起的重同步**把数据写进了本机库，但本机前端 store
  // 还停在同步前 → 主人实测「手机删了附件、电脑上附件还在」。后端（state.rs 的
  // spawn_resync_worker）在被动同步完成后 emit `sync://applied`，这里据此 reload。
  // 判定收敛在纯函数里（`src/lib/sync-applied.ts`，有单测）。
  void listen(PEER_SYNC_APPLIED_EVENT, (ev) => {
    if (!shouldReloadAfterPeerSync(ev.payload)) {
      return;
    }
    // 条目/分组 + 设备上次同步时间 + 冲突箱：三者都可能被对端这次同步改变。
    void vault.load();
    void sync.loadDevices();
    void sync.loadConflicts();
  });
}
