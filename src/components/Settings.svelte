<script lang="ts">
  // 设置（PRD P-10 / O-03 / O-08）：安全、体检、生成器、同步、解除配对、导出。
  import { save } from '@tauri-apps/plugin-dialog';
  import Icon from './common/Icon.svelte';
  import { AUTO_LOCK_OPTIONS, formatBytes } from '../lib/constants';
  import { toAppError } from '../lib/errors';
  import { api } from '../lib/ipc';
  import { settings } from '../lib/stores/settings.svelte';
  import { sync } from '../lib/stores/sync.svelte';
  import { ui } from '../lib/stores/ui.svelte';
  import type { VaultFileInfoDto, VaultSettings } from '../types';

  let draft = $state<VaultSettings>({ ...settings.value });
  let fileInfo = $state<VaultFileInfoDto | null>(null);
  let oldPassword = $state('');
  let newPassword = $state('');
  let newPassword2 = $state('');
  // 【已知缺口】`auto_lock_minutes` 位于 `Vault` 明文头部（PRD §4.1），当前 IPC 契约
  // （`get_settings` / `update_settings` 仅覆盖 `VaultSettings`）未提供其读写通道，
  // 故此处退化为「会话级本地偏好」，仅承载 UI 档位，待后端补充设置命令后接入持久化。
  // 详见 docs/03-impl.md 遗留 TODO（T14 自动锁持久化）。
  let autoLockMinutes = $state<number>(AUTO_LOCK_OPTIONS[1] ?? 5);

  $effect((): void => {
    // settings.value 加载完成后同步到草稿。
    draft = structuredClone(settings.value);
    void api.system
      .vaultFileInfo()
      .then((info: VaultFileInfoDto): void => {
        fileInfo = info;
      })
      .catch((): void => {
        fileInfo = null;
      });
  });

  async function saveSettings(): Promise<void> {
    await settings.save($state.snapshot(draft));
  }

  async function changePassword(): Promise<void> {
    if (newPassword !== newPassword2) {
      ui.toast('error', 'create.mismatch');
      return;
    }
    try {
      await api.vault.changeMasterPassword(oldPassword, newPassword);
      oldPassword = '';
      newPassword = '';
      newPassword2 = '';
      ui.toast('success', 'common.ok');
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  async function exportVault(): Promise<void> {
    const ok = await ui.confirm('settings.export_confirm_title', 'settings.export_confirm_body');
    if (!ok) {
      return;
    }
    const dest = await save({ defaultPath: 'passwordms-export.vault' });
    if (dest === null) {
      return;
    }
    try {
      await api.vault.exportVault(dest, oldPassword);
      ui.toast('success', 'common.ok');
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  async function unpair(deviceId: string): Promise<void> {
    await sync.unpair(deviceId);
  }
</script>

<section class="settings">
  <h2>{ui.t('settings.title')}</h2>

  <div class="card">
    <h3>{ui.t('settings.group_security')}</h3>
    <label class="field">
      <span>{ui.t('settings.auto_lock')}</span>
      <select bind:value={autoLockMinutes}>
        {#each AUTO_LOCK_OPTIONS as minutes (minutes)}
          <option value={minutes}>{ui.t('common.minutes', { n: minutes })}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      <span>{ui.t('settings.clipboard_ttl')}</span>
      <input type="number" min="5" max="120" bind:value={draft.clipboard_clear_secs} />
    </label>
  </div>

  <div class="card">
    <h3>{ui.t('settings.group_health')}</h3>
    <label class="field">
      <span>{ui.t('settings.weak_max_len')}</span>
      <input type="number" min="4" max="16" bind:value={draft.health.weak_max_len} />
    </label>
    <label class="field">
      <span>{ui.t('settings.stale_days')}</span>
      <input type="number" min="30" max="365" bind:value={draft.health.stale_days} />
    </label>
  </div>

  <div class="card">
    <h3>{ui.t('settings.group_sync')}</h3>
    <label class="check">
      <input type="checkbox" bind:checked={draft.auto_sync} />
      <span>{ui.t('settings.auto_sync')}</span>
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={draft.sync_on_start} />
      <span>{ui.t('settings.sync_on_start')}</span>
    </label>
  </div>

  <button class="btn primary" type="button" onclick={() => void saveSettings()}>
    {ui.t('common.save')}
  </button>

  <div class="card">
    <h3>{ui.t('settings.change_password')}</h3>
    <label class="field">
      <span>{ui.t('settings.change_password_old')}</span>
      <input type="password" bind:value={oldPassword} autocomplete="current-password" />
    </label>
    <label class="field">
      <span>{ui.t('settings.change_password_new')}</span>
      <input type="password" bind:value={newPassword} autocomplete="new-password" />
    </label>
    <label class="field">
      <span>{ui.t('create.confirm_password')}</span>
      <input type="password" bind:value={newPassword2} autocomplete="new-password" />
    </label>
    <button class="btn" type="button" onclick={() => void changePassword()}>{ui.t('common.confirm')}</button>
  </div>

  <div class="card">
    <h3>{ui.t('settings.export_vault')}</h3>
    <button class="btn ghost" type="button" onclick={() => void exportVault()}>
      <Icon name="download" size={15} />{ui.t('settings.export_vault')}
    </button>
  </div>

  <div class="card">
    <h3>{ui.t('sync.devices')}</h3>
    {#if sync.devices.length === 0}
      <p class="muted">{ui.t('sync.no_devices')}</p>
    {:else}
      <ul>
        {#each sync.devices as device (device.device_id)}
          <li>
            <span class="name">{device.device_name}</span>
            <button class="btn ghost small danger-text" type="button" onclick={() => void unpair(device.device_id)}>
              {ui.t('sync.unpair')}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="card">
    <h3>{ui.t('settings.vault_file')}</h3>
    {#if fileInfo}
      <p class="muted mono">{fileInfo.path}</p>
      <p class="muted">
        {ui.t('settings.size')}: {formatBytes(fileInfo.size_bytes)}
      </p>
    {/if}
  </div>
</section>

<style>
  .settings {
    flex: 1;
    padding: 1rem;
    overflow: auto;
    max-width: 640px;
  }
  h2 {
    margin: 0 0 1rem;
    font-size: 1.1rem;
  }
  .card {
    border: 1px solid var(--pms-border);
    border-radius: 12px;
    padding: 0.9rem;
    margin-bottom: 0.9rem;
  }
  h3 {
    margin: 0 0 0.6rem;
    font-size: 0.9rem;
    color: var(--pms-muted);
  }
  .muted {
    color: var(--pms-muted);
    font-size: 0.82rem;
    word-break: break-all;
  }
  .mono {
    font-family: var(--pms-mono);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }
  .name {
    font-size: 0.88rem;
  }
  .danger-text {
    color: var(--pms-danger);
  }
</style>
