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
  import Spinner from './common/Spinner.svelte';
  import { AUTO_LOCK_OPTIONS, formatBytes } from '../lib/constants';
  import { toAppError } from '../lib/errors';
  import { LOCALES, type Locale } from '../lib/i18n/index';
  import { api } from '../lib/ipc';
  import { loadSettingsPageSide } from '../lib/settings-page';
  import { session } from '../lib/stores/session.svelte';
  import { settings } from '../lib/stores/settings.svelte';
  import { sync } from '../lib/stores/sync.svelte';
  import { theme } from '../lib/stores/theme.svelte';
  import { ui } from '../lib/stores/ui.svelte';
  import {
    UPDATE_RELEASES_URL,
    fetchLatestVersion,
    isAutoCheckEnabled,
    markAutoChecked,
    setAutoCheckEnabled,
    shouldAutoCheck,
    storedLatest,
    type UpdateCheckStorage,
  } from '../lib/update-check';
  import { runVaultExport } from '../lib/vault-export';
  import { isSafeUrl } from '../lib/url-safety';
  import type { CompactionStatsDto, VaultFileInfoDto, VaultSettings } from '../types';

  /** D-63：项目主页（「关于」分组打开外链，走与详情页相同的安全校验 + IPC 链路）。 */
  const HOMEPAGE_URL = 'https://github.com/hengletime/PasswordMS';

  // 语言分段钮的显示名：语言名用母语书写（中文/繁體中文/English/ไทย），属于专有名词，不做翻译。
  const LOCALE_NAMES: Record<Locale, string> = {
    'zh-CN': '中文',
    'zh-TW': '繁體中文',
    'en-US': 'English',
    th: 'ไทย',
  };

  let draft = $state<VaultSettings>({ ...settings.value });
  let fileInfo = $state<VaultFileInfoDto | null>(null);
  // D-73：读取失败必须**显式建模**，否则界面上「读失败」与「本来就没有」长得一样（都是空白卡片）。
  let fileInfoFailed = $state(false);
  // D-73：档位读失败时显示的是兜底档，必须让用户知道（否则「5 分钟」看起来像真实设置）。
  let autoLockFailed = $state(false);
  // D-113 P2：快速解锁（本机旁路密封）。状态**全部来自后端**（`biometric_status`），
  // 不在前端维护第二份真相——开关是否生效只有 `biometric.json` 说了算。
  let bioBusy = $state(false);
  /** 已点「开启」，正在等用户输入主密码（§4.5：开启必须证明知道主密码）。 */
  let bioAsking = $state(false);
  let bioPassword = $state('');
  const bio = $derived(session.biometric);
  const bioAvailable = $derived(bio?.availability === 'available');
  const bioEnabled = $derived(bio?.state === 'ready');
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
  // D-76：保险库体积构成（只读统计）。`null` + `compactionStatsFailed` 分别表示
  // 「还没返回」与「读取失败」—— 两者在界面上必须长得不一样（D-73 的教训）。
  let compactionStats = $state<CompactionStatsDto | null>(null);
  let compactionStatsFailed = $state(false);
  /** 压缩自身进行中（重写 200MB 级容器可能几秒，必须有忙碌态）。 */
  let compacting = $state(false);
  /** 二次确认块是否打开（`ui.confirm()` 只支持「标题 + 一句正文」，装不下四个数字 + 三条提示）。 */
  let compactConfirmOpen = $state(false);

  /** 可回收字节数；stats 未取到按 0 处理（按钮本就不该出现）。 */
  const reclaimableBytes = $derived(compactionStats?.reclaimable_bytes ?? 0);
  /** 已删/无引用附件密文字节数（回答主人那句「是不是被删除的附件都还保存在库里」）。 */
  const deadAttachmentBytes = $derived(compactionStats?.attachment_dead_bytes ?? 0);
  /** 只有真的能回收才显示按钮：点了没反应的按钮就是假功能。 */
  const canCompact = $derived(reclaimableBytes > 0);
  /**
   * 同步进行中：后端 `compact_vault` 在 `state.session` 锁内执行，与同步**结构互斥**，
   * 不禁用的话用户会以为卡死。用现成的 `sync.syncingDeviceId` 判定（D-66 已把忙碌态按设备隔离）。
   */
  const syncBusy = $derived(sync.syncingDeviceId !== null);

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

  // ===== D-107：更新检查（方案 B：自动检查默认关 + 手动按钮；只提醒不下载） =====

  /** localStorage 抽象（测试可替换；运行环境恒为浏览器 WebView）。 */
  function updateStorage(): UpdateCheckStorage {
    return window.localStorage;
  }

  let updateEnabled = $state(false);
  /** idle / checking / uptodate / available / failed。 */
  let updateState = $state<'idle' | 'checking' | 'uptodate' | 'available' | 'failed'>('idle');
  /** 查到的新版本号（available 时显示）。 */
  let updateLatest = $state<string | null>(null);
  /** 防重入。刻意用**非响应式**变量：$effect 会追踪同步读取的 $state，
   * 在 effect 里读 `updateState` 再写它会形成自依赖（Svelte 5 会报自环）。 */
  let checkInFlight = false;

  /** 检查一次并把结果写进 UI 状态；自动检查成功与否都记节流时间戳。 */
  async function runUpdateCheck(auto: boolean): Promise<void> {
    if (checkInFlight || appVersion === null) {
      return;
    }
    checkInFlight = true;
    updateState = 'checking';
    const result = await fetchLatestVersion(window.fetch.bind(window), appVersion);
    if (auto) {
      markAutoChecked(updateStorage(), result?.hasUpdate ? result.latestVersion : null);
    }
    if (result === null) {
      // 手动检查失败才显式提示；自动检查静默（GitHub 不可达不该打扰用户）。
      updateState = auto ? 'idle' : 'failed';
    } else if (result.hasUpdate) {
      updateLatest = result.latestVersion;
      updateState = 'available';
    } else {
      updateLatest = null;
      updateState = 'uptodate';
    }
    checkInFlight = false;
  }

  /** 开关切换：开启后立即做一次（若节流窗口已开）；关闭清掉残留提示。 */
  async function onUpdateCheckToggle(event: Event): Promise<void> {
    const enabled = (event.target as HTMLInputElement).checked;
    setAutoCheckEnabled(updateStorage(), enabled);
    updateEnabled = enabled;
    if (enabled) {
      if (shouldAutoCheck(updateStorage())) {
        await runUpdateCheck(true);
      }
    } else {
      updateLatest = null;
      updateState = 'idle';
    }
  }

  /** 手动「检查更新」按钮。 */
  async function onUpdateCheckNow(): Promise<void> {
    await runUpdateCheck(false);
  }

  /** 跳转 Releases 下载页（open_url 白名单链路，与项目主页同路）。 */
  async function openReleases(): Promise<void> {
    if (!isSafeUrl(UPDATE_RELEASES_URL)) {
      ui.toast('error', 'err.unsafe_url');
      return;
    }
    try {
      await api.system.openUrl(UPDATE_RELEASES_URL);
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  // 进入设置页：同步开关显示 + 若自动检查开启且过节流窗口则静默查一次；
  // 上次自动检查查到过新版本的话，直接把提示摆出来（不必等下次检查）。
  $effect((): void => {
    updateEnabled = isAutoCheckEnabled(updateStorage());
    const cached = storedLatest(updateStorage());
    if (cached !== null && appVersion !== null) {
      updateLatest = cached;
      updateState = 'available';
    }
    if (shouldAutoCheck(updateStorage())) {
      void runUpdateCheck(true);
    }
  });
  // D-61：自动锁定档位真持久化（`Vault.auto_lock_minutes` 明文头部）——
  // 初始化读 IPC（get_auto_lock），换档即写（set_auto_lock）；失败回滚 select 显示值。
  let autoLockMinutes = $state<number>(AUTO_LOCK_OPTIONS[1] ?? 5);

  /**
   * 加载四块旁路数据并写回组件状态（D-73 失败隔离 / D-76 第四块）。
   *
   * 抽成函数而不是内联在 `$effect` 里，是因为**压缩成功后必须重新拉取**
   * `fileInfo` 与 `compactionStats` —— 只弹 toast 不刷新，卡片上还是 24.3 MB，
   * 用户会以为没生效（那正是这次缺陷的翻版）。
   *
   * @param force 压缩进行中也要读。只有 `runCompaction` 成功路径传 `true`：
   *   那一刻 `compact_vault` 已经返回、`state.session` 锁已释放，读到的就是压缩后的真值。
   *   **不要**用「先把 `compacting` 复位再去 reload」代替它 —— `$effect` 的依赖里
   *   有 `compacting`，复位会触发 effect 重跑，于是成功路径变成「显式 reload + effect reload」
   *   两次并发的四块读取；而用参数区分两个意图，语义清晰且显式那次一定真的跑。
   */
  async function reloadSideData(force = false): Promise<void> {
    // D-76：压缩进行中**不许**重新拉取旁路数据。后端 `compact_vault` 在 `state.session`
    // 锁内执行，此刻再调 `vault_compaction_stats` 不会失败、也不会超时——它会**一直阻塞**
    // 到压缩结束。D-73 的「逐块失败隔离」在这里救不了我们：那不是失败，是挂住。
    // 真实触发路径：压缩正在跑 → 用户离开设置页再回来 → `$effect` 重跑本函数。
    // `!force` 必须写在前面：强制调用不能被自己的守卫拦掉。
    if (!force && compacting) {
      return;
    }
    const side = await loadSettingsPageSide(
      {
        loadVaultFileInfo: (): Promise<VaultFileInfoDto> => api.system.vaultFileInfo(),
        loadAutoLock: (): Promise<number> => api.vault.getAutoLock(),
        loadVersion: (): Promise<string> => getVersion(),
        loadCompactionStats: (): Promise<CompactionStatsDto> => api.system.vaultCompactionStats(),
      },
      AUTO_LOCK_OPTIONS[1] ?? 5,
    );
    fileInfo = side.fileInfo;
    fileInfoFailed = side.fileInfoFailed;
    autoLockFailed = side.autoLockFailed;
    // 档位读失败时模块已回落成 fallback；这里再兜一次 null，保证 select 永远有选中项。
    autoLockMinutes = side.autoLockMinutes ?? (AUTO_LOCK_OPTIONS[1] ?? 5);
    appVersion = side.version;
    compactionStats = side.compactionStats;
    compactionStatsFailed = side.compactionStatsFailed;
    // D-113 P2：快速解锁状态与其它旁路数据同批刷新（只读 IPC；失败由 store 归一成
    // 「不可用」并保持 `null`，不参与 `loadSettingsPageSide` 的失败隔离矩阵——
    // 它坏了绝不该影响库文件信息/版本号的显示）。
    void session.refreshBiometric();
  }

  $effect((): void => {
    // D-73：`settings.value` 是 `$state` 的**深层响应式 Proxy**，而结构化克隆算法
    // 遇到 Proxy 必抛 `DataCloneError`。这行曾是 effect 的第一行，一抛 →
    // 后面三块（库文件信息 / 自动锁定档位 / 版本号）**从不执行**，
    // 而且不报错到界面：主人只看到「卡片空白 + 版本显示「—」+ 自动锁定永远是 5 分钟」。
    // 深拷纯对象用 `$state.snapshot()`。
    // ⚠️ 不要改成浅拷贝 `{ ...settings.value }`：`generator` / `health` 两层仍是代理，
    // 下面 `bind:value` 改草稿会**直接写穿到 store**，变成「取消不掉」的隐藏 bug。
    draft = $state.snapshot(settings.value);
    // D-73 / D-76：四块旁路数据走 `loadSettingsPageSide`，**各自隔离失败**；
    // 任一失败只影响它自己那块的显示，不再带塌其余三项。
    void reloadSideData();
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

  /** D-113 P2：点「开启」→ 展开主密码输入（§4.5：开启必须证明知道主密码）。 */
  function startEnableBiometric(): void {
    bioAsking = true;
    bioPassword = '';
  }

  /** 取消开启：收起输入框并清掉刚输入的主密码（不留在内存里）。 */
  function cancelEnableBiometric(): void {
    bioAsking = false;
    bioPassword = '';
  }

  /**
   * 提交开启：`biometric_enable` 内部会校验主密码 + 空库 + 平台可用性，再弹 Windows Hello。
   *
   * 成功后**不做本地状态猜测**——直接重查后端状态，让「已开启（日期）」来自真实记录。
   */
  async function submitEnableBiometric(): Promise<void> {
    if (bioBusy || bioPassword.length === 0) {
      return;
    }
    bioBusy = true;
    try {
      await api.vault.biometricEnable(bioPassword);
      bioPassword = '';
      bioAsking = false;
      await session.refreshBiometric();
    } catch (err) {
      // 主密码错（ERR_UNLOCK_FAILED）/ 平台不可用 / 空库（ERR_VALIDATION）都在这里出文案。
      ui.toast('error', toAppError(err).messageKey);
    } finally {
      bioBusy = false;
    }
  }

  /** 关闭快速解锁：确认后删除封印（这是撤销的唯一动作，§4.3 H-2）。 */
  async function disableBiometric(): Promise<void> {
    if (bioBusy) {
      return;
    }
    const ok = await ui.confirm('settings.bio_disable', 'settings.bio_disable_confirm');
    if (!ok) {
      return;
    }
    bioBusy = true;
    try {
      await api.vault.biometricDisable();
      await session.refreshBiometric();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    } finally {
      bioBusy = false;
    }
  }

  /** 封印创建时间的本地短日期（`bio_state_on` 的 `{date}` 占位）。 */
  function bioEnabledDate(): string {
    const ms = bio?.enabled_at;
    if (typeof ms !== 'number') {
      return '—';
    }
    return new Date(ms).toLocaleDateString(ui.locale);
  }

  /**
   * D-106（豆包审查 3.5-4）：剪贴板 TTL 钳制——HTML min/max 不阻止键入，
   * 0/负值会导致复制即清空、极大值会让密码长期残留剪贴板。
   */  function clampClipboardTtl(raw: number): number {
    const n = Math.floor(Number(raw));
    if (!Number.isFinite(n)) {
      return 30;
    }
    return Math.min(120, Math.max(5, n));
  }

  async function saveSettings(): Promise<void> {
    draft.clipboard_clear_secs = clampClipboardTtl(draft.clipboard_clear_secs);
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

  /**
   * D-76：执行压缩。
   *
   * 三条纪律（对应 §6.11.4）：
   * 1. 成功后**必须**重新拉取 `fileInfo` 与 `compactionStats` —— 只弹 toast 不刷新，
   *    卡片上还是 24.3 MB，用户会以为没生效（这正是本次缺陷的翻版）；
   * 2. 失败时**不改动任何显示数字**（宁可显示旧值，不许显示「已压缩」），且不自动重试；
   * 3. 防重入：这是重操作（重写整个容器 + `spawn_blocking`），双击不许跑两遍。
   */
  async function runCompaction(): Promise<void> {
    if (compacting) {
      return;
    }
    compacting = true;
    compactConfirmOpen = false;
    try {
      const result = await api.system.compactVault();
      ui.toast('success', 'settings.compact_done', {
        size: formatBytes(result.reclaimed_bytes),
      });
      // 显式刷新必须带 `force`：此刻 `compacting` 仍是 true（复位在 `finally`），
      // 不带 `force` 的话这一行会被守卫直接返回 —— 变成一条**永不执行**的语句，
      // 于是「压缩成功后重新拉取」（§6.11.4 判据 4）失效，卡片还显示旧大小。
      await reloadSideData(true);
    } catch (err) {
      // 压缩是大动作：失败时我们并不知道库处于什么状态，一句 `err.internal`（「内部错误」）
      // 会让用户担心库被写坏。故**通用内部错误**换成「保险库原样保留，数据没有变化」；
      // 后端给了**具体原因**（如未解锁）时仍照原样透出——那句更可操作，且同样不改变
      // 「数据没变」这个事实（失败路径不改任何显示数字，见 §6.11.7 判据 5）。
      const appErr = toAppError(err);
      ui.toast(
        'error',
        appErr.messageKey === 'err.internal' ? 'settings.compact_failed' : appErr.messageKey,
      );
    } finally {
      // 唯一的复位点，成功、失败、reload 抛错三条路径都经过它（别删）。
      // 它会让 `compacting` 变一次值，而该信号是 `$effect` 的依赖 → effect 会再跑一次
      // `reloadSideData()`（此刻 force=false、compacting=false，守卫放行）。
      // 也就是说成功路径一共**两组**四块读取：上面那次显式 force + 这次 effect 触发的。
      // 第二组发生在压缩结束、`state.session` 锁已释放之后，读到的同样是真值，不会阻塞也不会写错；
      // 想省掉它得引 `untrack`，属于过度设计（statistics 只遍历内存 slots 元数据，不解密 payload）。
      compacting = false;
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
      <input id="set-ttl" class="pms-input set-ctl" type="number" min="5" max="120" bind:value={draft.clipboard_clear_secs}
        onchange={() => { draft.clipboard_clear_secs = clampClipboardTtl(draft.clipboard_clear_secs); }} />
    </div>
  </div>

  <!-- D-113 P2：快速解锁（Windows Hello）。
       独立卡片而不是塞进「安全」分组：它是**立即生效的动作**，不归底部「保存」按钮管；
       混进安全卡片会让用户以为改完要按保存才生效。 -->
  <div class="pms-card set-card">
    <h3>{ui.t('settings.group_biometric')}</h3>
    <!-- 信息行用 `.set-row.is-info`（§6.10）：窄屏堆叠只针对控件行，信息行不能拆两行。 -->
    <div class="set-row is-info">
      <span class="set-label">{ui.t('settings.bio_title')}</span>
      <span class="bio-state">
        {#if bioEnabled}
          {ui.t('settings.bio_state_on', { date: bioEnabledDate() })}
        {:else if bio?.state === 'corrupt'}
          {ui.t('settings.bio_state_corrupt')}
        {:else if bio?.state === 'stale' && bio.reason}
          {ui.t(`settings.bio_state_stale_${bio.reason}`)}
        {:else}
          {ui.t('settings.bio_state_off')}
        {/if}
      </span>
    </div>
    <p class="pms-hint">{ui.t('settings.bio_desc')}</p>
    <p class="pms-hint">{ui.t('settings.bio_risk')}</p>
    {#if bio && !bioAvailable}
      <p class="pms-hint is-danger">{ui.t(`settings.bio_avail_${bio.availability}`)}</p>
    {/if}

    {#if bioAsking}
      <!-- 开启必须重新输入主密码（§4.5）：防「解锁后离开座位，旁人顺手开一条免密通道」。 -->
      <div class="set-row">
        <label class="set-label" for="set-bio-pw">{ui.t('settings.bio_enable_hint')}</label>
        <input
          id="set-bio-pw"
          class="pms-input set-ctl"
          type="password"
          autocomplete="current-password"
          bind:value={bioPassword}
          disabled={bioBusy}
        />
      </div>
      <div class="bio-actions">
        <button
          class="pms-btn is-primary is-sm"
          type="button"
          disabled={bioBusy || bioPassword.length === 0}
          onclick={() => void submitEnableBiometric()}
        >
          {#if bioBusy}<Spinner size={14} label={ui.t('common.loading')} />{/if}
          {ui.t('common.confirm')}
        </button>
        <button class="pms-btn is-sm" type="button" disabled={bioBusy} onclick={cancelEnableBiometric}>
          {ui.t('common.cancel')}
        </button>
      </div>
    {:else if bioEnabled}
      <button class="pms-btn is-ghost is-sm" type="button" disabled={bioBusy} onclick={() => void disableBiometric()}>
        {ui.t('settings.bio_disable')}
      </button>
    {:else}
      <!-- 平台不可用时置灰（§18.5 约束 7 的 UI 半边）；后端还会再兜一次。 -->
      <button
        class="pms-btn is-ghost is-sm"
        type="button"
        disabled={bioBusy || !bioAvailable}
        onclick={startEnableBiometric}
      >
        {ui.t('settings.bio_enable')}
      </button>
    {/if}
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
    <!-- D-110：复选框行标 `.is-check`——窄屏堆叠只适用含控件的行，复选框行必须登记行类型覆盖回来。 -->
    <div class="set-row is-check">
      <span class="set-label">{ui.t('settings.auto_sync')}</span>
      <label class="pms-check"><input type="checkbox" bind:checked={draft.auto_sync} /></label>
    </div>
    <div class="set-row is-check">
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
      <p class="pms-meta set-mono">{fileInfo.file_name}</p>
      <p class="pms-meta">
        {ui.t('settings.size')}: {formatBytes(fileInfo.size_bytes)}
      </p>
    {:else if fileInfoFailed}
      <p class="pms-hint">{ui.t('settings.vault_file_failed')}</p>
    {:else}
      <p class="pms-meta">{ui.t('common.loading')}</p>
    {/if}

    <!-- D-76：体积构成 + 回收入口。主人的原话是「把 2.4M 的附件删除后，设置里还是 24M，
         是不是被删除的附件都还保存在库里？」——所以这里首先**回答**那 24MB 是怎么来的，
         再提供动作。「已删附件占」排在按钮之前，不做小字脚注。
         两行都走 `.set-row.is-info`：它们是纯信息行，窄屏必须避开 `.set-row` 的堆叠规则（§6.10）。 -->
    <!-- 体积构成读不到时**不复用** `settings.vault_file_failed`：那句话的主语是「保险库文件大小」，
         安到这里会把「路径/大小读不到」的结论错贴到「体积构成读不到」上（失败归因错位）。
         也不能留白、更不能说「没有可回收的空间」——后者是结论，而我们此刻并不知道。 -->
    {#if compactionStatsFailed}
      <p class="pms-hint">{ui.t('settings.compact_stats_failed')}</p>
    {:else if compactionStats}
      {#if deadAttachmentBytes > 0}
        <div class="set-row is-info">
          <span class="pms-meta">
            {ui.t('settings.compact_dead_attachments', { size: formatBytes(deadAttachmentBytes) })}
          </span>
        </div>
      {/if}
      {#if canCompact}
        <div class="set-row is-info">
          <span class="pms-meta">
            {ui.t('settings.compact_reclaimable', { size: formatBytes(reclaimableBytes) })}
          </span>
        </div>
        <!-- `reclaimable_bytes === 0` 时**不渲染按钮**（不是 disabled）：点了没反应的按钮
             就是假功能，而「没有可回收的空间」才是真结论（见下面的 {:else} 分支）。 -->
        <button
          class="pms-btn is-ghost"
          type="button"
          disabled={compacting || syncBusy}
          onclick={() => (compactConfirmOpen = true)}
        >
          {#if compacting}<Spinner size={15} />{ui.t('settings.compacting')}{:else}<Icon
              name="file"
              size={15}
            />{ui.t('settings.compact')}{/if}
        </button>
        {#if syncBusy}
          <p class="pms-hint">{ui.t('settings.compact_wait_sync')}</p>
        {/if}
      {:else}
        <p class="pms-hint">{ui.t('settings.compact_none')}</p>
      {/if}
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
    <!-- D-107：更新检查（自动检查默认关 + 手动按钮；只提醒不下载，去 Releases 手动获取）。 -->
    <div class="set-row is-check">
      <span class="set-label">{ui.t('settings.update_auto')}</span>
      <label class="pms-check">
        <input type="checkbox" checked={updateEnabled} onchange={(e) => void onUpdateCheckToggle(e)} />
      </label>
    </div>
    <p class="pms-meta set-update-hint">{ui.t('settings.update_auto_hint')}</p>
    {#if updateState === 'available'}
      <p class="pms-alert is-warning">
        <Icon name="alert" size={16} />
        <span>{ui.t('settings.update_available', { version: updateLatest ?? '' })}</span>
      </p>
      <div class="set-row is-actions">
        <button class="pms-btn is-ghost is-sm" type="button" onclick={() => void openReleases()}>
          <Icon name="external" size={14} />{ui.t('settings.update_download')}
        </button>
      </div>
    {:else if updateState === 'checking'}
      <p class="pms-meta">{ui.t('settings.update_checking')}</p>
    {:else if updateState === 'uptodate'}
      <p class="pms-meta">{ui.t('settings.update_uptodate')}</p>
    {:else if updateState === 'failed'}
      <p class="pms-hint">{ui.t('settings.update_failed')}</p>
    {/if}
    <div class="set-row is-actions">
      <button
        class="pms-btn is-ghost is-sm"
        type="button"
        disabled={updateState === 'checking' || appVersion === null}
        onclick={() => void onUpdateCheckNow()}
      >
        <Icon name="sync" size={14} />{ui.t('settings.update_check_now')}
      </button>
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

<!-- D-86：Modal 移进 ImportWizard（「确认导入」常驻 footer，不随内容滚动）——
     这里只传弹窗开关与关闭回调，footer 由 ImportWizard 内部按 preview 状态渲染。 -->
<ImportWizard open={importOpen} onclose={() => (importOpen = false)} />

<!-- D-76：压缩二次确认。用 `ui.confirm()` 不可行 —— 它只支持「标题 + 一句正文」，
     装不下 §6.11.3 要求的四个数字 + 三条提示（三条提示缺一不可，各有一个误解要挡）。
     这里的行用局部 `.compact-fact` 而不是 `.set-row`：确认块在 560px 弹窗内，
     不应受设置页窄屏堆叠规则（§6.10）影响。 -->
<Modal
  open={compactConfirmOpen}
  titleKey="settings.compact_confirm_title"
  onclose={() => (compactConfirmOpen = false)}
>
  <div class="compact-facts">
    <div class="compact-fact">
      <span class="set-label">{ui.t('settings.compact_label_before')}</span>
      <span class="pms-meta compact-num">{fileInfo ? formatBytes(fileInfo.size_bytes) : '—'}</span>
    </div>
    <div class="compact-fact">
      <span class="set-label">{ui.t('settings.compact_label_after')}</span>
      <span class="pms-meta compact-num">{formatBytes(compactionStats?.after_bytes ?? 0)}</span>
    </div>
    <div class="compact-fact">
      <span class="set-label">{ui.t('settings.compact_label_reclaim')}</span>
      <span class="pms-meta compact-num">{formatBytes(reclaimableBytes)}</span>
    </div>
    <div class="compact-fact">
      <span class="set-label">{ui.t('settings.compact_label_dead')}</span>
      <span class="pms-meta compact-num">{formatBytes(deadAttachmentBytes)}</span>
    </div>
  </div>
  <ul class="compact-hints">
    <li class="pms-hint">{ui.t('settings.compact_hint_no_password')}</li>
    <li class="pms-hint">{ui.t('settings.compact_hint_trash')}</li>
    <li class="pms-hint">{ui.t('settings.compact_hint_disk')}</li>
  </ul>
  {#snippet footer()}
    <button class="pms-btn is-ghost" type="button" onclick={() => (compactConfirmOpen = false)}>
      {ui.t('common.cancel')}
    </button>
    <button class="pms-btn is-primary" type="button" onclick={() => void runCompaction()}>
      {ui.t('settings.compact_start')}
    </button>
  {/snippet}
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
  /* D-113 P2：快速解锁状态文字（信息行右侧）。
     刻意**不用** `.set-ctl`：那有 160px 固定宽，而「主密码已修改，快速解锁已失效。」这类
     句子需要自然换行；再加 160px 会让它在窄屏挤成竖条。 */
  .bio-state {
    max-width: 62%;
    text-align: right;
    font-size: 0.86rem;
    color: var(--pms-text);
  }
  .bio-actions {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.6rem;
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
  /* D-107：更新自动检查的说明行（小字提示，紧跟开关行）。 */
  .set-update-hint {
    margin: 0.1rem 0 0.4rem;
  }
  /* D-76：压缩确认块的「标签 / 数值」两列 + 三条提示。只做布局 ——
     字号与颜色一律继承 `.set-label` / `.pms-meta` / `.pms-hint`（§5.7.4 的同一纪律：
     组件样式里不许自写字体与配色）。 */
  .compact-fact {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.15rem 0;
  }
  /* 数字用等宽，几行数值右缘对齐、便于上下对比。 */
  .compact-num {
    flex: 0 0 auto;
    font-family: var(--pms-mono);
  }
  .compact-hints {
    list-style: disc;
    margin: 0.8rem 0 0;
    padding-left: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .compact-hints .pms-hint {
    margin: 0;
  }
  /* 移动端：每行由「label 左 / 控件右」改为上下堆叠，控件占满整宽，避免 160px 固定宽控件 + 长文案溢出。
     ⚠️ 下面 `.set-row` 的堆叠**只适用于含控件的行**（`.set-ctl` / 输入 / 分段控件）。
     纯信息行必须加 `.is-info`、按钮行必须加 `.is-actions`、复选框行必须加 `.is-check`（D-110）在下方覆盖回来（同层叠靠源码顺序取胜）。 */
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
    /* D-110：复选框行（自动同步 / 启动同步 / 更新检查）——窄屏保持「文字左 / 复选框右」。
       不覆盖的话复选框被 column 堆到文字下一行，孤零零一个方块（主人手机实测）。
       桌面端基础 .set-row 已是横排，无需桌面规则。 */
    .set-row.is-check {
      flex-direction: row;
      align-items: center;
      justify-content: space-between;
      gap: 0.75rem;
    }
  }
</style>
