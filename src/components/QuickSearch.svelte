<script lang="ts">
  // 快速搜索小窗（v1.4.11）。
  //
  // 与主窗口是**两个独立的 WebView**：这里没有 vault / session store 可用，
  // 数据全部现取（`get_status` → `list_items` → `get_item` → `copy_secret`）。
  // 反过来说，`localStorage` 同源共享，所以语言与主题直接继承，不需要宿主传参。
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import { api } from '../lib/ipc';
  import { LOCALES, mapSystemLanguage, translate, type Locale } from '../lib/i18n';
  import { copiedLabel, moveIndex, rankItems } from '../lib/quick-search';
  import { AUTOLOCK_LOCKED_EVENT } from '../lib/auto-lock-event';
  import { QUICK_SEARCH_EVENT } from '../lib/tray-events';
  // 主题 store 在模块加载时就会把 `data-theme` 应用到 <html>：引用即继承主窗口主题。
  // 它内部同步标题栏的那步在小窗上会失败（无边框 + 无 set-theme 权限），那边已 catch 吞掉。
  import '../lib/stores/theme.svelte';
  import type { ItemDto } from '../types';

  const LOCALE_KEY = 'passwordms.locale';
  /** 复制成功后的驻留时间：足够看清"复制了什么"，又不至于挡着粘贴。 */
  const HIDE_DELAY_MS = 450;

  let locale: Locale = 'zh-CN';
  let query = $state('');
  let items = $state<ItemDto[]>([]);
  let selected = $state(0);
  let loading = $state(true);
  let locked = $state(false);
  let failed = $state(false);
  let busy = $state(false);
  /** 底部状态行：复制回执、失效提示都走这里（小窗没有 toast 层）。 */
  let note = $state<{ text: string; tone: 'ok' | 'warn' } | null>(null);
  let input = $state<HTMLInputElement>();

  let clipboardSecs = 30;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  const hits = $derived(rankItems(items, query));

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

  /** 每次唤出都重新拉一遍：用户可能刚在主窗口改过条目或改了剪贴板 TTL。 */
  async function load(): Promise<void> {
    loading = true;
    failed = false;
    note = null;
    try {
      const status = await api.system.getStatus();
      // 没有库 / 已锁定：小窗没有任何东西可复制，如实说明并让用户回主窗口。
      // 正常情况下根本走不到这里——热键在锁屏态直接唤主窗口（见 `hotkey.rs`）。
      locked = status.locked || !status.has_vault;
      if (locked) {
        items = [];
        return;
      }
      // 全量条目：小窗不做分组/收藏过滤，空查询时按频度给"最常用的那几条"。
      items = await api.items.list(undefined, undefined, undefined);
      const settings = await api.settings.get();
      clipboardSecs = settings.clipboard_clear_secs ?? 30;
    } catch {
      failed = true;
    } finally {
      loading = false;
    }
  }

  async function copy(kind: 'password' | 'username'): Promise<void> {
    const hit = hits[selected];
    if (!hit || busy) {
      return;
    }
    busy = true;
    try {
      // 列表接口刻意不带明文密码（`wipe_plaintext`），取明文必须走详情。
      const item = await api.items.get(hit.itemId);
      if (item.kind.type !== 'login') {
        return;
      }
      const value = kind === 'password' ? item.kind.password : item.kind.username;
      if (!value) {
        note = { text: t('quick.no_username'), tone: 'warn' };
        return;
      }
      await api.system.copySecret(value, clipboardSecs);
      // 频度埋点：小窗取过的条目也该在主列表里往前排（后端 60s 节流，失败无所谓）。
      void api.items.recordUsage(hit.itemId).catch((): void => {});
      note = { text: t(copiedLabel(kind, !!hit.username)), tone: 'ok' };
      // 先把回执显示出来再收起：复制了却没有任何反馈，用户会怀疑是否成功而重按。
      hideTimer = setTimeout(hideWindow, HIDE_DELAY_MS);
    } catch {
      note = { text: t('quick.copy_failed'), tone: 'warn' };
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
      void copy(event.shiftKey ? 'username' : 'password');
    }
  }

  onMount((): (() => void) | void => {
    locale = readLocale();
    void load();
    // 每次唤出：清空上次的关键词（留着旧词会让"按热键"看起来没反应）、重拉数据、聚焦。
    const offFocus = listen(QUICK_SEARCH_EVENT, () => {
      query = '';
      selected = 0;
      void load();
      // 等一帧：窗口刚从隐藏转可见时 input 还没法拿焦点。
      requestAnimationFrame((): void => input?.focus());
    });
    // 显示期间被自动锁定：小窗立刻收起。留着一个能读明文的窗口在锁屏后还开着，
    // 等于把"锁屏"这件事做成了装饰。
    const offLock = listen(AUTOLOCK_LOCKED_EVENT, () => hideWindow());

    requestAnimationFrame((): void => input?.focus());

    return (): void => {
      void offFocus.then((off) => off());
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
      placeholder={t('quick.placeholder')}
      aria-label={t('quick.placeholder')}
    />
    {#if query}
      <button class="qs-clear" onclick={() => { query = ''; input?.focus(); }} aria-label={t('common.clear_search')}>×</button>
    {/if}
  </div>

  <div class="qs-body">
    {#if loading}
      <p class="qs-hint">{t('common.loading')}</p>
    {:else if failed}
      <p class="qs-hint is-bad">{t('quick.load_failed')}</p>
    {:else if locked}
      <p class="qs-hint is-bad">{t('quick.locked_hint')}</p>
    {:else if hits.length === 0}
      <!-- 说清"没有匹配的登录条目"而不是笼统的"没找到"：小窗刻意不搜笔记与附件，
           笼统文案会让用户以为自己的条目丢了。 -->
      <p class="qs-hint">{query ? t('quick.empty') : t('quick.empty_all')}</p>
    {:else}
      <ul class="qs-list">
        {#each hits as hit, i (hit.itemId)}
          <li>
            <button
              class="qs-row"
              class:is-on={i === selected}
              onclick={() => { selected = i; void copy('password'); }}
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
      <span class="qs-keys">
        <kbd>Enter</kbd>{t('quick.key_password')}
        <kbd>Shift</kbd>+<kbd>Enter</kbd>{t('quick.key_username')}
        <kbd>Esc</kbd>{t('quick.key_close')}
      </span>
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
    /* 长标题必须截断：小窗宽度固定，撑破会让用户名换行、行高跳动。 */
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

  .qs-keys kbd {
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r-sm);
    padding: 0 0.25rem;
    font-family: var(--pms-mono);
    font-size: 0.72rem;
    background: var(--pms-surface);
  }

  .qs-note.is-warn {
    color: var(--pms-warning);
  }
</style>
