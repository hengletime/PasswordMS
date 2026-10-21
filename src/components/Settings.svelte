<script lang="ts">
  // 设置（PRD P-10 / O-03 / O-08 / D-38 信息架构）：
  // 分组自上而下按使用频率排布：外观（主题/语言）→ 安全 → 体检 → 同步 → 数据（导入/导出）
  // → 设备 → 改密码 → 库文件。高频项置顶，低频/危险项沉底。
  // 顶栏原来的「语言 / 导入」入口移到这里（D-38：顶栏只留高频动作）。
  import { save } from '@tauri-apps/plugin-dialog';
  import { getVersion } from '@tauri-apps/api/app';
  import Icon from './common/Icon.svelte';
  import ImportWizard from './ImportWizard.svelte';
  import LegalDialog from './LegalDialog.svelte';
  import Modal from './common/Modal.svelte';
  import { AUTO_LOCK_OPTIONS, formatBytes } from '../lib/constants';
  import { toAppError } from '../lib/errors';
  import { LOCALES, type Locale } from '../lib/i18n/index';
  import { api } from '../lib/ipc';
  import { loadSettingsPageSide } from '../lib/settings-page';
  import { settings } from '../lib/stores/settings.svelte';
  import { sync } from '../lib/stores/sync.svelte';
  import { theme } from '../lib/stores/theme.svelte';
  import { ui } from '../lib/stores/ui.svelte';
  import { runVaultExport } from '../lib/vault-export';
  import { isSafeUrl } from '../lib/url-safety';
  import type { VaultFileInfoDto, VaultSettings } from '../types';

  /** D-63：项目主页（「关于」分组打开外链，走与详情页相同的安全校验 + IPC 链路）。 */
  const HOMEPAGE_URL = 'https://github.com/hengletime/PasswordMS';

  // 语言分段钮的显示名：语言名用母语书写（中文/English），属于专有名词，不做翻译。
  const LOCALE_NAMES: Record<Locale, string> = { 'zh-CN': '中文', 'en-US': 'English' };

  let draft = $state<VaultSettings>({ ...settings.value });
  let fileInfo = $state<VaultFileInfoDto | null>(null);
  // D-73：读取失败必须**显式建模**，否则界面上「读失败」与「本来就没有」长得一样（都是空白卡片）。
  let fileInfoFailed = $state(false);
  // D-73：档位读失败时显示的是兜底档，必须让用户知道（否则「5 分钟」看起来像真实设置）。
  let autoLockFailed = $state(false);
  let oldPassword = $state('');
  // D-51：导出保险库需要单独输入当前主密码（后端 repo.export_vault 会用它做 unlock_check 后再 copy 库文件）。
  // 旧实现误用「修改主密码」卡片的 oldPassword，而导出卡片根本没有输入框 → 空密码必报「解锁失败，请重试。」
  let exportPassword = $state('');
  let newPassword = $state('');
  let newPassword2 = $state('');
  let importOpen = $state(false);
  // D-63：「关于」分组 —— 版本号运行时读取（禁硬编码），失败显示「—」不阻塞页面。
  let appVersion = $state<string | null>(null);
  let legalOpen = $state(false);
  let legalKind = $state<'disclaimer' | 'terms'>('disclaimer');

  /** 打开法律条款弹窗（免责声明 / 用户协议）。 */
  function openLegal(docKind: 'disclaimer' | 'terms'): void {
    legalKind = docKind;
    legalOpen = true;
  }

  /** 打开项目主页：与详情页 openUrl 同一链路（url-safety 白名单校验 → IPC opener）。 */
  async function openHomepage(): Promise<void> {
    if (!isSafeUrl(HOMEPAGE_URL)) {
      ui.toast('error', 'err.unsafe_url');
      return;
    }
    try {
      await api.system.openUrl(HOMEPAGE_URL);
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }
  // D-61：自动锁定档位真持久化（`Vault.auto_lock_minutes` 明文头部）——
  // 初始化读 IPC（get_auto_lock），换档即写（set_auto_lock）；失败回滚 select 显示值。
  let autoLockMinutes = $state<number>(AUTO_LOCK_OPTIONS[1] ?? 5);

  $effect((): void => {
    // D-73：`settings.value` 是 `$state` 的**深层响应式 Proxy**，而结构化克隆算法
    // 遇到 Proxy 必抛 `DataCloneError`。这行曾是 effect 的第一行，一抛 →
    // 后面三块（库文件信息 / 自动锁定档位 / 版本号）**从不执行**，
    // 而且不报错到界面：主人只看到「卡片空白 + 版本显示「—」+ 自动锁定永远是 5 分钟」。
    // 深拷纯对象用 `$state.snapshot()`。
    // ⚠️ 不要改成浅拷贝 `{ ...settings.value }`：`generator` / `health` 两层仍是代理，
    // 下面 `bind:value` 改草稿会**直接写穿到 store**，变成「取消不掉」的隐藏 bug。
    draft = $state.snapshot(settings.value);
    // D-73：三块旁路数据走 `loadSettingsPageSide`，**各自隔离失败**；
    // 任一失败只影响它自己那块的显示，不再带塌其余两项。
    void loadSettingsPageSide(
      {
        loadVaultFileInfo: (): Promise<VaultFileInfoDto> => api.system.vaultFileInfo(),
        loadAutoLock: (): Promise<number> => api.vault.getAutoLock(),
        loadVersion: (): Promise<string> => getVersion(),
      },
      AUTO_LOCK_OPTIONS[1] ?? 5,
    ).then((side): void => {
      fileInfo = side.fileInfo;
      fileInfoFailed = side.fileInfoFailed;
      autoLockFailed = side.autoLockFailed;
      // 档位读失败时模块已回落成 fallback；这里再兜一次 null，保证 select 永远有选中项。
      autoLockMinutes = side.autoLockMinutes ?? (AUTO_LOCK_OPTIONS[1] ?? 5);
      appVersion = side.version;
    });
  });

  /** D-61：换档 → set_auto_lock → success toast；失败 error toast 并回滚显示值。 */
  async function onAutoLockChange(event: Event): Promise<void> {
    const value = Number((event.target as HTMLSelectElement).value);
    const previous = autoLockMinutes;
    if (value === previous) {
      return;
    }
    autoLockMinutes = value;
    try {
      await api.vault.setAutoLock(value);
      ui.toast('success', 'settings.auto_lock_updated', { n: value });
    } catch (err) {
      autoLockMinutes = previous; // 回滚 select 显示值（档位未生效）
      ui.toast('error', toAppError(err).messageKey);
    }
  }

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
    // D-51 / Q-1：守卫与流程抽到 src/lib/vault-export.ts（可被 vitest 行为级验证），
    // 这里只注入真实依赖；`exportPassword` 为空时该模块不发 IPC、就地提示。
    const result = await runVaultExport({
      exportPassword,
      confirm: (): Promise<boolean> =>
        ui.confirm('settings.export_confirm_title', 'settings.export_confirm_body'),
      pickDestination: (): Promise<string | null> =>
        save({ defaultPath: 'passwordms-export.vault' }),
      exportVault: (dest: string, password: string): Promise<void> =>
        api.vault.exportVault(dest, password),
      toast: (kind, messageKey): void => {
        ui.toast(kind, messageKey);
      },
    });
    if (result.cleared) {
      exportPassword = ''; // 导出成功后清空，避免明文主密码残留在组件状态里
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
      <select id="set-autolock" class="pms-input set-ctl" value={autoLockMinutes} onchange={(e) => void onAutoLockChange(e)}>
        {#each AUTO_LOCK_OPTIONS as minutes (minutes)}
          <option value={minutes}>{ui.t('common.minutes', { n: minutes })}</option>
        {/each}
      </select>
    </div>
    {#if autoLockFailed}
      <p class="pms-hint">{ui.t('settings.auto_lock_failed')}</p>
    {/if}
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
    <!-- D-73：主人原话「保险库文件是什么作用？没有看到文件啊？」——
         标题没解释 + 下面空白是**双重失败**。先给一句它是什么/有什么用，再给路径与大小；
         读不到时显示「读取失败」而不是留空（空状态 = 静默兜底，把失败伪装成「本来就没有」）。 -->
    <p class="pms-meta">{ui.t('settings.vault_file_hint')}</p>
    {#if fileInfo}
      <p class="pms-meta set-mono">{fileInfo.path}</p>
      <p class="pms-meta">
        {ui.t('settings.size')}: {formatBytes(fileInfo.size_bytes)}
      </p>
    {:else if fileInfoFailed}
      <p class="pms-hint">{ui.t('settings.vault_file_failed')}</p>
    {:else}
      <p class="pms-meta">{ui.t('common.loading')}</p>
    {/if}
  </div>

  <!-- D-63：「关于」分组（需求：软件里必须有版本与作者等信息）。 -->
  <div class="pms-card set-card">
    <h3>{ui.t('settings.group_about')}</h3>
    <!-- D-73：纯信息行加 `is-info`、按钮行加 `is-actions`。
         窄屏的 `.set-row { flex-direction: column }` 是**为控件行写的**（160px 固定宽在 390px 会溢出），
         它只判了 `.set-row`，于是把信息行（「作者 / hengletime」被拆成两行，读起来像两条独立信息）
         和按钮行（`align-items: stretch` 把三个按钮拉成满宽竖排，吃掉小半屏）一起误伤。
         按行类型显式区分，不用 `:has()`（WebView 版本差异，别赌）。 -->
    <div class="set-row is-info">
      <span class="set-name">PasswordMS</span>
      <span class="pms-meta">{ui.t('settings.about_version')}: {appVersion ?? '—'}</span>
    </div>
    <div class="set-row is-info">
      <span class="set-label">{ui.t('settings.about_author')}</span>
      <span class="pms-meta">hengletime</span>
    </div>
    <div class="set-row is-info">
      <span class="set-label">{ui.t('settings.about_license')}</span>
      <span class="pms-meta">MIT License</span>
    </div>
    <div class="set-row is-actions">
      <button class="pms-btn is-ghost is-sm" type="button" onclick={() => void openHomepage()}>
        <Icon name="external" size={14} />{ui.t('settings.about_homepage')}
      </button>
      <button class="pms-btn is-ghost is-sm" type="button" onclick={() => openLegal('disclaimer')}>
        {ui.t('settings.about_disclaimer')}
      </button>
      <button class="pms-btn is-ghost is-sm" type="button" onclick={() => openLegal('terms')}>
        {ui.t('settings.about_terms')}
      </button>
    </div>
  </div>
</section>

<Modal open={importOpen} titleKey="import.title" onclose={() => (importOpen = false)}>
  <ImportWizard ondone={() => (importOpen = false)} />
</Modal>

<!-- D-63：法律条款弹窗（「关于」分组打开）。 -->
<LegalDialog bind:open={legalOpen} kind={legalKind} />

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
  /* 移动端：每行由「label 左 / 控件右」改为上下堆叠，控件占满整宽，避免 160px 固定宽控件 + 长文案溢出。
     ⚠️ 下面 `.set-row` 的堆叠**只适用于含控件的行**（`.set-ctl` / 输入 / 分段控件）。
     纯信息行必须加 `.is-info`、按钮行必须加 `.is-actions` 在下方覆盖回来（同层叠靠源码顺序取胜）。 */
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
    /* 纯信息行（左标签 / 右文本）：窄屏仍保持左右两端对齐。
       不能跟着上面的 .set-row 堆叠 ——「作者 / hengletime」拆成两行会被读成两条信息。 */
    .set-row.is-info {
      flex-direction: row;
      align-items: baseline;
      justify-content: space-between;
      gap: 0.75rem;
    }
    .set-row.is-info .pms-meta {
      text-align: right;
      min-width: 0; /* 长文本可收缩，防溢出 */
    }
    /* 按钮行：窄屏横向排布、允许换行。被 stretch 拉成满宽竖排会吃掉半屏。 */
    .set-row.is-actions {
      flex-direction: row;
      flex-wrap: wrap;
      align-items: center;
      justify-content: flex-start;
      gap: 0.5rem;
    }
  }
</style>
