<script lang="ts">
  // 自动输入浮层（1.5.39）。
  //
  // 与快速搜索小窗同族（骨架照抄 QuickSearch.svelte）：与主窗口是两个独立的
  // WebView，这里没有 vault / session store 可用，数据全部现取。`localStorage`
  // 同源共享，所以语言与主题直接继承，不需要宿主传参。
  //
  // 与 QuickSearch 的**关键差别**在数据与选定行为：列表来自 `autotype_candidates`，
  // 每项只有标题、用户名、网址与使用次数（没有密码 / 备注 / TOTP 密钥）；回车不复制、
  // 不取详情——只把 `item_id` 与动作名交给 `auto_type_fill`，用户名与密码由 Rust 侧
  // 自取并注入目标窗口。`auto_type_fill` 的入参与返回不含明文；浮层列表由
  // `autotype_candidates` 提供，只含标题、用户名、网址与使用次数。
  // 守门见 autotype-guard.test.ts。
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import { api } from '../lib/ipc';
  import { LOCALES, mapSystemLanguage, translate, type Locale } from '../lib/i18n';
  import { moveIndex, rankItems } from '../lib/quick-search';
  import {
    autotypeCodeOf,
    autotypeFailureKey,
    type AutotypeAction,
    type AutotypeCandidate,
  } from '../lib/autotype';
  import { AUTOLOCK_LOCKED_EVENT } from '../lib/auto-lock-event';
  import { AUTOTYPE_PICK_EVENT } from '../lib/tray-events';
  // 主题 store 在模块加载时就会把 `data-theme` 应用到 <html>：引用即继承主窗口主题。
  // 它内部同步标题栏的那步在小窗上会失败（无边框 + 无 set-theme 权限），那边已 catch 吞掉。
  import '../lib/stores/theme.svelte';

  const LOCALE_KEY = 'passwordms.locale';
  /** 注入成功后的驻留时间：足够看清回执，又不会让用户以为没生效而重按。 */
  const HIDE_DELAY_MS = 450;

  let locale: Locale = 'zh-CN';
  let query = $state('');
  /** 候选行只有 5 个字段（`autotype_candidates`），浮层的 JS 堆里没有任何明文。 */
  let candidates = $state<AutotypeCandidate[]>([]);
  let selected = $state(0);
  let loading = $state(true);
  let locked = $state(false);
  let failed = $state(false);
  let busy = $state(false);
  /** 底部状态行：注入回执、失败原因都走这里（浮层没有 toast 层）。 */
  let note = $state<{ text: string; tone: 'ok' | 'warn' } | null>(null);
  let input = $state<HTMLInputElement>();

  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  // 排序复用快速搜索的同一个 rankItems（结构型入参，两边位置口径一致）。
  const hits = $derived(rankItems(candidates, query));

  function t(key: string): string {
    return translate(locale, key);
  }

  /** 读界面语言：与主窗口同一个 localStorage 键，读不到就按系统语言兜底（D-102 口径）。 */
  function readLocale(): Locale {
    try {
      const raw = localStorage.getItem(LOCALE_KEY);
      if (raw && (LOCALES as readonly string[]).includes(raw)) {
        return raw as Locale;
      }
    } catch {
      // 隐私模式读不到：走系统语言。
    }
    return mapSystemLanguage(typeof navigator === 'undefined' ? '' : navigator.language);
  }

  function hideWindow(): void {
    if (hideTimer !== undefined) {
      clearTimeout(hideTimer);
      hideTimer = undefined;
    }
    // 缺权限 / 窗口已销毁都只是"没藏起来"，不值得弹错——用户按 Esc 时窗口消失
    // 这件事本身没有替代路径，报错也修不了。
    void getCurrentWindow().hide().catch((): void => {});
  }

  /** 每次唤出都重新拉一遍：用户可能刚在主窗口改过条目或锁定过库。 */
  async function load(): Promise<void> {
    loading = true;
    failed = false;
    note = null;
    try {
      const status = await api.system.getStatus();
      // 没有库 / 已锁定：浮层没有任何东西可注入，如实说明并让用户回主窗口。
      // 正常情况下根本走不到这里——热键在锁屏态不会唤出本窗口（Rust 侧把关）。
      locked = status.locked || !status.has_vault;
      if (locked) {
        candidates = [];
        return;
      }
      // 候选列表来自专用接口：只有标题、用户名、网址与使用次数，没有密码 / 备注 /
      // TOTP 密钥。不要改回全量条目列表——那个接口带 password，明文会躺在本窗口的
      // JS 堆里（M3）。
      candidates = await api.autotype.candidates();
    } catch {
      failed = true;
    } finally {
      loading = false;
    }
  }

  /** 选定条目 → 按动作注入。前端只交 `item_id` 与动作名，明文一律不经过本窗口。 */
  async function fill(action: AutotypeAction): Promise<void> {
    const hit = hits[selected];
    if (!hit || busy) {
      return;
    }
    busy = true;
    try {
      await api.autotype.fill(hit.itemId, action);
      // 先把回执显示出来再收起：注入了却没有任何反馈，用户会怀疑是否成功而重按。
      note = { text: t('autotype.done'), tone: 'ok' };
      hideTimer = setTimeout(hideWindow, HIDE_DELAY_MS);
    } catch (err) {
      // 失败**不关窗口**：让人看清原因（映射表在 autotype.ts，映射不到落人话兜底）。
      note = { text: t(autotypeFailureKey(autotypeCodeOf(err))), tone: 'warn' };
    } finally {
      busy = false;
    }
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      hideWindow();
      return;
    }
    if (locked || failed) {
      return;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      selected = moveIndex(selected, 1, hits.length);
      return;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      selected = moveIndex(selected, -1, hits.length);
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      // 五种动作的修饰键直选。**Ctrl+Shift 必须先判**：它是 Ctrl 与 Shift 两个
      // 单键分支的叠加，顺序反了 Ctrl+Shift+Enter 会落进「仅密码」。
      // Alt+Enter 的 preventDefault 由上面统一做了（否则 Windows 会去敲窗口菜单/响铃）。
      const action: AutotypeAction =
        event.ctrlKey && event.shiftKey
          ? 'totp'
          : event.ctrlKey
            ? 'password'
            : event.shiftKey
              ? 'username'
              : event.altKey
                ? 'url'
                : 'default';
      void fill(action);
    }
  }

  onMount((): (() => void) | void => {
    locale = readLocale();
    void load();
    // 每次唤出：清空上次的关键词（留着旧词会让"按热键"看起来没反应）、重拉数据、聚焦。
    const offPick = listen(AUTOTYPE_PICK_EVENT, () => {
      query = '';
      selected = 0;
      void load();
      // 等一帧：窗口刚从隐藏转可见时 input 还没法拿焦点。
      requestAnimationFrame((): void => input?.focus());
    });
    // 显示期间被自动锁定：浮层立刻收起。留着一个能触发注入的窗口在锁屏后还开着，
    // 等于把"锁屏"这件事做成了装饰。
    const offLock = listen(AUTOLOCK_LOCKED_EVENT, () => hideWindow());

    requestAnimationFrame((): void => input?.focus());

    return (): void => {
      void offPick.then((off) => off());
      void offLock.then((off) => off());
    };
  });

  // 选中项跟随键盘滚动。`nearest` 而不是 `center`：少量结果时不希望列表自己跳来跳去。
  $effect((): void => {
    void selected;
    const el = document.querySelector('.qs-row.is-on');
    el?.scrollIntoView({ block: 'nearest' });
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div class="qs">
  <div class="qs-head">
    <span class="qs-ico" aria-hidden="true">⌕</span>
    <input
      bind:this={input}
      bind:value={query}
      oninput={() => (selected = 0)}
      class="qs-input"
      type="text"
      spellcheck="false"
      autocomplete="off"
      placeholder={t('autotype.search_placeholder')}
      aria-label={t('autotype.search_placeholder')}
    />
    {#if query}
      <button class="qs-clear" onclick={() => { query = ''; input?.focus(); }} aria-label={t('common.clear_search')}>×</button>
    {/if}
  </div>

  <div class="qs-body">
    {#if loading}
      <p class="qs-hint">{t('common.loading')}</p>
    {:else if failed}
      <p class="qs-hint is-bad">{t('autotype.failed')}</p>
    {:else if locked}
      <p class="qs-hint is-bad">{t('autotype.failed_not_unlocked')}</p>
    {:else if hits.length === 0}
      <!-- 浮层只搜登录条目（排序纯函数的既有边界）：库为空与无匹配对用户的下一步
           是同一个——换词再搜或回主窗口建条目，共用一句文案就够。 -->
      <p class="qs-hint">{t('autotype.empty')}</p>
    {:else}
      <ul class="qs-list">
        {#each hits as hit, i (hit.itemId)}
          <li>
            <button
              class="qs-row"
              class:is-on={i === selected}
              onclick={() => { selected = i; void fill('default'); }}
              onmouseenter={() => (selected = i)}
            >
              <span class="qs-title">{hit.title}</span>
              <span class="qs-user">{hit.username ?? t('quick.no_username')}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="qs-foot">
    {#if note}
      <span class="qs-note" class:is-warn={note.tone === 'warn'} role="status">{note.text}</span>
    {:else if !locked && !failed}
      <span class="qs-keys">{t('autotype.foot_hint')}</span>
    {/if}
  </div>
</div>

<style>
  /* 无边框窗口：这层 1px 边框是它与桌面之间的唯一边界线索。 */
  .qs {
    display: flex;
    flex-direction: column;
    height: 100vh;
    box-sizing: border-box;
    background: var(--pms-surface);
    color: var(--pms-fg);
    border: 1px solid var(--pms-border-strong);
    border-radius: var(--pms-r);
    overflow: hidden;
    font-family: var(--pms-font);
  }

  .qs-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.7rem 0.8rem;
    border-bottom: 1px solid var(--pms-border);
    background: var(--pms-bg-2);
  }

  .qs-ico {
    color: var(--pms-muted);
    font-size: 1.1rem;
    line-height: 1;
  }

  .qs-input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--pms-fg);
    font: inherit;
    font-size: 1rem;
  }

  .qs-input::placeholder {
    color: var(--pms-faint);
  }

  .qs-clear {
    border: 0;
    background: transparent;
    color: var(--pms-muted);
    font-size: 1.1rem;
    line-height: 1;
    cursor: pointer;
    padding: 0 0.2rem;
  }

  .qs-clear:hover {
    color: var(--pms-danger);
  }

  .qs-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .qs-hint {
    margin: 0;
    padding: 1.1rem 0.9rem;
    color: var(--pms-muted);
    font-size: 0.86rem;
    text-align: center;
  }

  .qs-hint.is-bad {
    color: var(--pms-danger);
  }

  .qs-list {
    list-style: none;
    margin: 0;
    padding: 0.25rem;
  }

  .qs-row {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.15rem;
    width: 100%;
    box-sizing: border-box;
    padding: 0.5rem 0.6rem;
    border: 0;
    border-radius: var(--pms-r-sm);
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .qs-row.is-on {
    background: var(--pms-accent-soft);
  }

  .qs-title {
    font-size: 0.92rem;
    /* 长标题必须截断：浮窗宽度固定，撑破会让用户名换行、行高跳动。 */
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .qs-user {
    font-size: 0.78rem;
    color: var(--pms-muted);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .qs-foot {
    border-top: 1px solid var(--pms-border);
    padding: 0.45rem 0.8rem;
    background: var(--pms-bg-2);
    font-size: 0.75rem;
    color: var(--pms-muted);
    min-height: 1.1rem;
  }

  .qs-keys {
    display: flex;
    gap: 0.3rem;
    align-items: center;
    flex-wrap: wrap;
  }

  .qs-note.is-warn {
    color: var(--pms-warning);
  }
</style>
