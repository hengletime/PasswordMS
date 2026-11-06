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
  import {
    AUTO_LOCK_OPTIONS,
    CLIPBOARD_TTL_MAX,
    CLIPBOARD_TTL_MIN,
    HEALTH_STALE_MAX,
    HEALTH_STALE_MIN,
    HEALTH_WEAK_MAX,
    HEALTH_WEAK_MIN,
    formatBytes,
    formatDateTime,
  } from '../lib/constants';
  import { clampClipboardTtl, clampHealthSettings } from '../lib/settings-clamp';
  import {
    backupHints,
    restorePointKindKey,
    splitPoints,
    sumBytes,
  } from '../lib/backup-hints';
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
    bioStaleStateKey,
    bioTextSuffix,
    localizeBioMessageKey,
  } from '../lib/bio-platform';
  import { ErrorCode, toAppError } from '../lib/errors';
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
  import type {
    BackupPointDto,
    BackupStatusDto,
    CompactionStatsDto,
    PrefsDto,
    VaultFileInfoDto,
    VaultSettings,
  } from '../types';

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
  /**
   * 自动输入热键设置的平台门控（1.5.39；D-121/D-122 同一口径）：判据独立成派生态，
   * 不嵌进别的 if——嵌进去的判据会在条件短路时永远走不到。
   * 平台来源是后端 DTO 的 `platform` 字段（`bioPlatformOf`），**禁止 navigator.userAgent**。
   * Windows 才渲染改键控件；Android 只渲染纯文案，不渲染 `<button>`——移动端没有
   * 全局热键这回事，挂一个录不上的按钮等于给不存在的功能做广告。
   */
  const autotypeHotkeyEditable = $derived(bioPlatform === 'windows');
  const autotypeHotkeyTextOnly = $derived(bioPlatform === 'android');
  let oldPassword = $state('');
  // D-51：导出保险库需要单独输入当前主密码（后端 repo.export_vault 会用它做 unlock_check 后再 copy 库文件）。
  // 旧实现误用「修改主密码」卡片的 oldPassword，而导出卡片根本没有输入框 → 空密码必报「解锁失败，请重试。」
  let exportPassword = $state('');
  let newPassword = $state('');
  let newPassword2 = $state('');
  /**
   * D-114：改密结果的**常驻**回执（`none` = 未发生）。
   *
   * 原实现成功后只弹一条成功 toast，文案键是 common.ok —— 那是按钮标签「确定」
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

  // ===== B-03（1.5.29）：自动回滚点状态 =====
  //
  // 与上面 `lastBackupRecord`（用户**导出**的记录，存 localStorage）是两件事：
  // 导出的是能带走、换机可用的文件；回滚点只在本机，卸载即消失。
  // 分开存、分开显示，且文案必须各自带边界——同名不同物是本项目踩过的坑。
  let backupStatus = $state<BackupStatusDto | null>(null);
  let backupLoadFailed = $state(false);
  /**
   * 备份清单是否展开（D-184）。
   *
   * 默认**收起**。主人 2026-11-04 原话：「备份这个卡片在设置里占了很大地盘，
   * 能不能平常不用时是缩在上面，只要有几行提示……用户需要时可点击按钮向下展开。」
   * 摊开时这张卡约 1000px（两张表 6 行 + 两个组头 + 自动清理块），收起后约 180px。
   *
   * **故意不做持久化**：每次都收起来，"平常不用就缩着"才成立；真要看的人
   * 点一下是一秒的事，而记住"上次展开"会让这张卡隔三差五又占满一屏。
   */
  let backupExpanded = $state(false);
  /** 收起态的摘要：几份、多大、其中几份要用保存时的密码（D-184）。 */
  const backupSummary = $derived.by(() => {
    const points = backupStatus?.points ?? [];
    return {
      count: points.length,
      // **必须过 `formatBytes`**：`restore_point_summary` 的 `{size}` 是给"人看的体积"，
      // 不是字节数。D-186 之前这里直接塞了 `sumBytes(points)`，界面上印出
      // 「共 4 份（519484）」——同一张卡片的表头却写着「507.3 KB」，自相矛盾。
      size: formatBytes(sumBytes(points)),
      old: points.filter((p) => p.needs_old_password).length,
    };
  });
  /**
   * 两张表：现在的密码能打开 / 改密码之前。
   *
   * 分组判据是 `needs_old_password`（后端判的），**不在这里从 kind 或文件名再推一遍**
   * ——两份判据一有分歧，卡片就会自相矛盾（标题说这组用现在的密码、行里说要用旧密码）。
   * 见 `lib/backup-hints.ts` 的 `splitPoints`。
   */
  const pointGroups = $derived(splitPoints(backupStatus?.points ?? []));

  /**
   * 两张表的描述。抽成数据是为了**表格只写一遍**：两组除了标题和说明没有区别，
   * 抄两遍的话下一次加一列就会只加到一半（而模板没法用测试逼着"两处一致"）。
   *
   * `noteKey` 用空串表示"这一组不需要额外说明"，而不是 `null`：
   * 模板里 `{#if table.noteKey !== ''}` 比 `!== null` 少一次类型收窄。
   */
  const pointTables = $derived([
    {
      group: 'current' as const,
      rows: pointGroups.current,
      titleKey: 'settings.restore_point_group_current',
      noteKey: '',
    },
    {
      group: 'older' as const,
      rows: pointGroups.older,
      titleKey: 'settings.restore_point_group_older',
      noteKey: 'settings.restore_point_old_note',
    },
  ]);
  /** 待确认的删除目标（`null` = 弹窗关闭）。 */
  let backupClearTarget = $state<
    | {
        kind: 'group';
        /** 哪一组（决定确认文案：删旧代要提醒"那把密码在本机没有别的副本了"）。 */
        group: 'current' | 'older';
        count: number;
        bytes: number;
      }
    | {
        kind: 'point';
        /** 与「退回去」同一个标识；后端只拿它去重新枚举的清单里比对。 */
        token: string;
        atMs: number;
        bytes: number;
      }
    | null
  >(null);
  let backupClearing = $state(false);
  /** 正在切换钉住的那一份（token）；`null` = 空闲。防连点，也让那一行的图钉转灰。 */
  let pinBusy = $state<string | null>(null);
  /** 自动清理开关正在写盘。 */
  let retentionBusy = $state(false);

  /**
   * 钉住 / 取消钉住一份。
   *
   * 钉住**只挡自动清理**，不挡手动删除——这个区别必须让用户看得见，所以：
   * 钉住的那一行图钉变实心、并给 toast 说清"自动清理不会动它，你可以随时删"。
   *
   * 用「上一份状态」推新状态（`!point.pinned`），不读 DOM：读 DOM 会在失败重试时
   * 把状态翻错一次，而这里翻错的方向恰好是"用户以为钉住了、其实没钉"。
   */
  async function togglePin(point: BackupPointDto): Promise<void> {
    if (pinBusy !== null) {
      return;
    }
    pinBusy = point.token;
    const next = !point.pinned;
    try {
      await api.backups.setPinned(point.token, next);
      ui.toast(
        'success',
        next ? 'settings.restore_point_pinned' : 'settings.restore_point_unpinned',
      );
    } catch (err) {
      // 「那一份已经不在盘上了」不该报红：目标已经达成，用户要的是"别再自动删它"，
      // 而它已经没了。其余失败照实说。
      const appErr = toAppError(err);
      ui.toast(
        'error',
        appErr.code === ErrorCode.ERR_BACKUP_BUSY
          ? appErr.messageKey
          : 'settings.restore_point_pin_failed',
      );
    } finally {
      pinBusy = null;
      // 成功要重画（图钉状态来自后端），失败也要重画（可能已经被自动清理删掉了）。
      await refreshBackupStatus();
    }
  }

  /**
   * 开关「自动清理」。
   *
   * 关掉只是"以后不再自动删"，**不会撤销已经删掉的**；打开也只是"下次保存时清一次"，
   * 不是立刻动手。文案必须说清这一点，否则用户会以为点一下就会腾出空间。
   */
  async function toggleRetention(e: Event): Promise<void> {
    const on = (e.currentTarget as HTMLInputElement).checked;
    retentionBusy = true;
    try {
      await api.backups.setRetention(on);
      ui.toast(
        'success',
        on ? 'settings.restore_point_retention_turned_on' : 'settings.restore_point_retention_turned_off',
      );
    } catch {
      ui.toast('error', 'settings.restore_point_retention_failed');
    } finally {
      retentionBusy = false;
      await refreshBackupStatus();
    }
  }
  // 「用这份退回去」的确认弹窗。密码**不是**走过场：后端先用它把那一份验一遍，
  // 验不过一个字节都不动、也不锁屏。所以这个框既是一次确认，也是一次校验。
  //
  // `restorePointTarget` 记的是用户**点的那一行**，不是回到 `backupStatus` 现取：
  // 列表在弹窗开着的时候可能被下一次刷新改掉，而确认框里说的必须是用户看清了的
  // 那一份（时间、大小、要不要旧密码）。取现成的会在他读文案的几秒里换成另一份。
  let restorePointTarget = $state<BackupPointDto | null>(null);
  let restorePointOpen = $state(false);
  let restorePointPwd = $state('');
  let restorePointBusy = $state(false);
  /** 换密回执里关于回滚点的部分（`null` = 这次没有可说的）。 */
  let pwdBackupDetail = $state<{
    count: number;
    failed: number;
    saved: boolean;
    /**
     * `isolate` **整体**没做成（连批次目录都没建起来）。
     *
     * 这一格必须单独存在：那种情况下 `count`（moved）与 `failed` 双双为 0，
     * 而备份其实**一份都没归档、全留在当前代**并贴着「当前主密码可开」的错标签。
     * 只看前两个数的分支一个都不会进，回执就只剩「主密码已修改」——最坏的情况
     * 恰好是唯一不报警的情况。
     */
    isolateError: boolean;
  } | null>(null);

  /** 只刷回滚点状态。换密成功后调用——**不要**用 `reloadSideData()`：
   *  它会把 `draft` 从 store 重置一遍，冲掉用户在别的卡片上还没保存的编辑。 */
  async function refreshBackupStatus(): Promise<void> {
    try {
      backupStatus = await api.backups.status();
      backupLoadFailed = false;
    } catch {
      backupStatus = null;
      backupLoadFailed = true;
    }
  }

  /**
   * 删除回滚点（一份 / 一组）。
   *
   * 三种目标都走这一个函数，因为它们的**回执语言**必须一致：删除是逐份尝试的，
   * 任何一份删不掉都会留下，而界面已经按返回值划掉了。`{deleted, remaining}`
   * 四格分开说，是唯一能表达"没删干净"的通道。
   *
   * 目标不同、**确认文案不同**：
   * - 删一份 → 说清"删掉之后不能再退回到那一刻"；
   * - 删当前这一组 → 说清"改密码之前那些不会被删"；
   * - 删改密码之前那一组 → 说清"那些旧密码在这台电脑上就没副本了"。
   * 不互指的话，用户点完一个就以为清干净了，而两个方向各自只清一半。
   */
  async function runBackupClear(): Promise<void> {
    const target = backupClearTarget;
    if (target === null || backupClearing) {
      return;
    }
    backupClearing = true;
    try {
      const out = await (async () => {
        if (target.kind === 'point') {
          return api.backups.removePoint(target.token);
        }
        if (target.group === 'current') {
          return api.backups.clearCurrent();
        }
        // 旧代按批存在（一次换密一批）。界面不再显示"批次"，但删除仍要一批一批走：
        // 后端的删除单位就是批次目录，一次一条，回执才能逐批核对。
        let deleted = 0;
        let remaining = 0;
        for (const batch of backupStatus?.old_batches ?? []) {
          const one = await api.backups.clearOldBatch(batch.dir_name);
          deleted += one.deleted;
          remaining += one.remaining;
        }
        return { deleted, remaining };
      })();
      backupClearTarget = null;
      // 「删了几份」不足以说出真相：删除是逐份尝试的，任何一份删不掉都会留下。
      //
      // `{0, 0}` 只剩**一个来源**：目标已经不在了（别人删掉了 / 界面没刷）
      // ——也就是"你这次是重试"。所以这一格是**目标已达成**，不是失败：
      // 给红条会让用户以为出了问题，去反复点第二次。
      if (out.remaining === 0 && out.deleted > 0) {
        ui.toast('success', 'settings.restore_point_cleared', { count: out.deleted });
      } else if (out.deleted > 0) {
        ui.toast('error', 'settings.restore_point_cleared_partial', {
          count: out.deleted,
          remaining: out.remaining,
        });
      } else if (out.remaining > 0) {
        ui.toast('error', 'settings.restore_point_cleared_none', { remaining: out.remaining });
      } else {
        // 文案本身是陈述句（"已经不在磁盘上了"），语气交给我们这里定。
        ui.toast('success', 'settings.restore_point_cleared_gone');
      }
    } catch (err) {
      // 失败**不改**列表：回滚点还在盘上，界面不能先把它划掉。
      //
      // 「占用中」要说清原因（恢复或换密还在跑，等它结束就行），这是用户唯一
      // 能自己解决的一类失败；其余失败给通用句，不把原始 token 摆出来。
      const appErr = toAppError(err);
      ui.toast(
        'error',
        appErr.code === ErrorCode.ERR_BACKUP_BUSY
          ? appErr.messageKey
          : 'settings.restore_point_failed',
      );
    } finally {
      backupClearing = false;
      // **成败都要刷**：`Err` 也可能已经删掉了一部分（逐份删除+中途失败），
      // 不刷新的话界面留着旧清单，用户照着它再点一次只会拿到 {0,0}。
      await refreshBackupStatus();
    }
  }

  /**
   * 用本机备份退回去（回到用户点的那一份）。
   *
   * 这个按钮存在的理由：设置页写着「改坏了能退回来」，但在此之前**没有任何回退动作**
   * ——备份文件躺在 `backups/` 里、没有 `.vault` 后缀，连「从备份恢复」的对话框都
   * 列不出它。一句承诺配一个不存在的入口，比不承诺更糟。
   *
   * 1.5.30 补了动作，却给了它一个几乎不可能成立的条件（只认 `pre-restore`，
   * 而那个文件只有在用户**已经做过一次恢复**之后才存在）。所以现在是"清单 + 每行一个
   * 按钮"：能退的东西盘上有几份就列几份（D-178）。
   *
   * 成功后**不调 `api.vault.lock()`**：后端在覆盖库文件之前已经锁了会话，这里再锁一次
   * 只是掩盖事实；只把界面状态同步成"已锁"，让用户回到锁屏页重新解锁。
   */
  async function runRestorePoint(): Promise<void> {
    const target = restorePointTarget;
    if (restorePointBusy || target === null) {
      return;
    }
    if (restorePointPwd.length === 0) {
      ui.toast('error', 'settings.restore_point_pwd_required');
      return;
    }
    restorePointBusy = true;
    try {
      const at = await api.backups.restoreFromPoint(target.token, restorePointPwd);
      restorePointOpen = false;
      restorePointTarget = null;
      restorePointPwd = '';
      // 顺序要紧：先把界面送回去，再报结果。反过来会出现"toast 已经弹出、
      // 页面还停在设置页"的一瞬，而那一刻后端已经是锁定的——用户点什么都会失败。
      await session.lock();
      // 退回到的是**改密码之前**那一份时，库的密码也跟着变回那一把。用户下一秒
      // 就会站在锁屏页上，拿现在的密码解不开——这句话必须在这里说，锁屏页说不出来。
      ui.toast(
        'success',
        target.needs_old_password
          ? 'settings.restore_point_restored_old_pwd'
          : 'settings.restore_point_restored',
        { time: formatDateTime(at, ui.locale) },
      );
    } catch (err) {
      const appErr = toAppError(err);
      // 「密码打不开」与「备份坏了」是两件事，后端已经分流好了（`lock.restore_password_cannot_open`
      // vs 损坏类文案），这里照它的 key 说，不折成一句通用的失败。
      ui.toast('error', appErr.messageKey);
      // 只在失败时刷：成功时会话已经锁掉，`backup_status` 必然 `NotUnlocked`——
      // 刷了只会把 `backupLoadFailed` 置成 true，而组件下一秒就随锁屏卸载了。
      await refreshBackupStatus();
    } finally {
      restorePointBusy = false;
    }
  }

  /**
   * 把后端枚举翻成一句话。认不出的按"读写出错"——不显示原始 token。
   *
   * **磁盘满与本机备份的额度满必须分开说**（D-186）：两者用户要做的事完全不同——
   * 前者去清磁盘，后者只要删几份旧备份（磁盘可能还剩几十 GB）。合成一句
   * 「磁盘空间或配额不足」，用户去看磁盘发现空间充足，只会认为应用在胡说，
   * 然后什么也不做，备份就一直停着。
   */
  function backupReason(kind: string | null): string {
    switch (kind) {
      case 'no_space':
        return ui.t('settings.restore_point_reason_no_space');
      case 'quota_exceeded':
        return ui.t('settings.restore_point_reason_quota');
      case 'readonly':
      case 'permission':
        return ui.t('settings.restore_point_reason_permission');
      case 'device_gone':
        return ui.t('settings.restore_point_reason_device');
      default:
        return ui.t('settings.restore_point_reason_other');
    }
  }
  /**
   * 用户点「知道了」：清掉那条失败记录。
   *
   * 清的是**记录**，不是事实——那次没存成的退路不会因为点了按钮回来。
   * 所以文案是「知道了」而不是「忽略」或「已解决」：它只承认"我看过了"。
   */
  async function dismissBackupError(): Promise<void> {
    try {
      await api.backups.clearError();
    } catch {
      // 清不掉也不阻断：界面照旧显示那条记录，用户能看到它没消失。
    }
    await refreshBackupStatus();
  }
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
   * 录制态静置多久就主动说一句话（D-200，1.6.3）。
   *
   * 按了键却什么都没发生，在 Windows 上是**必然结果**、不是程序卡了：这个组合若已经被
   * 注册成全局热键（本程序自己的也算），系统会把按键拦下来交给注册方，录制窗口根本
   * 收不到 `keydown`。真机原话是「改不回去了，什么提示都没有」——沉默是这里最坏的输出。
   */
  const HOTKEY_STALL_HINT_MS = 8000;

  /** 开一个静置计时器；录到东西时调用方负责 `clearTimeout` 撤掉。 */
  function startStallHint(): number {
    return window.setTimeout(() => {
      ui.toast('info', 'settings.hotkey_no_response');
    }, HOTKEY_STALL_HINT_MS);
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
    const stall = startStallHint();
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
      // 已经录到东西：撤掉静置提示，免得过 8 秒又冒出一句"按了没反应"。
      window.clearTimeout(stall);
      void saveHotkey(combo);
    };
    window.addEventListener('keydown', onKeydown, true);
    return (): void => {
      window.clearTimeout(stall);
      window.removeEventListener('keydown', onKeydown, true);
    };
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

  // ===== 1.5.39：自动输入热键（prefs.autotype_hotkey，默认 Ctrl+Alt+Space） =====
  //
  // 录制机制与上面的快速搜索热键完全同一套（capture 监听 + comboFromEvent +
  // 后端 canonical 校验/回滚），只是读写的是 prefs 的另一个字段。

  /** 是否正在录制自动输入热键。 */
  let autotypeHotkeyRecording = $state(false);

  /** 显示文案；未设置时明确写"未设置"而不是留白。 */
  const autotypeHotkeyDisplay = $derived(
    prefsDto && prefsDto.autotype_hotkey.length > 0
      ? formatHotkey(prefsDto.autotype_hotkey)
      : ui.t('settings.hotkey_none'),
  );

  function startAutotypeHotkeyRecord(): void {
    autotypeHotkeyRecording = true;
  }

  /** 与上面 `hotkeyRecording` 的 $effect 同一形状：capture 接管键盘，组件销毁时注销。 */
  $effect((): (() => void) | void => {
    if (!autotypeHotkeyRecording) {
      return;
    }
    const stall = startStallHint();
    const onKeydown = (event: KeyboardEvent): void => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === 'Escape') {
        autotypeHotkeyRecording = false;
        return;
      }
      const combo = comboFromEvent(event);
      if (combo === null) {
        // 还只是按下了修饰键，或者没有主修饰键——继续等，不报错。
        return;
      }
      window.clearTimeout(stall);
      void saveAutotypeHotkey(combo);
    };
    window.addEventListener('keydown', onKeydown, true);
    return (): void => {
      window.clearTimeout(stall);
      window.removeEventListener('keydown', onKeydown, true);
    };
  });

  /** 保存自动输入热键。失败时保持录制态，用户直接再按一个组合即可。 */
  async function saveAutotypeHotkey(spec: string): Promise<void> {
    try {
      prefsDto = await api.prefs.setAutotypeHotkey(spec);
      autotypeHotkeyRecording = false;
      ui.toast('success', 'settings.hotkey_saved');
    } catch (err) {
      // ERR_HOTKEY_UNAVAILABLE（被占用）/ ERR_HOTKEY_*（键位问题）都由后端给出具体文案。
      ui.toast('error', toAppError(err).messageKey);
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
    // B-03：回滚点状态。**失败只影响这一块**——它读不到不该让库文件信息/版本号一起空白。
    // 不并进 `loadSettingsPageSide` 的失败隔离矩阵：那是「四块」的契约，加第五块会改它的
    // 单元测试语义，而这里一行 try/catch 就够。
    await refreshBackupStatus();
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
   * 失效原因对应的文案键（§23 / D-185）。
   *
   * 实现在 `lib/bio-platform.ts`（`bioStaleStateKey`）：那里是**显式映射表**。
   * 原实现是拿原因值去拼键名后缀，而后端 wire 值叫 `master_password_changed`、
   * 字典键叫 `bio_state_stale_password` —— 五个原因里两个永远拼出空键，界面直接
   * 显示裸键名（2026-11-04 主人真机截图）。这里只做转发，判据只留一份。
   */
  function bioStaleKey(reason: string): string {
    return bioStaleStateKey(reason, bioSuffix);
  }

  /**
   * 把当前 `draft` 写回设置（D-169）。
   *
   * 设置页原先有个孤零零的「保存」按钮，只服务三处输入框，而它上面还隔着两张卡、
   * 旁边一堆**改了立刻生效**的控件（主题、语言、热键、自启、自动锁定、快速解锁）。
   * 主人实测时就是这么踩的：「改了剪贴板秒数，以为数字改完就好了，结果没生效」。
   *
   * 现在改成**失焦即保存**（`onchange` 在值变化且失焦/回车时触发，不是每敲一个键都写）：
   * 页面上不再存在「要不要按保存」这类需要用户记住的规则。
   *
   * 走 `saveQuiet` 而不是 `save`：连着改三项会连弹三次「设置已保存」。失败必须可见——
   * 静默失败等于让用户以为改生效了（这正是本轮要消灭的那类误导）。
   */
  async function applySettings(): Promise<void> {
    draft.clipboard_clear_secs = clampClipboardTtl(draft.clipboard_clear_secs);
    const health = clampHealthSettings($state.snapshot(draft.health));
    draft.health.weak_max_len = health.weak_max_len;
    draft.health.stale_days = health.stale_days;
    const ok = await settings.saveQuiet($state.snapshot(draft));
    if (!ok) {
      ui.toast('error', 'settings.save_failed');
      return;
    }
    // 写成功后再以库里的值为准：后端可能对某些字段有额外规整，界面不该停在「我以为的值」上。
    draft = $state.snapshot(settings.value);
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
      // B-03：回执必须读回来。`isolated_backups` / `isolate_failed` / `pre_rekey_saved`
      // 三个数不读，界面就只能说一句"改密成功"——而用户不知道旧钥匙还开着，
      // 也不知道那几份有没有被归档到正确的地方。
      const result = await api.vault.changeMasterPassword(oldPassword, newPassword);
      pwdBackupDetail = {
        count: result.isolated_backups,
        failed: result.isolate_failed,
        saved: result.pre_rekey_saved,
        isolateError: result.isolate_error,
      };
      oldPassword = '';
      newPassword = '';
      newPassword2 = '';
      // 归档之后旧代那一栏才有内容，刷一次状态（不整页重载，免得冲掉其它卡片未保存的编辑）。
      await refreshBackupStatus();
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
      pwdBackupDetail = null;
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
    pwdBackupDetail = null;
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
  <!-- D-169：这一行替换掉原先孤零零的「保存」按钮。
       设置页曾经同时存在三种生效方式（改了就生效 / 要按保存 / 点了直接执行），
       而没有任何视觉区分，主人实测时踩到的正是「改了剪贴板秒数以为已经生效」。 -->
  <p class="pms-hint">{ui.t('settings.instant_hint')}</p>

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
    <!-- 1.5.39：自动输入热键（读写 prefs.autotype_hotkey，默认 Ctrl+Alt+Space）。
         平台门控（D-121/D-122）：判据独立成派生态（autotypeHotkeyEditable /
         autotypeHotkeyTextOnly），Windows 才渲染改键控件；Android 只渲染纯文案、
         不渲染 <button>——移动端没有全局热键，挂一个录不上的按钮等于给
         不存在的功能做广告。录制复用上面那套 capture 键盘监听。 -->
    {#if autotypeHotkeyEditable}
      <div class="set-row">
        <span class="set-label">{ui.t('settings.autotype_hotkey')}</span>
        <button
          class="pms-btn is-sm set-ctl"
          class:is-primary={autotypeHotkeyRecording}
          type="button"
          aria-label={ui.t('settings.autotype_hotkey')}
          onclick={startAutotypeHotkeyRecord}
        >
          {autotypeHotkeyRecording ? ui.t('settings.hotkey_recording') : autotypeHotkeyDisplay}
        </button>
      </div>
      <p class="pms-hint">{ui.t('settings.autotype_hotkey_hint')}</p>
    {:else if autotypeHotkeyTextOnly}
      <p class="pms-hint">{ui.t('settings.autotype_hotkey')}</p>
      <p class="pms-hint">{ui.t('settings.autotype_hotkey_hint')}</p>
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
      <input id="set-ttl" class="pms-input set-ctl" type="number" min={CLIPBOARD_TTL_MIN} max={CLIPBOARD_TTL_MAX} bind:value={draft.clipboard_clear_secs}
        onchange={() => void applySettings()} />
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
      <input id="set-weak" class="pms-input set-ctl" type="number" min={HEALTH_WEAK_MIN} max={HEALTH_WEAK_MAX} bind:value={draft.health.weak_max_len}
        onchange={() => void applySettings()} />
    </div>
    <div class="set-row">
      <label class="set-label" for="set-stale">{ui.t('settings.stale_days')}</label>
      <input id="set-stale" class="pms-input set-ctl" type="number" min={HEALTH_STALE_MIN} max={HEALTH_STALE_MAX} bind:value={draft.health.stale_days}
        onchange={() => void applySettings()} />
    </div>
  </div>

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

  <!-- B-03（1.5.29）：自动回滚点。**与上面「导出保险库」是两件事**：
       导出的是能带走、换机可用的加密副本；回滚点只在本机，卸载 / 换机 / 清空数据目录
       会一起没。所以「只在本机」这句必须与状态**同屏**，不做 tooltip、不做跳转链接——
       用户看到"回滚点正常"就不再导出，而换机照样丢，那是产品主动制造的更糟处境。 -->
  <div class="pms-card set-card">
    <h3>{ui.t('settings.restore_point')}</h3>
    <p class="pms-meta">{ui.t('settings.restore_point_hint')}</p>
    <p class="pms-alert is-warning">
      <Icon name="alert" size={16} />
      <span>{ui.t('settings.restore_point_local_only')}</span>
    </p>

    {#if backupLoadFailed}
      <p class="pms-hint is-danger">{ui.t('settings.restore_point_load_failed')}</p>
    {:else if backupStatus === null}
      <p class="pms-meta">{ui.t('common.loading')}</p>
    {:else}
      {#if backupStatus.last_ms > 0}
        <p class="pms-hint">
          {ui.t('settings.restore_point_last_ok', {
            time: formatDateTime(backupStatus.last_ms, ui.locale),
          })}
        </p>
      {/if}
      <!-- D-186（2026-11-04，主人截图：「最近一次备份没存成…」红字一直挂着）：
           失败记录是**粘性**的——下一次成功不会自动清（刻意的：日备份被配额挡一周、
           `last.bak` 每天成功时，自动清会让"退路在悄悄变薄"这件事彻底消失）。
           但「粘性 + 没有出口」= 永久误报：界面同时显示「上次备份 10:30」和
           「最近一次没存成 10:28」，用户只能得出"备份坏了"。所以两件事：
             ① 如果之后已经成功过，话要改成"曾经有一次没存成"，不再暗示现在坏了；
             ② 给一个「知道了」把记录清掉（清的是记录，不是事实）。 -->
      {#if backupStatus.last_error_ms > 0}
        <p class="pms-hint is-danger set-backup-error">
          {#if backupStatus.last_ms > backupStatus.last_error_ms}
            {ui.t('settings.restore_point_error_recovered', {
              reason: backupReason(backupStatus.last_error_kind),
              time: formatDateTime(backupStatus.last_error_ms, ui.locale),
              ok_time: formatDateTime(backupStatus.last_ms, ui.locale),
            })}
          {:else}
            {ui.t('settings.restore_point_error', {
              reason: backupReason(backupStatus.last_error_kind),
              time: formatDateTime(backupStatus.last_error_ms, ui.locale),
            })}
          {/if}
          <button class="pms-btn is-ghost is-sm" type="button" onclick={dismissBackupError}>
            {ui.t('settings.restore_point_error_dismiss')}
          </button>
        </p>
        <!-- 额度满**不是**磁盘满：用户去看磁盘会发现几十 GB 空着，然后什么也不做。
             所以要跟着一句"该做什么"，而且只在这种原因下出现，别的原因不瞎猜。 -->
        {#if backupStatus.last_error_kind === 'quota_exceeded'}
          <p class="pms-meta">{ui.t('settings.restore_point_quota_hint')}</p>
        {/if}
      {/if}

      <!-- D-178 / D-179：盘上有几份能退的，就列几行；**一行一份、行内两个按钮**。
           上一版是"每份三行文字 + 右侧一个长按钮"，主人的原话是「搞得这么麻烦，
           看上去也累」。累的根源不是字多，而是**同一件事要在两张表里各看一遍**：
           一张候选清单、下面又一张旧代批次表，同一个文件在两处各出现一次。
           现在按"要用哪个密码"只分两张表，表里一行一份，行内只有「退回去」「删除」。

           空清单时一句都不渲染：没有可退的东西，按钮留着就是一句做不到的承诺。 -->
      {#if backupStatus.points.length === 0}
        <p class="pms-meta">{ui.t('settings.restore_point_never')}</p>
      {:else}
        <!-- D-184：收起态一行说清"几份、多大、几份要用保存时的密码"。
             摘要里**必须带"其中几份要用保存时的密码"**——这是用户唯一无法从
             "共 N 份"里推出来的信息，而它决定了退回去时要输哪把密码。
             数量为 0 时不写这一截：给一个永远为 0 的读数只是噪音。 -->
        <div class="set-backup-summary">
          <span class="set-summary">
            {backupSummary.old > 0
              ? ui.t('settings.restore_point_summary_old', backupSummary)
              : ui.t('settings.restore_point_summary', backupSummary)}
          </span>
          <button
            class="pms-btn is-ghost is-sm"
            type="button"
            aria-expanded={backupExpanded}
            onclick={() => (backupExpanded = !backupExpanded)}
          >
            {ui.t(
              backupExpanded
                ? 'settings.restore_point_collapse'
                : 'settings.restore_point_expand',
            )}
          </button>
        </div>

        {#if backupExpanded}
          <p class="pms-hint">{ui.t('settings.restore_point_table_hint')}</p>
          {#each pointTables as table (table.group)}
            {#if table.rows.length > 0}
              <div class="set-group-head">
                <h4>
                  {ui.t(table.titleKey, {
                    count: table.rows.length,
                    size: formatBytes(sumBytes(table.rows)),
                  })}
                </h4>
                <button
                  class="pms-btn is-ghost is-sm set-danger"
                  type="button"
                  onclick={() =>
                    (backupClearTarget = {
                      kind: 'group',
                      group: table.group,
                      count: table.rows.length,
                      bytes: sumBytes(table.rows),
                    })}
                >
                  {ui.t('settings.restore_point_clear_group')}
                </button>
              </div>
              {#if table.noteKey !== ''}
                <p class="pms-hint">{ui.t(table.noteKey)}</p>
              {/if}
              <table class="set-table">
                <thead>
                  <tr>
                    <th scope="col">{ui.t('settings.restore_point_col_time')}</th>
                    <th scope="col">{ui.t('settings.restore_point_col_what')}</th>
                    <th scope="col" class="col-actions">
                      {ui.t('settings.restore_point_col_actions')}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {#each table.rows as point (point.token)}
                    <tr>
                      <td class="col-time">{formatDateTime(point.at_ms, ui.locale)}</td>
                      <td>{ui.t(restorePointKindKey(point.kind))}</td>
                      <td class="col-actions">
                        <button
                          class="pms-btn is-ghost is-sm"
                          type="button"
                          onclick={() => {
                            restorePointTarget = point;
                            restorePointPwd = '';
                            restorePointOpen = true;
                          }}
                        >
                          {ui.t('settings.restore_point_retreat')}
                        </button>
                        <!-- 钉住做成图标按钮：它是"别自动删"的一个附加开关，不是主操作。
                             给文字按钮会和「退回去 / 删除」抢注意力，而主人要的是
                             「一行一条，有删除按钮/恢复按钮就好了」。 -->
                        <button
                          class="pms-icon-btn set-pin"
                          class:is-pinned={point.pinned}
                          type="button"
                          aria-pressed={point.pinned}
                          aria-label={ui.t(
                            point.pinned
                              ? 'settings.restore_point_unpin'
                              : 'settings.restore_point_pin',
                          )}
                          title={ui.t(
                            point.pinned
                              ? 'settings.restore_point_unpin'
                              : 'settings.restore_point_pin',
                          )}
                          disabled={pinBusy === point.token}
                          onclick={() => void togglePin(point)}
                        >
                          <Icon name="pin" size={15} />
                        </button>
                        <button
                          class="pms-btn is-ghost is-sm set-danger"
                          type="button"
                          onclick={() =>
                            (backupClearTarget = {
                              kind: 'point',
                              token: point.token,
                              atMs: point.at_ms,
                              bytes: point.bytes,
                            })}
                        >
                          {ui.t('settings.restore_point_remove')}
                        </button>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          {/each}

          <!-- 自动清理：一个开关 + 一句人话。
               不做档位下拉（主人：「所有的功能越简单越好」），天数与份数由后端给。
               关掉之后一份都不删——这是"我自己管"的唯一表达方式。 -->
          <div class="set-retention">
            <div class="set-row is-check">
              <span class="set-label">{ui.t('settings.restore_point_retention_on')}</span>
              <label class="pms-check">
                <input
                  type="checkbox"
                  checked={backupStatus.retention_on}
                  disabled={retentionBusy}
                  onchange={(e) => void toggleRetention(e)}
                />
              </label>
            </div>
            <p class="pms-hint">
              {ui.t('settings.restore_point_retention_note', {
                days: backupStatus.retention_days,
              })}
            </p>
          </div>
        {/if}
      {/if}

      <!-- 盘上有、界面删不了（名字认不出，或目录读不出）。不说出来就是"假装没有"；
           也不能给它的名字编一个 `to_string_lossy` 的版本——那个假名字永远匹配不上，
           用户删的时候会被告知"不存在"，而它明明在。
           两类原因**故意合成一句**：用户要做的完全一样（去文件夹里手动处理）。
           `null` = 后端数不清：那一格**不许出现数字**，所以说的是"个数无法统计"。

           D-175：该显示哪几条由 `backupHints()` 决定，不在这里写 `{#if}`。
           原来两格靠**兄弟关系**保证"最坏情形也说出口"，而兄弟关系是布局不是逻辑——
           被顺手并进上面 `old_files > 0` 那一块，改完测试全绿、这一格静默消失，
           而全部批次读不出时 `old_files` 恰好是 0，正好把该开口的一刻吞掉。

           D-184：这一段**刻意留在折叠之外**——它是异常态（有文件删不掉），
           收起来就等于"假装没有"，而收起的动机只是省地方。 -->
      {#each backupHints(backupStatus) as hint (hint.id)}
        <p class="pms-hint is-danger">
          {#if hint.count === null}
            {ui.t('settings.restore_point_old_unreadable_uncounted')}
          {:else}
            {ui.t('settings.restore_point_old_unreadable', { count: hint.count })}
          {/if}
        </p>
      {/each}
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
      {#if pwdBackupDetail !== null}
        {#if pwdBackupDetail.isolateError}
          <!-- 整体没归档：份数未知（不知道有多少），所以这条不能带数字。
               它必须排在 `failed` 之前——那一格此时恒为 0，永远轮不到它。 -->
          <p class="pms-alert is-danger pwd-result" role="alert">
            <Icon name="alert" size={16} />
            <span>{ui.t('settings.password_changed_isolate_error')}</span>
          </p>
        {:else if pwdBackupDetail.failed > 0}
          <!-- 没能归档的那几份会留在当前代、被标成"当前主密码可开"，实际要用旧密码。
               用户忘记新密码时按这个标签去试，那是他最后一次尝试——必须说出来。 -->
          <p class="pms-alert is-danger pwd-result" role="alert">
            <Icon name="alert" size={16} />
            <span>{ui.t('settings.password_changed_isolate_failed', { count: pwdBackupDetail.failed })}</span>
          </p>
        {:else if pwdBackupDetail.count > 0}
          <p class="pms-meta">{ui.t('settings.password_changed_detail', { count: pwdBackupDetail.count })}</p>
        {:else if !pwdBackupDetail.saved}
          <p class="pms-hint is-danger">{ui.t('settings.password_changed_no_backup')}</p>
        {/if}
      {/if}
      <!-- B-03 两层，缺一不可：
           ① 这一步只换「锁」不换「钥匙」——它在承认能力边界；
           ② 旧代只防顺手牵羊，不防懂行的人——它说清"删旧代"这个动作的效力范围。
           **没有第三层**：原来这里还有一句「新建库 + 全量导入 = 真正的换钥匙」，
           那是假的——导出只有加密 `.vault`（整容器字节复制），而导入只认
           Bitwarden JSON/CSV（`import/mod.rs`），且导入会静默丢附件。
           路不存在就不能写成出口：用户照着做会发现根本做不成，而"做不到"被
           包装成"你可以这样做"比直接说"做不到"更伤人。
           真作废能力要等「更换钥匙」这个功能本身落地，那时再加第三层。 -->
      <p class="pms-hint">{ui.t('settings.change_password_note_lock_only')}</p>
      <p class="pms-hint">{ui.t('settings.change_password_note_old')}</p>
      <p class="pms-hint">{ui.t('settings.change_password_note_no_rekey')}</p>
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

<!-- 删除确认。三种目标、三句不同的话，缺一句就会让人点错：
     删一份要说"删掉之后不能再退回到那一刻"；删当前这一组要说"改密码之前那些不会被删"；
     删改密码之前那一组要说"那些旧密码在这台电脑上就没有副本了"。
     不互指的话，用户点完一个就以为清干净了，而每个方向各自只清一半。 -->
<Modal
  open={backupClearTarget !== null}
  titleKey="settings.restore_point_clear_title"
  onclose={() => (backupClearTarget = null)}
>
  <p class="pms-alert is-danger">
    <Icon name="alert" size={16} />
    <span>
      {#if backupClearTarget?.kind === 'point'}
        {ui.t('settings.restore_point_remove_confirm', {
          time: formatDateTime(backupClearTarget.atMs, ui.locale),
          size: formatBytes(backupClearTarget.bytes),
        })}
      {:else if backupClearTarget?.kind === 'group' && backupClearTarget.group === 'older'}
        {ui.t('settings.restore_point_clear_group_confirm_older', {
          count: backupClearTarget.count,
          size: formatBytes(backupClearTarget.bytes),
        })}
      {:else}
        {ui.t('settings.restore_point_clear_group_confirm', {
          count: backupClearTarget?.count ?? 0,
          size: formatBytes(backupClearTarget?.bytes ?? 0),
        })}
      {/if}
    </span>
  </p>
  {#snippet footer()}
    <button class="pms-btn is-ghost" type="button" onclick={() => (backupClearTarget = null)}>
      {ui.t('common.cancel')}
    </button>
    <button
      class="pms-btn is-primary"
      type="button"
      disabled={backupClearing}
      onclick={() => void runBackupClear()}
    >
      {ui.t('common.delete')}
    </button>
  {/snippet}
</Modal>

<!-- 「用这份退回去」确认框。三件事必须同时说清，缺一句都会让人点错：
     ① 回到哪一刻（具体时间 + 那是哪一步的备份，不是"N 份"）
     ② 那次之后的改动会没有
     ③ 要输哪个密码。
     `needs_old_password` 为真时第 ③ 条是**另一套说法**：那不是"确认是你本人"，
     而是另一把钥匙——而退回去之后主密码也会跟着变回那一把。用错句子等于在
     最需要准确的一刻给了句安慰话。
     不写「回滚点」三个字：这个词在界面上已经换掉了，改用"备份"。 -->
<Modal
  open={restorePointOpen}
  titleKey="settings.restore_point_restore_title"
  onclose={() => {
    if (!restorePointBusy) {
      restorePointOpen = false;
      restorePointTarget = null;
      restorePointPwd = '';
    }
  }}
>
  <p class="pms-alert is-danger">
    <Icon name="alert" size={16} />
    <span>
      {ui.t(
        restorePointTarget?.needs_old_password
          ? 'settings.restore_point_restore_body_old_pwd'
          : 'settings.restore_point_restore_body',
        {
          time: formatDateTime(restorePointTarget?.at_ms ?? 0, ui.locale),
          size: formatBytes(restorePointTarget?.bytes ?? 0),
        },
      )}
    </span>
  </p>
  {#if restorePointTarget !== null}
    <p class="pms-hint">
      {ui.t(restorePointKindKey(restorePointTarget.kind))}
    </p>
  {/if}
  <label class="pms-label" for="rp-pwd">
    {ui.t(
      restorePointTarget?.needs_old_password
        ? 'settings.restore_point_restore_pwd_old'
        : 'settings.restore_point_restore_pwd',
    )}
  </label>
  <input
    id="rp-pwd"
    class="pms-input"
    type="password"
    autocomplete="current-password"
    bind:value={restorePointPwd}
    disabled={restorePointBusy}
  />
  {#snippet footer()}
    <button
      class="pms-btn is-ghost"
      type="button"
      disabled={restorePointBusy}
      onclick={() => {
        restorePointOpen = false;
        restorePointTarget = null;
        restorePointPwd = '';
      }}
    >
      {ui.t('common.cancel')}
    </button>
    <button
      class="pms-btn is-primary"
      type="button"
      disabled={restorePointBusy}
      onclick={() => void runRestorePoint()}
    >
      {ui.t('settings.restore_point_restore_confirm')}
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
  /* D-184：收起态的摘要行（「共 N 份（X MB）· 其中 M 份要用保存时的密码」+ 展开按钮）。
     单独一个类而不是复用 `.set-row`：`.set-row` 的三个变体（is-info / is-actions /
     is-check）是窄屏守门测试在数的结构事实，这里多插一行会把那份读数搅乱，而这一行
     本来也不需要 `.set-row` 的 160px 控件宽。`flex-wrap` 让窄屏自然换行，不必再加媒体查询。 */
  .set-backup-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    flex-wrap: wrap;
    margin-top: 0.5rem;
  }
  /* D-186：失败那行后面要挂一个「知道了」。用 flex + wrap 让窄屏上按钮掉到第二行，
     不去挤文字；`:has()` 在这里用不上（父元素没有可判的结构特征）。 */
  .set-backup-error {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .set-summary {
    font-size: 0.88rem;
    color: var(--pms-muted);
    font-variant-numeric: tabular-nums;
  }
  /* 「能退回去的备份」两张表（D-179：主人要「一行一条，有删除按钮/恢复按钮就好了」）。
     用表格而不是卡片列表：三列天然上下对齐，扫一眼就能比"哪份更近、哪份是哪一步"。
     只做排版，字号与配色一律继承 `.pms-meta` / `.pms-hint` 与 CSS 变量。 */
  .set-group-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin: 0.9rem 0 0.2rem;
  }
  .set-group-head h4 {
    margin: 0;
  }
  .set-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.85rem;
    margin: 0.3rem 0 0.6rem;
  }
  .set-table th {
    text-align: left;
    font-weight: 500;
    font-size: 0.78rem;
    color: var(--pms-muted);
    padding: 0.25rem 0.4rem;
    border-bottom: 1px solid var(--pms-border);
  }
  .set-table td {
    padding: 0.3rem 0.4rem;
    border-bottom: 1px solid var(--pms-border);
    vertical-align: middle;
  }
  .set-table tbody tr:last-child td {
    border-bottom: none;
  }
  /* 等宽数字：一列时间上下对齐。 */
  .set-table .col-time {
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .set-table .col-actions {
    text-align: right;
    white-space: nowrap;
  }
  .set-table .col-actions > * + * {
    margin-left: 0.35rem;
  }
  /* 钉住的图钉：实心变强调色。颜色是**唯一**能说明"这份不会被自动删"的行内标记，
     所以不能只靠 `aria-pressed`——屏幕上看不见的东西等于没说。 */
  .set-pin.is-pinned {
    color: var(--pms-accent);
  }
  .set-pin.is-pinned:hover {
    color: var(--pms-accent);
  }
  .set-pin:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .set-retention {
    margin-top: 1rem;
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
    /* 备份记录表：三列在手机上挤不下。时间列与操作列都带 `nowrap`（合计约 115px
       + 150px），中间「做了什么」只剩几十像素 —— 中文被压成一字一行，整张表被撑到
       半屏高（2026-11-04 主人手机实测「表格被上下撑得很高」）。
       窄屏改成每条两行：上排「时间 ……… 三个操作」，下排整行宽放描述。
       **只改 display，不动 DOM**：桌面仍是真表格，读屏的单元格顺序也不变。
       注意 table / tbody 必须一起 block —— 否则表格布局会把 tr 的 grid 修回
       表格行（CSS 表格匿名框规则），改了这一堆等于没改。 */
    .set-table,
    .set-table tbody {
      display: block;
    }
    .set-table thead {
      /* 列名在堆叠布局里对不上任何一行，留着只会重复。 */
      display: none;
    }
    .set-table tr {
      display: grid;
      grid-template-columns: 1fr auto;
      grid-template-areas:
        'time actions'
        'what what';
      align-items: center;
      gap: 0.15rem 0.5rem;
      padding: 0.45rem 0;
      border-bottom: 1px solid var(--pms-border);
    }
    .set-table td {
      /* 边框挪到 tr 上，否则一格一条横线。 */
      display: block;
      padding: 0;
      border-bottom: none;
    }
    .set-table td.col-time {
      grid-area: time;
    }
    /* 第二列是「发生了什么」：窄屏独占一行、用弱化色，与上一行的时间分开。 */
    .set-table td:nth-child(2) {
      grid-area: what;
      color: var(--pms-muted);
      font-size: 0.8rem;
    }
    .set-table td.col-actions {
      grid-area: actions;
    }
    .set-table tbody tr:last-child {
      border-bottom: none;
    }
  }
</style>
