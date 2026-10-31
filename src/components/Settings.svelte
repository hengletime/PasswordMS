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
  import { AUTO_LOCK_OPTIONS, formatBytes, formatDateTime } from '../lib/constants';
  import {
    BACKUP_STALE_DAYS,
    recordBackup,
    lastBackup,
    shouldPromptBackup,
    type BackupRecord,
    type BackupStorage,
  } from '../lib/backup-reminder';
  import {
    bioCanOpenSettings,
    bioPlatformOf,
    bioShouldOfferSettings,
    bioTextSuffix,
    localizeBioMessageKey,
  } from '../lib/bio-platform';
  import { toAppError } from '../lib/errors';
  import { comboFromEvent, formatHotkey } from '../lib/hotkey-combo';
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
    pendingUpdateVersion,
    setAutoCheckEnabled,
    setPendingUpdate,
    shouldAutoCheck,
    type UpdateCheckStorage,
  } from '../lib/update-check';
  import { runVaultExport } from '../lib/vault-export';
  import { isSafeUrl } from '../lib/url-safety';
  import type { CompactionStatsDto, PrefsDto, VaultFileInfoDto, VaultSettings } from '../types';

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
  /**
   * D-115：开启流程的失败文案，**就地**显示在输入框下方。
   *
   * 原先只 `ui.toast('error', …)`：主密码输错时 toast 在右下角，而用户的注意力在
   * 输入框上；而且文案是 `err.unlock_failed`「解锁失败，请重试」——人在设置页里，
   * 却被告知「解锁失败」，等于把「密码错了」翻译成一句听不懂的话。
   */
  let bioErrorKey = $state('');
  const bio = $derived(session.biometric);
  const bioAvailable = $derived(bio?.availability === 'available');
  const bioEnabled = $derived(bio?.state === 'ready');
  /**
   * D-115：**已开启但平台当前不可用**（封印完好、平台不可用）。
   *
   * `SealState::Ready` 只说明 `biometric.json` 结构完好且 bind 自洽，**不**说明
   * Windows Hello 现在能用。两个事实可以同时成立（主人真机截图：状态「已开启」
   * 配红字「未检测到可用的 Windows Hello」）。状态行必须如实反映这个组合，
   * 否则用户看到的是自相矛盾的两句话。
   */
  const bioDegraded = $derived(bioEnabled && bio !== null && !bioAvailable);
  /**
   * D-121（B 案）：系统生物识别设置入口的显示条件。
   *
   * 只在「有事可做」时出现：平台不可用（该去开通），或封印已失效（D-119 的墓碑，
   * 说明系统里的指纹 / 人脸记录变过了）。一切正常时不打扰。
   *
   * **必须用独立判据，不能再嵌在 `!bioAvailable` 里**：指纹变更导致的失效恰好发生在
   * `availability = available` 的时候（平台好好的，只是那把钥匙废了），
   * 嵌进去就永远走不到——主人真机找不到这个按钮正是这个原因。
   */
  const bioSealBroken = $derived(bio?.state === 'stale' || bio?.state === 'corrupt');
  const bioShowSettingsEntry = $derived(bio !== null && (!bioAvailable || bioSealBroken));
  /**
   * §23：平台名词与平台专属文案的后缀。
   *
   * Windows 沿用无后缀的键，Android 用 `_android` 那一套——**不能只换名词就完事**：
   * 两个平台的设置路径（「登录选项」vs「指纹、人脸与密码」）和风险模型都不同。
   * 通用句（「本机尚未设置 {noun}」这类）走 `bioNoun` 插值，避免每种语言写两遍。
   */
  const bioPlatform = $derived(bioPlatformOf(bio));
  const bioSuffix = $derived(bioTextSuffix(bioPlatform));
  const bioNoun = $derived(ui.t(`settings.bio_noun${bioSuffix}`));
  let oldPassword = $state('');
  // D-51：导出保险库需要单独输入当前主密码（后端 repo.export_vault 会用它做 unlock_check 后再 copy 库文件）。
  // 旧实现误用「修改主密码」卡片的 oldPassword，而导出卡片根本没有输入框 → 空密码必报「解锁失败，请重试。」
  let exportPassword = $state('');
  let newPassword = $state('');
  let newPassword2 = $state('');
  /**
   * D-114：改密结果的**常驻**回执（`none` = 未发生）。
   *
   * 原实现成功后只 `ui.toast('success', 'common.ok')` —— 文案是按钮标签「确定」
   * （等于什么也没说），3 秒后自动消失，主人实测「右下角一闪而过，看不出成没成」。
   * 改密是低频高危操作，结果必须留在原地，直到用户下一次动手。
   *
   * `bio_off` 这一档对应第二个缺陷：命令层在改密成功后按 §18.5 约束 2 删掉快速解锁
   * 封印，前端既不重查也不解释 → 用户只看到「快速解锁悄悄没了」。所以这里要区分
   * 「改密前是否开着」，才能如实说出它为什么没了。
   */
  let pwdChanged = $state<'none' | 'plain' | 'bio_off'>('none');
  /** D-115：改密失败的就地回执（与成功回执同一套清除机制）。 */
  let pwdErrorKey = $state('');
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

  // ===== v1.4.10：系统集成（全局热键 / 开机自启） =====
  //
  // 这两项存在**应用级配置**里（`prefs.json`），不随保险库加密——锁屏态与冷启动就要可用。
  // 与 `settings.value` 完全独立：不要把它们塞进 `VaultSettings`，那会让"锁着的时候
  // 热键注册不出来"，而锁屏正是最想按热键的时刻。

  /** 应用级配置；`null` = 尚未加载或读取失败（看 `prefsFailed` 区分）。 */
  let prefsDto = $state<PrefsDto | null>(null);
  /** `true` = 读取失败（UI 必须显示"读取失败"，不能渲染成"未设置"）。 */
  let prefsFailed = $state(false);
  /** 是否正在录制新热键（监听键盘）。 */
  let hotkeyRecording = $state(false);

  /** 热键显示文案；未设置时明确写"未设置"而不是留白。 */
  const hotkeyDisplay = $derived(
    prefsDto && prefsDto.hotkey.length > 0
      ? formatHotkey(prefsDto.hotkey)
      : ui.t('settings.hotkey_none'),
  );

  /** 读应用级配置。独立 try/catch：它失败不该影响四块旁路数据里的任何一块。 */
  async function loadPrefs(): Promise<void> {
    try {
      prefsDto = await api.prefs.get();
      prefsFailed = false;
    } catch {
      prefsFailed = true;
    }
  }

  /** 开始录制：把按钮切到等待态，实际的按键由下面的 window 监听接住。 */
  function startHotkeyRecord(): void {
    hotkeyRecording = true;
  }

  /**
   * 录制期间接管键盘。
   *
   * 用 `capture` + `preventDefault`：否则用户按 Ctrl+P 会先被 WebView 当成打印、
   * Ctrl+S 当成保存，按键被吃掉导致"录不上"。
   * 用 `$effect` 管注册/注销，避免组件被销毁时（切到别的分区）留下一个野监听。
   */
  $effect((): (() => void) | void => {
    if (!hotkeyRecording) {
      return;
    }
    const onKeydown = (event: KeyboardEvent): void => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === 'Escape') {
        hotkeyRecording = false;
        return;
      }
      const combo = comboFromEvent(event);
      if (combo === null) {
        // 还只是按下了修饰键，或者没有主修饰键——继续等，不报错。
        return;
      }
      void saveHotkey(combo);
    };
    window.addEventListener('keydown', onKeydown, true);
    return (): void => window.removeEventListener('keydown', onKeydown, true);
  });

  /** 保存热键。失败时**保持录制态**，用户直接再按一个组合即可——省一次点击。 */
  async function saveHotkey(spec: string): Promise<void> {
    try {
      prefsDto = await api.prefs.setHotkey(spec);
      hotkeyRecording = false;
      ui.toast('success', 'settings.hotkey_saved');
    } catch (err) {
      // ERR_HOTKEY_UNAVAILABLE（被占用）/ ERR_HOTKEY_* （键位问题）都由后端给出具体文案。
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 开关开机自启。失败时重读真实状态，让勾选框回滚到系统的实际值。 */
  async function onAutostartToggle(event: Event): Promise<void> {
    const enabled = (event.target as HTMLInputElement).checked;
    try {
      prefsDto = await api.prefs.setAutostart(enabled);
      ui.toast('success', enabled ? 'settings.autostart_on' : 'settings.autostart_off');
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
      void loadPrefs();
    }
  }

  // ===== D-107：更新检查（方案 B：自动检查默认关 + 手动按钮；只提醒不下载） =====

  /** localStorage 抽象（测试可替换；运行环境恒为浏览器 WebView）。 */
  function updateStorage(): UpdateCheckStorage {
    return window.localStorage;
  }

  /** D-144/D-146：备份记录的存储（与更新检查共用 localStorage，键名各自独立）。 */
  function backupStorage(): BackupStorage | null {
    // SSR / 测试环境无 window 时不记录，而不是抛错——这是辅助信息，不值得中断导出。
    return typeof window === 'undefined' ? null : window.localStorage;
  }

  /** 上次成功导出的记录（进入设置页时读一次；导出成功后就地更新）。 */
  let lastBackupRecord = $state<BackupRecord | null>(null);

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
    } else if (result !== null) {
      // D-142：手动检查也要同步缓存。此前只在自动检查时写，于是「查到已最新」
      // 只改了内存态、缓存里那句旧提示还留着 —— 当场隐去，重进设置页又冒出来。
      setPendingUpdate(updateStorage(), result.hasUpdate ? result.latestVersion : null);
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
  // 上次检查查到过新版本的话，直接把提示摆出来（不必等下次检查）——
  // 但必须先用**本机当前版本**过一遍：升级后旧缓存不再构成更新，提示立即消失（D-142）。
  $effect((): void => {
    updateEnabled = isAutoCheckEnabled(updateStorage());
    if (appVersion !== null) {
      const pending = pendingUpdateVersion(updateStorage(), appVersion);
      if (pending !== null) {
        updateLatest = pending;
        updateState = 'available';
      }
    }
    if (shouldAutoCheck(updateStorage())) {
      void runUpdateCheck(true);
    }
    // D-144/D-146：每次进设置页都重新读一次备份记录。
    // 不放进上面那个 effect 的同一段判断里也可以，但这样只需一次挂载即可。
    const store = backupStorage();
    if (store !== null) {
      lastBackupRecord = lastBackup(store);
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
    // v1.4.10：系统集成配置（应用级，与上面四块互相独立）。
    void loadPrefs();
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
    bioErrorKey = '';
  }

  /** 取消开启：收起输入框并清掉刚输入的主密码（不留在内存里）。 */
  function cancelEnableBiometric(): void {
    bioAsking = false;
    bioPassword = '';
    bioErrorKey = '';
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
    bioErrorKey = '';
    try {
      await api.vault.biometricEnable(bioPassword);
      bioPassword = '';
      bioAsking = false;
      await session.refreshBiometric();
      // D-114：重新开启成功 → 上面那句「快速解锁已因此关闭」就不能再挂着（已经不成立了）。
      pwdChanged = 'none';
    } catch (err) {
      // D-115：主密码错（ERR_WRONG_MASTER_PASSWORD）/ 平台不可用 / 空库（ERR_VALIDATION）
      // 全部**就地**显示在输入框下——用户的眼睛在输入框上，不在右下角。
      // D-120：取消验证这类含平台名词的句子要按平台取词（安卓不认「Windows Hello」）。
      bioErrorKey = localizeBioMessageKey(toAppError(err).messageKey, bioPlatform);
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
   * 失效原因对应的文案键（§23）。
   *
   * 只有 `enrollment_changed`（生物特征变更）与平台绑定：Windows 说「Windows Hello 配置
   * 已变更」，Android 说「指纹 / 人脸记录已变更」。其余三条（改主密码 / 换 VK / 被篡改）
   * 是数据层原因，两个平台说法一致，不带后缀。
   */
  function bioStaleKey(reason: string): string {
    return reason === 'enrollment_changed'
      ? `settings.bio_state_stale_enrollment${bioSuffix}`
      : `settings.bio_state_stale_${reason}`;
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
    // 改密**会**在后端删掉快速解锁封印（§18.5 约束 2）。先记下改密前是否开着，
    // 成功后才能如实告诉用户「它为什么没了」（D-114）。
    const bioWasOn = bioEnabled;
    pwdErrorKey = '';
    try {
      await api.vault.changeMasterPassword(oldPassword, newPassword);
      oldPassword = '';
      newPassword = '';
      newPassword2 = '';
      // 重查而不是本地改值：封印是否真被删除以后端为准（前端不维护第二份真相）。
      await session.refreshBiometric();
      pwdChanged = bioWasOn ? 'bio_off' : 'plain';
      // toast 仍然给（新文案），但**不再是唯一回执**——它 3 秒就没了。
      ui.toast(
        'success',
        bioWasOn ? `settings.password_changed_bio_off${bioSuffix}` : 'settings.password_changed',
      );
    } catch (err) {
      // 失败时清掉上一次的成功回执：否则「已修改」会和「修改失败」同时在屏幕上。
      pwdChanged = 'none';
      // D-115：失败回执同样**留在原地**。同一张卡片上「成功常驻、失败只闪 3 秒」
      // 是自相矛盾的；而「旧密码输错」正是这条流程最常见的失败。
      // 错误码由 core 的 `WrongMasterPassword` 给出，文案不再借用「解锁失败」。
      pwdErrorKey = toAppError(err).messageKey;
    }
  }

  /** D-115：三个密码框任一改动 → 清掉上一条回执（成功与失败都清）。 */
  function clearPwdReceipt(): void {
    pwdChanged = 'none';
    pwdErrorKey = '';
  }

  /**
   * D-114：一键跳到系统「登录选项」。
   *
   * 为什么不复用 `api.system.openUrl`：那条链路只放行 http/https（PRD F-05 不变量），
   * `ms-settings:` 打不开；为它放宽等于把「打开链接」变成「打开任意系统协议」。
   * 所以走一条**无参数**的专用窄命令，目标 URI 在 Rust 里是常量。
   */
  async function openSignInOptions(): Promise<void> {
    try {
      await api.vault.openBiometricSettings();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /**
   * D-114：窗口重新获得焦点时重查快速解锁状态。
   *
   * 用户被引导去系统设置开通 Windows Hello 后**必然**会切回来；不重查的话卡片还停在
   * 「未检测到」，等于刚做完的事没被承认（这正是 #3 那个困惑的另一半）。
   * 顺带覆盖「用户在系统里关掉了 Hello」：状态会跟着变，而不是一直显示可用。
   */
  $effect(() => {
    if (typeof window === 'undefined') {
      return;
    }
    const onFocus = (): void => {
      // 正在开启/关闭时不插一脚（会与按钮上的 busy 态打架）。
      if (!bioBusy) {
        void session.refreshBiometric();
      }
    };
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  });

  async function exportVault(): Promise<void> {
    // D-51 / Q-1：守卫与流程抽到 src/lib/vault-export.ts（可被 vitest 行为级验证），
    // 这里只注入真实依赖；`exportPassword` 为空时该模块不发 IPC、就地提示。
    const result = await runVaultExport({
      exportPassword,
      confirm: (): Promise<boolean> =>
        ui.confirm('settings.export_confirm_title', 'settings.export_confirm_body'),
      pickDestination: (): Promise<string | null> =>
        save({ defaultPath: 'passwordms-export.vault' }),
      exportVault: (dest: string, password: string): Promise<number> =>
        api.vault.exportVault(dest, password),
      toast: (kind, messageKey, params): void => {
        ui.toast(kind, messageKey, params);
      },
    });
    if (result.cleared) {
      exportPassword = ''; // 导出成功后清空，避免明文主密码残留在组件状态里
    }
    // D-144：只认「真的写进去了多少字节」——记录后卡片上会显示时间与体积，
    // 下次想确认「我到底备份成功过没有」不必凭记忆。
    if (result.bytes !== null && result.bytes > 0) {
      const storage = backupStorage();
      if (storage !== null) {
        recordBackup(storage, result.bytes, Date.now());
      }
      lastBackupRecord = { atMs: Date.now(), bytes: result.bytes };
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

  <!-- v1.4.10：系统集成（全局热键 / 开机自启）。
       为什么单独成卡：这两项属于「本机怎么用这个程序」，既不进保险库、也不随库迁移，
       和上面外观/安全的语义不同；混进去会让用户以为换台机器也会带着走。 -->
  {#if prefsDto === null && !prefsFailed}
    <!-- 配置还没读到：先不渲染，避免卡片"闪现又消失" -->
  {:else if prefsDto && !prefsDto.supported}
    <!-- 移动端没有托盘/全局热键/自启：整卡不渲染（D-116 口径，不给不存在的功能做广告） -->
  {:else}
  <div class="pms-card set-card">
    <h3>{ui.t('settings.group_system')}</h3>
    <div class="set-row">
      <span class="set-label">{ui.t('settings.hotkey')}</span>
      <button
        class="pms-btn is-sm set-ctl"
        class:is-primary={hotkeyRecording}
        type="button"
        aria-label={ui.t('settings.hotkey')}
        onclick={startHotkeyRecord}
      >
        {hotkeyRecording ? ui.t('settings.hotkey_recording') : hotkeyDisplay}
      </button>
    </div>
    <p class="pms-hint">{ui.t('settings.hotkey_hint')}</p>
    {#if prefsFailed}
      <!-- 读取失败 ≠ 未设置：必须显式说出来（D-73 的教训）。 -->
      <p class="pms-alert is-warning">
        <Icon name="alert" size={16} />
        <span>{ui.t('settings.prefs_load_failed')}</span>
      </p>
    {:else if prefsDto && prefsDto.hotkey.length > 0 && !prefsDto.hotkey_ok}
      <!-- 已设置却没注册上：大概率被别的程序占了，直接告诉用户怎么修。 -->
      <p class="pms-alert is-warning">
        <Icon name="alert" size={16} />
        <span>{ui.t('settings.hotkey_unavailable')}</span>
      </p>
    {/if}
    <div class="set-row is-check">
      <span class="set-label">{ui.t('settings.autostart')}</span>
      <label class="pms-check">
        <input
          type="checkbox"
          checked={prefsDto?.autostart ?? false}
          onchange={(e) => void onAutostartToggle(e)}
        />
      </label>
    </div>
    <p class="pms-hint">{ui.t('settings.autostart_hint')}</p>
  </div>
  {/if}

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

  <!-- D-113 P2：快速解锁（Windows Hello）。P3 起 Android 也接上了（Keystore）。
       独立卡片而不是塞进「安全」分组：它是**立即生效的动作**，不归底部「保存」按钮管；
       混进安全卡片会让用户以为改完要按保存才生效。

       D-116：**本平台没有适配器时整块不渲染**。安卓 v1.3.1 曾显示
       「Windows Hello 快速解锁 · 未开启 · 当前平台不支持」——把 Windows 专属名词搬到
       安卓，还给一个当时还没做的功能挂了块「不支持」的牌子（主人真机实测）。
       §23（v2.40）落 P3 后，判据从「有没有适配器」变成「是不是 Windows / Android」：
       这两个平台都渲染，名词与风险说明走 `bioSuffix`（Windows 无后缀 / Android `_android`）。
       只有 macOS / Linux 这类真没接的平台才落进 `not_implemented`。
       `bio === null` 表示状态还没读到/读失败，同样不渲染：宁可晚一帧出现，也不谎报「未开启」。
       **不要**把这条改成「显示但置灰」——那是在为不存在的功能做广告。 -->
  {#if bio && bio.availability !== 'not_implemented'}
  <div class="pms-card set-card">
    <h3>{ui.t('settings.group_biometric')}</h3>
    <!-- 信息行用 `.set-row.is-info`（§6.10）：窄屏堆叠只针对控件行，信息行不能拆两行。 -->
    <div class="set-row is-info">
      <span class="set-label">{ui.t(`settings.bio_title${bioSuffix}`)}</span>
      <span class="bio-state" class:is-warn={bioDegraded}>
        {#if bioDegraded}
          <!-- D-115：封印完好但平台不可用时，只说「已开启」会与下面的红字打架。 -->
          {ui.t('settings.bio_state_on_unavailable', { date: bioEnabledDate() })}
        {:else if bioEnabled}
          {ui.t('settings.bio_state_on', { date: bioEnabledDate() })}
        {:else if bio?.state === 'corrupt'}
          {ui.t('settings.bio_state_corrupt')}
        {:else if bio?.state === 'stale' && bio.reason}
          {ui.t(bioStaleKey(bio.reason))}
        {:else}
          {ui.t('settings.bio_state_off')}
        {/if}
      </span>
    </div>
    <p class="pms-hint">{ui.t(`settings.bio_desc${bioSuffix}`)}</p>
    <p class="pms-hint">{ui.t(`settings.bio_risk${bioSuffix}`)}</p>
    {#if bio && !bioAvailable}
      <p class="pms-hint is-danger">
        {ui.t(`settings.bio_avail_${bio.availability}`, { noun: bioNoun })}
      </p>
      <!-- D-114：`not_present` 的真实含义多半是「还没开通 Hello」，光说「检测不到」等于
           把用户留在原地。给一句明确的下一步。
           D-122：这条路只对 Windows 成立（有可点的按钮），Android 的说明统一由下面
           那个 `bio_manage_hint_android` 承担，不在这里重复说第二遍。 -->
      {#if bioShouldOfferSettings(bio.availability) && bioCanOpenSettings(bioPlatform)}
        <p class="pms-hint">{ui.t('settings.bio_avail_hint')}</p>
      {/if}
    {/if}
    <!-- D-121（B 案）：入口只在**有事可做**时出现——平台不可用（去开通），或封印已失效
         （去看看系统里增删了哪些指纹）。一切正常时不打扰：那不是问题现场。
         判据单独提到两个信息块之外，因为「失效」这次可能发生在平台完全可用的时候
         （D-119 的墓碑：`availability = available` 而 `state = stale`），
         条件若还挂在 `!bioAvailable` 里就永远走不到——主人真机就是这么找不到它的。

         D-122：Android 上**没有按钮**，只有一句话。`ACTION_BIOMETRIC_ENROLL` 在国产
         ROM 上各家响应不一，主人真机点击完全静默（startActivity 不抛异常 ⇒ Intent 发出
         去了、被 ROM 忽略或秒退），既跳不过去也没法报错。与其留一个点了没反应的按钮，
         不如把「去哪个大类里找」说清楚——各品牌路径不同，用户自己两步就到。 -->
    {#if bioShowSettingsEntry}
      {#if bioCanOpenSettings(bioPlatform)}
        <button
          class="pms-btn is-ghost is-sm"
          type="button"
          onclick={() => void openSignInOptions()}
        >
          {ui.t('settings.bio_open_settings')}
        </button>
      {:else}
        <p class="pms-hint">{ui.t('settings.bio_manage_hint_android')}</p>
      {/if}
    {/if}
    {#if bioDegraded}
      <!-- D-115：状态行写着「已开启」而这里说「用不了」，必须解释清楚为什么，
           以及「要彻底撤销该怎么做」——否则用户会以为关掉 Hello 就等于撤销。 -->
      <p class="pms-hint">{ui.t(`settings.bio_degraded_hint${bioSuffix}`)}</p>
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
          oninput={() => (bioErrorKey = '')}
        />
      </div>
      {#if bioErrorKey !== ''}
        <!-- D-115：就地显示。主密码输错时用户的眼睛在输入框上，不在右下角；
             而且原来的文案是「解锁失败，请重试」——人在设置页里，这话听不懂。 -->
        <p class="pms-hint is-danger" role="alert">{ui.t(bioErrorKey)}</p>
      {/if}
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
  {/if}

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
    <!-- D-143：卸载/换机即清库的常驻提醒。放在卡片最顶部（在「导出的是加密副本」说明之前），
         因为它是**为什么要导出**的理由，用户先读到它才会往下读怎么导。
         文案跨平台通用：桌面卸载/换机同样需要导出，不做平台判定（也就不依赖 UA）。 -->
    <p class="pms-alert is-danger">
      <Icon name="alert" size={16} />
      <span>{ui.t('settings.uninstall_wipes_vault')}</span>
    </p>
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
    <!-- D-144/D-146：把「上一次备份到底成功了吗、写进去多少」变成一行可见事实。
         2026-10-31 的事故里，用户唯一的反馈是「确定」，0 字节的文件没人发现；
         有了这行，体积与时间都摆在眼前，`0 B` 或「从未导出」无处可藏。 -->
    {#if lastBackupRecord !== null}
      <p class="pms-hint" class:is-danger={shouldPromptBackup(lastBackupRecord, Date.now())}>
        {ui.t('settings.backup_last_ok', {
          time: formatDateTime(lastBackupRecord.atMs, ui.locale),
          size: formatBytes(lastBackupRecord.bytes),
        })}
        {#if shouldPromptBackup(lastBackupRecord, Date.now())}
          {ui.t('settings.backup_stale_hint', { days: BACKUP_STALE_DAYS })}
        {/if}
      </p>
    {:else}
      <p class="pms-hint is-danger">{ui.t('settings.backup_never')}</p>
    {/if}
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
      <input id="set-pw-old" class="pms-input set-ctl" type="password" bind:value={oldPassword} autocomplete="current-password"
        oninput={clearPwdReceipt} />
    </div>
    <div class="set-row">
      <label class="set-label" for="set-pw-new">{ui.t('settings.change_password_new')}</label>
      <input id="set-pw-new" class="pms-input set-ctl" type="password" bind:value={newPassword} autocomplete="new-password"
        oninput={clearPwdReceipt} />
    </div>
    <div class="set-row">
      <label class="set-label" for="set-pw-new2">{ui.t('create.confirm_password')}</label>
      <input id="set-pw-new2" class="pms-input set-ctl" type="password" bind:value={newPassword2} autocomplete="new-password"
        oninput={clearPwdReceipt} />
    </div>
    <button class="pms-btn" type="button" onclick={() => void changePassword()}>{ui.t('common.confirm')}</button>
    {#if pwdErrorKey !== ''}
      <!-- D-115：失败同样是常驻回执。同一张卡片上「成功留 10 分钟、失败只闪 3 秒」
           说不通，而「旧密码输错」正是这条流程最常见的失败。 -->
      <p class="pms-alert is-danger pwd-result" role="alert">
        <Icon name="alert" size={16} />
        <span>{ui.t(pwdErrorKey)}</span>
      </p>
    {/if}
    {#if pwdChanged !== 'none'}
      <!-- D-114：结果留在原地。toast 三秒就消失，不能作为改密这种高危操作唯一的回执；
           `role="status"` 让读屏也念出来。`bio_off` 顺带解释了「快速解锁为什么没了」。 -->
      <p class="pms-alert is-success pwd-result" role="status">
        <Icon name="check" size={16} />
        <span>{ui.t(pwdChanged === 'bio_off' ? `settings.password_changed_bio_off${bioSuffix}` : 'settings.password_changed')}</span>
      </p>
    {/if}
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
        class="pms-btn is-ghost is-sm set-about-btn"
        type="button"
        disabled={updateState === 'checking' || appVersion === null}
        onclick={() => void onUpdateCheckNow()}
      >
        <Icon name="sync" size={14} />{ui.t('settings.update_check_now')}
      </button>
      <button class="pms-btn is-ghost is-sm set-about-btn" type="button" onclick={() => void openHomepage()}>
        <Icon name="external" size={14} />{ui.t('settings.about_homepage')}
      </button>
      <button class="pms-btn is-ghost is-sm set-about-btn" type="button" onclick={() => openLegal('disclaimer')}>
        {ui.t('settings.about_disclaimer')}
      </button>
      <button class="pms-btn is-ghost is-sm set-about-btn" type="button" onclick={() => openLegal('terms')}>
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
  /* D-115：已开启但平台当前不可用（`SealState::Ready` + `availability != available`）。 */
  .bio-state.is-warn {
    color: var(--pms-warning);
  }
  .bio-actions {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.6rem;
  }
  /* D-114：改密成功的常驻结果条（`.pms-alert.is-success` 的底色在 global.css）。 */
  .pwd-result {
    margin-top: 0.75rem;
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
    /* D-149：「关于」分组的四个按钮（检查更新 / 项目主页 / 免责声明 / 用户条款）
       390px 上换行后落成 3+1 的参差排布，看着像两组不相干的东西。给这组按钮标
       `.set-about-btn`，窄屏排成两列等宽 —— 2×2 是唯一能一眼看出「四个同类按钮」的排法。
       刻意**不**给 `.set-row` 加新变体：`is-info` / `is-actions` / `is-check` 是行类型
       三分（D-73），多一个变体就让「控件行必须继续走窄屏堆叠」的守门失去判据。 */
    .set-row.is-actions .set-about-btn {
      flex: 1 1 calc(50% - 0.25rem);
      justify-content: center;
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
