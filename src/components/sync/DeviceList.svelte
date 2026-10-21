<script lang="ts">
  // 已配对设备列表（O-07：解除配对 / D-66：重命名 + 配对时间 / D-67：真指纹）。
  // 设计规范 §5.4：设备行 = 图标 + 名称（可重命名，行内编辑）+ 指纹 + 配对时间 + 上次同步
  // + 铅笔改名按钮 + 解绑 ghost 按钮。
  //
  // 行内编辑的硬约束（§5.6）：
  // - 一次只允许**一行**处于编辑态（`editingId` 单一状态源，不另设布尔）；
  // - Enter 保存 / Esc 取消；空名（trim 后为空）或超长（>64 **字节**）时保存按钮 disabled；
  // - 输入框必须是 `.pms-input`，不得自写边框/底色/字号；
  // - 成功后由 store 的 `loadDevices()` 刷新数据 → 这里只负责退出编辑态。
  //
  // D-67 / D-69：指纹 = `DeviceDto.fingerprint`（sha256(对端公钥) 前 8 字节，16 位小写 hex），
  // **不是** `device_id`。`device_id` 只挂在 `title` 上供悬停排查。
  // D-69：指纹为空（老配对记录 / 本机侧拿不到对端长期公钥，见 backlog D-70）时**整行不渲染**——
  // 绝不回退成 device_id，那正是 D-67 判死的「拿 UUID 冒充指纹」。
  import Icon from '../common/Icon.svelte';
  import { formatDateTime } from '../../lib/constants';
  import { fingerprintText, hasSessionCode } from '../../lib/device-label';
  import { sync } from '../../lib/stores/sync.svelte';
  import { ui } from '../../lib/stores/ui.svelte';

  /**
   * 昵称长度上限，与后端 `Repository::rename_paired_device` 的
   * `MAX_DEVICE_NAME_BYTES = 64` 对齐。
   *
   * ⚠️ 是**字节**不是字符：22 个汉字 = 66 字节 → 超限；21 个汉字 = 63 字节 → 允许。
   * 用 `maxlength` 只能限 UTF-16 码元（汉字 1 个码元），单独用它会放过 64 汉字的名称，
   * 让后端回 `Validation("device_name_too_long")` → 用户只看到笼统的「输入无效。」。
   *
   * ⚠️ 改这个数字必须**四处同改**，漏一处就是「界面在骗用户」或「测试假绿」：
   * ① 此处；② core 的 `MAX_DEVICE_NAME_BYTES`；③ core 测试
   * `rename_paired_device.rs::rename_rejects_over_64_bytes` 的边界构造（65 字节必拒、
   * 22 汉字必拒、21 汉字必过）；④ i18n `sync.rename_too_long` 的中英两条文案，
   * 其中「约 21 个汉字」= `floor(64/3)`，改 32 就变 `floor(32/3) = 10`。
   * 另：Emoji 是 4 字节/个（64 字节 = 16 个），所以文案**只说汉字，不要顺带说「或 N 个字符」**。
   */
  const MAX_NAME_BYTES = 64;

  /** UTF-8 字节长度（与后端 `str::len()` 同口径）。 */
  function utf8Bytes(text: string): number {
    return new TextEncoder().encode(text).length;
  }

  /** 当前处于编辑态的设备 id（null = 无；单一状态源，天然保证「同时只有一行编辑」）。 */
  let editingId = $state<string | null>(null);
  /** 编辑中的草稿名称（进入编辑态时用当前名初始化）。 */
  let draft = $state<string>('');
  /** 保存请求在飞（期间保存/取消按钮禁用，防重复提交）。 */
  let saving = $state<boolean>(false);

  /** 进入编辑态：记录目标设备并把当前名灌进草稿。 */
  function startEdit(deviceId: string, currentName: string): void {
    editingId = deviceId;
    draft = currentName;
  }

  /** 退出编辑态（取消 / 保存成功）。 */
  function endEdit(): void {
    editingId = null;
    draft = '';
  }

  /** 保存新昵称；空名 / 超长 / 保存中一律忽略（不发 IPC）。 */
  async function saveEdit(deviceId: string): Promise<void> {
    const name = draft.trim();
    if (name.length === 0 || utf8Bytes(name) > MAX_NAME_BYTES || saving) {
      return;
    }
    saving = true;
    try {
      const ok = await sync.renameDevice(deviceId, name);
      if (ok) {
        endEdit();
      }
    } finally {
      saving = false;
    }
  }

  /** 键盘交互：Enter 保存、Esc 取消（其余按键交给输入框本身）。 */
  function onKeydown(event: KeyboardEvent, deviceId: string): void {
    if (event.key === 'Enter') {
      event.preventDefault();
      void saveEdit(deviceId);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      endEdit();
    }
  }

  /**
   * Svelte action：输入框挂载即聚焦并全选。
   * 用 action 而非 `$effect` + `querySelector`，避免手动跨 DOM 查询。
   */
  function autofocus(node: HTMLInputElement): void {
    node.focus();
    node.select();
  }

  async function unpair(deviceId: string, name: string): Promise<void> {
    const ok = await ui.confirm('sync.unpair', 'sync.unpair_confirm', { name });
    if (ok) {
      await sync.unpair(deviceId);
    }
  }
</script>

<div class="pms-card sync-card">
  <h3>{ui.t('sync.devices')}</h3>

  {#if sync.devices.length === 0}
    <p class="pms-meta">{ui.t('sync.no_devices')}</p>
  {:else}
    <ul class="sync-list">
      {#each sync.devices as device (device.device_id)}
        {@const editing = editingId === device.device_id}
        {@const trimmed = draft.trim()}
        {@const tooLong = utf8Bytes(trimmed) > MAX_NAME_BYTES}
        {@const canSave = trimmed.length > 0 && !tooLong && !saving}
        <li class="sync-device">
          <span class="sync-device-icon"><Icon name="device" size={18} /></span>
          <div class="sync-device-meta">
            {#if editing}
              <input
                class="pms-input"
                type="text"
                maxlength="64"
                autocomplete="off"
                spellcheck="false"
                placeholder={ui.t('sync.rename_hint')}
                aria-label={ui.t('common.rename')}
                bind:value={draft}
                use:autofocus
                onkeydown={(event) => onKeydown(event, device.device_id)}
              />
              {#if tooLong}
                <p class="pms-hint sync-rename-hint">{ui.t('sync.rename_too_long')}</p>
              {/if}
              <div class="sync-rename-actions">
                <button
                  class="pms-btn is-sm is-primary"
                  type="button"
                  disabled={!canSave}
                  onclick={() => void saveEdit(device.device_id)}
                >
                  {ui.t('common.save')}
                </button>
                <button class="pms-btn is-ghost is-sm" type="button" disabled={saving} onclick={endEdit}>
                  {ui.t('common.cancel')}
                </button>
              </div>
            {:else}
              <span class="sync-device-name">{device.device_name}</span>
            {/if}
            {#if hasSessionCode(device.fingerprint)}
              <span class="sync-device-sub">
                {ui.t('sync.fingerprint')}:
                <span class="sync-mono" title={device.device_id}>{fingerprintText(device.fingerprint)}</span>
              </span>
            {/if}
            <span class="sync-device-sub">
              {ui.t('sync.paired_at')}:
              {device.paired_at ? formatDateTime(device.paired_at, ui.locale) : ui.t('common.unknown')}
            </span>
            <span class="sync-device-sub">
              {ui.t('sync.last_sync')}:
              {device.last_sync_at ? formatDateTime(device.last_sync_at, ui.locale) : ui.t('sync.never')}
            </span>
          </div>
          {#if !editing}
            <button
              class="pms-icon-btn"
              type="button"
              title={ui.t('common.rename')}
              aria-label={ui.t('common.rename')}
              onclick={() => startEdit(device.device_id, device.device_name)}
            >
              <Icon name="pencil" size={15} />
            </button>
          {/if}
          <button class="pms-btn is-ghost is-sm sync-danger" type="button" onclick={() => void unpair(device.device_id, device.device_name)}>
            {ui.t('sync.unpair')}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .sync-card {
    padding: 1.1rem;
  }
  h3 {
    margin: 0 0 0.7rem;
    font-size: 1rem;
    font-weight: 600;
  }
  .sync-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .sync-device {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r);
    padding: 0.65rem 0.75rem;
    background: var(--pms-surface);
  }
  .sync-device-icon {
    color: var(--pms-accent);
    display: flex;
  }
  .sync-device-meta {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .sync-device-name {
    font-size: 0.9rem;
    font-weight: 600;
  }
  /* D-66：行内改名输入框直接复用全局 .pms-input（宽度已是 100%），
     此组件**不重写**任何 border / background / font-size —— 见设计规范 §5.6。 */
  .sync-rename-actions {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    margin-top: 0.35rem;
  }
  /* 就地内联小字（§4「错误提示」：.pms-hint 形态 + --pms-danger 色，不弹大块红字）。 */
  .sync-rename-hint {
    margin: 0.2rem 0 0;
    color: var(--pms-danger);
  }
  .sync-device-sub {
    font-size: 0.72rem;
    color: var(--pms-muted);
    word-break: break-all;
  }
  .sync-mono {
    font-family: var(--pms-mono);
  }
  .sync-danger {
    color: var(--pms-danger);
  }
</style>
