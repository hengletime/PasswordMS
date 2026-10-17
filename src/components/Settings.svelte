<script lang="ts">
  // 设置（PRD P-10 / O-03 / O-08 / D-38 信息架构）：
  // 分组自上而下按使用频率排布：外观（主题/语言）→ 安全 → 体检 → 同步 → 数据（导入/导出）
  // → 设备 → 改密码 → 库文件。高频项置顶，低频/危险项沉底。
  // 顶栏原来的「语言 / 导入」入口移到这里（D-38：顶栏只留高频动作）。
  import { save } from '@tauri-apps/plugin-dialog';
  import Icon from './common/Icon.svelte';
  import ImportWizard from './ImportWizard.svelte';
  import Modal from './common/Modal.svelte';
  import { AUTO_LOCK_OPTIONS, formatBytes } from '../lib/constants';
  import { toAppError } from '../lib/errors';
  import { LOCALES, type Locale } from '../lib/i18n/index';
  import { api } from '../lib/ipc';
  import { settings } from '../lib/stores/settings.svelte';
  import { sync } from '../lib/stores/sync.svelte';
  import { theme } from '../lib/stores/theme.svelte';
  import { ui } from '../lib/stores/ui.svelte';
  import type { VaultFileInfoDto, VaultSettings } from '../types';

  // 语言分段钮的显示名：语言名用母语书写（中文/English），属于专有名词，不做翻译。
  const LOCALE_NAMES: Record<Locale, string> = { 'zh-CN': '中文', 'en-US': 'English' };

  let draft = $state<VaultSettings>({ ...settings.value });
  let fileInfo = $state<VaultFileInfoDto | null>(null);
  let oldPassword = $state('');
  // D-51：导出保险库需要单独输入当前主密码（后端 repo.export_vault 会用它做 unlock_check 后再 copy 库文件）。
  // 旧实现误用「修改主密码」卡片的 oldPassword，而导出卡片根本没有输入框 → 空密码必报「解锁失败，请重试。」
  let exportPassword = $state('');
  let newPassword = $state('');
  let newPassword2 = $state('');
  let importOpen = $state(false);
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
    // D-51：为空时**不发 IPC**（空密码到后端必 UnlockFailed →「解锁失败，请重试。」误导用户），直接就地提示。
    if (exportPassword.length === 0) {
      ui.toast('error', 'settings.export_password_required');
      return;
    }
    const ok = await ui.confirm('settings.export_confirm_title', 'settings.export_confirm_body');
    if (!ok) {
      return;
    }
    const dest = await save({ defaultPath: 'passwordms-export.vault' });
    if (dest === null) {
      return;
    }
    try {
      await api.vault.exportVault(dest, exportPassword);
      exportPassword = ''; // 导出成功后清空，避免明文主密码残留在组件状态里
      ui.toast('success', 'common.ok');
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  async function unpair(deviceId: string): Promise<void> {
    await sync.unpair(deviceId);
  }
</script>

<section class="set">
  <h2 class="set-title">{ui.t('settings.title')}</h2>

  <div class="pms-card set-card">
    <h3>{ui.t('settings.group_appearance')}</h3>
    <div class="set-row">
      <span class="set-label">{ui.t('theme.label')}</span>
      <div class="set-seg" role="group" aria-label={ui.t('theme.label')}>
        <button
          class="pms-btn is-sm"
          class:is-primary={theme.mode === 'system'}
          type="button"
          aria-pressed={theme.mode === 'system'}
          onclick={() => theme.setMode('system')}
        >
          {ui.t('theme.system')}
        </button>
        <button
          class="pms-btn is-sm"
          class:is-primary={theme.mode === 'light'}
          type="button"
          aria-pressed={theme.mode === 'light'}
          onclick={() => theme.setMode('light')}
        >
          {ui.t('theme.light')}
        </button>
        <button
          class="pms-btn is-sm"
          class:is-primary={theme.mode === 'dark'}
          type="button"
          aria-pressed={theme.mode === 'dark'}
          onclick={() => theme.setMode('dark')}
        >
          {ui.t('theme.dark')}
        </button>
      </div>
    </div>
    <div class="set-row">
      <span class="set-label">{ui.t('nav.language')}</span>
      <div class="set-seg" role="group" aria-label={ui.t('nav.language')}>
        {#each LOCALES as loc (loc)}
          <button
            class="pms-btn is-sm"
            class:is-primary={ui.locale === loc}
            type="button"
            aria-pressed={ui.locale === loc}
            onclick={() => ui.setLocale(loc)}
          >
            {LOCALE_NAMES[loc]}
          </button>
        {/each}
      </div>
    </div>
  </div>

  <div class="pms-card set-card">
    <h3>{ui.t('settings.group_security')}</h3>
    <div class="set-row">
      <label class="set-label" for="set-autolock">{ui.t('settings.auto_lock')}</label>
      <select id="set-autolock" class="pms-input set-ctl" bind:value={autoLockMinutes}>
        {#each AUTO_LOCK_OPTIONS as minutes (minutes)}
          <option value={minutes}>{ui.t('common.minutes', { n: minutes })}</option>
        {/each}
      </select>
    </div>
    <div class="set-row">
      <label class="set-label" for="set-ttl">{ui.t('settings.clipboard_ttl')}</label>
      <input id="set-ttl" class="pms-input set-ctl" type="number" min="5" max="120" bind:value={draft.clipboard_clear_secs} />
    </div>
  </div>

  <div class="pms-card set-card">
    <h3>{ui.t('settings.group_health')}</h3>
    <div class="set-row">
      <label class="set-label" for="set-weak">{ui.t('settings.weak_max_len')}</label>
      <input id="set-weak" class="pms-input set-ctl" type="number" min="4" max="16" bind:value={draft.health.weak_max_len} />
    </div>
    <div class="set-row">
      <label class="set-label" for="set-stale">{ui.t('settings.stale_days')}</label>
      <input id="set-stale" class="pms-input set-ctl" type="number" min="30" max="365" bind:value={draft.health.stale_days} />
    </div>
  </div>

  <div class="pms-card set-card">
    <h3>{ui.t('settings.group_sync')}</h3>
    <div class="set-row">
      <span class="set-label">{ui.t('settings.auto_sync')}</span>
      <label class="pms-check"><input type="checkbox" bind:checked={draft.auto_sync} /></label>
    </div>
    <div class="set-row">
      <span class="set-label">{ui.t('settings.sync_on_start')}</span>
      <label class="pms-check"><input type="checkbox" bind:checked={draft.sync_on_start} /></label>
    </div>
  </div>

  <button class="pms-btn is-primary" type="button" onclick={() => void saveSettings()}>
    {ui.t('common.save')}
  </button>

  <div class="pms-card set-card">
    <h3>{ui.t('import.title')}</h3>
    <button class="pms-btn is-ghost" type="button" onclick={() => (importOpen = true)}>
      <Icon name="upload" size={15} />{ui.t('import.title')}
    </button>
  </div>

  <div class="pms-card set-card">
    <h3>{ui.t('settings.export_vault')}</h3>
    <p class="pms-alert is-warning">
      <Icon name="alert" size={16} />
      <span>{ui.t('settings.export_confirm_body')}</span>
    </p>
    <!-- D-51：导出同样需要当前主密码（文案复用 settings.change_password_old「当前主密码」，语义一致）。
         id 不能复用 set-pw-old，否则与「修改主密码」卡片的输入框重复。 -->
    <div class="set-row">
      <label class="set-label" for="set-pw-export">{ui.t('settings.change_password_old')}</label>
      <input
        id="set-pw-export"
        class="pms-input set-ctl"
        type="password"
        bind:value={exportPassword}
        autocomplete="current-password"
      />
    </div>
    <button class="pms-btn is-ghost" type="button" onclick={() => void exportVault()}>
      <Icon name="download" size={15} />{ui.t('settings.export_vault')}
    </button>
  </div>

  <div class="pms-card set-card">
    <h3>{ui.t('sync.devices')}</h3>
    {#if sync.devices.length === 0}
      <p class="pms-meta">{ui.t('sync.no_devices')}</p>
    {:else}
      <ul class="set-list">
        {#each sync.devices as device (device.device_id)}
          <li class="set-list-row">
            <span class="set-name">{device.device_name}</span>
            <button class="pms-btn is-ghost is-sm set-danger" type="button" onclick={() => void unpair(device.device_id)}>
              {ui.t('sync.unpair')}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="pms-card set-card">
    <h3>{ui.t('settings.change_password')}</h3>
    <p class="pms-alert is-warning">
      <Icon name="alert" size={16} />
      <span>{ui.t('create.warning_body')}</span>
    </p>
    <div class="set-row">
      <label class="set-label" for="set-pw-old">{ui.t('settings.change_password_old')}</label>
      <input id="set-pw-old" class="pms-input set-ctl" type="password" bind:value={oldPassword} autocomplete="current-password" />
    </div>
    <div class="set-row">
      <label class="set-label" for="set-pw-new">{ui.t('settings.change_password_new')}</label>
      <input id="set-pw-new" class="pms-input set-ctl" type="password" bind:value={newPassword} autocomplete="new-password" />
    </div>
    <div class="set-row">
      <label class="set-label" for="set-pw-new2">{ui.t('create.confirm_password')}</label>
      <input id="set-pw-new2" class="pms-input set-ctl" type="password" bind:value={newPassword2} autocomplete="new-password" />
    </div>
    <button class="pms-btn" type="button" onclick={() => void changePassword()}>{ui.t('common.confirm')}</button>
  </div>

  <div class="pms-card set-card">
    <h3>{ui.t('settings.vault_file')}</h3>
    {#if fileInfo}
      <p class="pms-meta set-mono">{fileInfo.path}</p>
      <p class="pms-meta">
        {ui.t('settings.size')}: {formatBytes(fileInfo.size_bytes)}
      </p>
    {/if}
  </div>
</section>

<Modal open={importOpen} titleKey="import.title" onclose={() => (importOpen = false)}>
  <ImportWizard ondone={() => (importOpen = false)} />
</Modal>

<style>
  .set {
    flex: 1;
    padding: 1.1rem;
    overflow: auto;
    max-width: 680px;
  }
  .set-title {
    margin: 0 0 1rem;
    font-size: 1.2rem;
    font-weight: 600;
  }
  .set-card {
    padding: 1rem;
    margin-bottom: 1rem;
  }
  h3 {
    margin: 0 0 0.7rem;
    font-size: 1rem;
    font-weight: 600;
    color: var(--pms-fg);
  }
  .set-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.35rem 0;
  }
  .set-label {
    font-size: 0.86rem;
    color: var(--pms-muted);
  }
  .set-ctl {
    width: 160px;
    flex: 0 0 auto;
  }
  /* 分段控件：跟随系统 / 浅色 / 深色。 */
  .set-seg {
    display: inline-flex;
    gap: 0.3rem;
    flex: 0 0 auto;
  }
  .set-mono {
    font-family: var(--pms-mono);
    word-break: break-all;
  }
  .set-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .set-list-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }
  .set-name {
    font-size: 0.88rem;
  }
  .set-danger {
    color: var(--pms-danger);
  }
  .set-card .pms-alert {
    margin: 0 0 0.75rem;
  }
  /* 移动端：每行由「label 左 / 控件右」改为上下堆叠，控件占满整宽，避免 160px 固定宽控件 + 长文案溢出。 */
  @media (max-width: 767px) {
    .set {
      padding: 0.9rem;
    }
    .set-row {
      flex-direction: column;
      align-items: stretch;
      gap: 0.35rem;
    }
    .set-ctl {
      width: 100%;
    }
    .set-seg {
      align-self: flex-start;
    }
  }
</style>
