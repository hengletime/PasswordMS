<script lang="ts">
  // 条目详情：复制（含倒计时清空）、显示密码二次确认、打开网址、TOTP、按 kind 分发。
  import Icon from './common/Icon.svelte';
  import AttachmentView from './AttachmentView.svelte';
  import SecureNoteView from './SecureNoteView.svelte';
  import { copySecretWithAutoClear } from '../lib/clipboard';
  import { formatDateTime } from '../lib/constants';
  import { toAppError } from '../lib/errors';
  import { api } from '../lib/ipc';
  import { currentTotp, formatCode, secondsRemaining } from '../lib/totp';
  import { isSafeUrl } from '../lib/url-safety';
  import { settings } from '../lib/stores/settings.svelte';
  import { ui } from '../lib/stores/ui.svelte';
  import { vault } from '../lib/stores/vault.svelte';

  interface Props {
    onedit: () => void;
  }
  const { onedit }: Props = $props();

  let item = $derived(vault.selectedItem);
  let revealed = $state(false);
  let copyCountdown = $state(0);
  /**
   * 剪贴板里当前是哪个字段（D-146）。
   *
   * 与 `copyCountdown` 配对使用：只有两者同时成立提示行才渲染。它存在的理由是「复制了
   * 什么」和「还剩几秒」是两个事实——之前只存秒数，复制用户名再把密码复制进剪贴板后，
   * 提示行既说不出剪贴板里换成了密码，还会被两路定时器轮流写秒数。
   */
  let copiedField = $state<'username' | 'password' | null>(null);
  let totpCode = $state('');
  let totpRemaining = $state(0);
  // D-97（B）：密钥无效进入错误态——只弹一次 toast，行内给出可见错误行，
  // 不再每个 30s 窗口 / 每次切回条目都骚扰一次（旧版 toast 会堆积）。
  let totpInvalid = $state(false);

  // 切换条目时复位秘密展示状态。
  $effect((): void => {
    const id = item?.item_id ?? null;
    revealed = false;
    totpCode = '';
    totpInvalid = false;
    // 引用 id 以建立依赖（切换条目即复位）。
    void id;
  });

  const totpSecret = $derived(
    item?.kind.type === 'login' ? item.kind.totp_secret ?? '' : '',
  );

  // TOTP 实时刷新（每 秒 更新倒计时，跨周期重新取码）。
  $effect(() => {
    if (totpSecret.length === 0) {
      return;
    }
    let cancelled = false;
    let lastWindow = -1;
    const tick = async (): Promise<void> => {
      if (cancelled || totpInvalid) {
        // D-97（B）：已判定密钥无效即停止取码循环（错误态行内可见，无需反复重试）。
        return;
      }
      const now = Date.now();
      const remaining = secondsRemaining(now);
      totpRemaining = remaining;
      const window = Math.floor(now / 1000 / 30);
      if (window !== lastWindow) {
        lastWindow = window;
        try {
          const result = await currentTotp(totpSecret);
          if (!cancelled) {
            totpCode = result.code;
            totpInvalid = false;
          }
        } catch (err) {
          if (!cancelled) {
            totpInvalid = true;
            ui.toast('error', toAppError(err).messageKey);
          }
        }
      }
    };
    void tick();
    const handle = setInterval((): void => void tick(), 1000);
    return (): void => {
      cancelled = true;
      clearInterval(handle);
    };
  });

  /**
   * 复制：用户名与密码走**同一条**路径——都按用户配置的 TTL 到期清空并显示倒计时
   * （PRD F-06「分别一键复制用户名 / 密码，30 秒后自动清空」）。
   *
   * 【D-135】原实现对用户名传 `copy_secret(text, 0)`：前端把 0 当「不清空」，后端却读成
   * 「0 秒后清空」——写进去的同一瞬间被自己的兜底线程清掉。于是 toast 报「已复制」、
   * 用户粘出来是空的。**两端对同一个参数的理解不一致，比参数本身错更隐蔽**。
   *
   * 【D-146】`field` 是必需参数：剪贴板只有一格，提示必须说清**现在里面是什么**。上一轮
   * 复制的倒计时由 `copySecretWithAutoClear` 在内部作废（旧代的 tick 不再转发），这里只需
   * 把「当前字段」换过来；toast 走 `dedupeKey` 顶掉上一条，不再两条并排堆着。
   */
  async function copyText(text: string, field: 'username' | 'password'): Promise<void> {
    if (text.length === 0) {
      return;
    }
    const ttl = settings.value.clipboard_clear_secs;
    await copySecretWithAutoClear(text, ttl, (remaining: number): void => {
      copyCountdown = remaining;
      if (remaining === 0) {
        // 到点就收起提示行：剪贴板已清空，再挂着「还剩 0 秒」是另一种谎话。
        copiedField = null;
      }
    });
    copiedField = field;
    ui.toast('success', 'common.copied_countdown', { n: ttl }, 'clipboard-copy');
    // #531：使用埋点（密码 / 用户名的复制都走这里）。后端 60s 节流；
    // 复制已成功才埋 —— 失败不算「使用」。
    if (item !== null) {
      void vault.recordItemUsage(item.item_id);
    }
  }

  async function openUrl(): Promise<void> {
    if (item?.kind.type !== 'login') {
      return;
    }
    const raw = item.kind.url ?? '';
    if (!isSafeUrl(raw)) {
      ui.toast('error', 'err.unsafe_url');
      return;
    }
    try {
      await api.system.openUrl(raw);
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  async function revealPassword(): Promise<void> {
    if (revealed) {
      revealed = false;
      return;
    }
    const ok = await ui.confirm('editor.reveal_title', 'editor.reveal_body');
    if (ok) {
      revealed = true;
    }
  }

  async function toggleFavorite(): Promise<void> {
    if (item === null) {
      return;
    }
    await vault.toggleFavorite(item.item_id, !item.favorite);
  }

  async function remove(): Promise<void> {
    if (item === null) {
      return;
    }
    const ok = await ui.confirm('item.delete', 'trash.purge_confirm_body');
    if (ok) {
      await vault.deleteItem(item.item_id, false);
    }
  }

  // D-88：登录项转安全笔记（主人拍板 A1 映射）。
  // 后端 update_item 允许整只替换 kind（repo: item.kind = draft.kind）且 item_id 不变，
  // 同步历史 / 附件 / 收藏 / 分组全部天然延续；一次 IPC，无新建+删除的中间态。
  // 映射：notes → 正文；其余字段全部拼进正文尾部（A1：一样不丢），不留在任何密文外。
  async function convertToNote(): Promise<void> {
    if (item === null || item.kind.type !== 'login') {
      return;
    }
    const ok = await ui.confirm('item.convert_note_title', 'item.convert_note_body');
    if (!ok) {
      return;
    }
    const login = item.kind;
    const extra: string[] = [];
    if (login.url) {
      extra.push(`${ui.t('editor.field_url')}: ${login.url}`);
    }
    if (login.username) {
      extra.push(`${ui.t('editor.field_username')}: ${login.username}`);
    }
    if (login.password) {
      extra.push(`${ui.t('editor.field_password')}: ${login.password}`);
    }
    if (login.totp_secret) {
      extra.push(`${ui.t('editor.field_totp')}: ${login.totp_secret}`);
    }
    for (const field of login.custom_fields) {
      if (field.key.trim().length > 0) {
        extra.push(`${field.key}: ${field.value}`);
      }
    }
    let body = login.notes ?? '';
    if (extra.length > 0) {
      if (body.length > 0) {
        body += '\n\n';
      }
      body += `${ui.t('item.convert_fields_header')}\n${extra.join('\n')}`;
    }
    const result = await vault.updateItem(item.item_id, {
      kind: { type: 'secure_note', title: login.title, body },
      folder_id: item.folder_id,
      favorite: item.favorite,
    });
    if (result !== null) {
      ui.toast('success', 'item.convert_done');
    }
  }
</script>

<section class="detail">
  {#if item === null}
    <div class="detail-empty">
      <Icon name="key" size={30} />
      <p class="pms-meta">{ui.t('common.none')}</p>
    </div>
  {:else}
    <header class="detail-head">
      <div class="detail-titles">
        <h2>{item.kind.title}</h2>
        <span class="pms-badge is-accent">
          {item.kind.type === 'login'
            ? ui.t('item.type_login')
            : item.kind.type === 'secure_note'
              ? ui.t('item.type_note')
              : ui.t('item.type_attachment')}
        </span>
      </div>
      <div class="detail-tools">
        <button
          class="pms-icon-btn fav-on"
          class:fav-on-active={item.favorite}
          type="button"
          aria-label={item.favorite ? ui.t('item.unfavorite') : ui.t('item.favorite')}
          aria-pressed={item.favorite}
          onclick={() => void toggleFavorite()}
        >
          <Icon name="star" size={17} filled={item.favorite} />
        </button>
        <button class="pms-icon-btn" type="button" aria-label={ui.t('common.edit')} onclick={onedit}>
          <Icon name="settings" size={17} />
        </button>
        <!-- D-88：登录 → 安全笔记（一次性转换，A1 映射不丢数据）。仅登录项显示。 -->
        {#if item.kind.type === 'login'}
          <button
            class="pms-icon-btn"
            type="button"
            title={ui.t('item.convert_note')}
            aria-label={ui.t('item.convert_note')}
            onclick={() => void convertToNote()}
          >
            <Icon name="file" size={17} />
          </button>
        {/if}
        <button class="pms-icon-btn" type="button" aria-label={ui.t('item.delete')} onclick={() => void remove()}>
          <Icon name="trash" size={17} />
        </button>
      </div>
    </header>

    {#if item.kind.type === 'login'}
      {#if item.kind.url}
        <div class="detail-row">
          <span class="pms-label detail-key">{ui.t('editor.field_url')}</span>
          <span class="detail-value detail-link">{item.kind.url}</span>
          <button class="pms-icon-btn" type="button" aria-label={ui.t('item.open_url')} onclick={() => void openUrl()}>
            <Icon name="external" size={16} />
          </button>
        </div>
      {/if}

      {#if item.kind.username}
        <div class="detail-row">
          <span class="pms-label detail-key">{ui.t('editor.field_username')}</span>
          <span class="detail-value">{item.kind.username}</span>
          <button
            class="pms-icon-btn"
            type="button"
            aria-label={ui.t('item.copy_username')}
            onclick={() =>
              void copyText(
                item.kind.type === 'login' ? item.kind.username ?? '' : '',
                'username',
              )}
          >
            <Icon name="copy" size={16} />
          </button>
        </div>
      {/if}

      {#if item.kind.password}
        <div class="detail-row">
          <span class="pms-label detail-key">{ui.t('editor.field_password')}</span>
          <span class="detail-value detail-mono">{revealed ? item.kind.password : '••••••••'}</span>
          <button
            class="pms-icon-btn"
            type="button"
            aria-label={revealed ? ui.t('editor.hide_password') : ui.t('editor.show_password')}
            onclick={() => void revealPassword()}
          >
            <Icon name={revealed ? 'eye' : 'eye_off'} size={16} />
          </button>
          <button
            class="pms-icon-btn"
            type="button"
            aria-label={ui.t('item.copy_password')}
            onclick={() =>
              void copyText(
                item.kind.type === 'login' ? item.kind.password ?? '' : '',
                'password',
              )}
          >
            <Icon name="copy" size={16} />
          </button>
        </div>
      {/if}

      {#if copyCountdown > 0 && copiedField !== null}
        <!-- D-146：提示行必须点名「现在剪贴板里是什么」。原先只写「将在 {n} 秒后清空」
             且挂在密码行内，复制用户名时既错位、也不说清复制的是哪个字段。 -->
        <p class="pms-hint is-warning" role="status">
          {ui.t('common.copied_countdown_field', {
            field: ui.t(copiedField === 'username' ? 'editor.field_username' : 'editor.field_password'),
            n: copyCountdown,
          })}
        </p>
      {/if}

      {#if totpSecret}
        <div class="detail-totp">
          <span class="pms-label">{ui.t('editor.totp_now')}</span>
          {#if totpInvalid}
            <!-- D-97（B）：错误必须是可见的一行字，不渲染 = 把失败伪装成「本来就没有」。 -->
            <span class="pms-hint is-danger">{ui.t('err.totp_invalid_detail')}</span>
          {:else}
            <strong class="detail-mono detail-totp-code">{formatCode(totpCode)}</strong>
            <span class="pms-meta">{ui.t('common.seconds', { n: totpRemaining })}</span>
          {/if}
        </div>
      {/if}

      {#if item.kind.notes}
        <div class="detail-block">
          <span class="pms-label">{ui.t('editor.field_notes')}</span>
          <p class="detail-notes">{item.kind.notes}</p>
        </div>
      {/if}

      {#if item.kind.custom_fields.length > 0}
        <div class="detail-block">
          <span class="pms-label">{ui.t('editor.custom_fields')}</span>
          {#each item.kind.custom_fields as field (field.key)}
            <div class="detail-row">
              <span class="pms-label detail-key">{field.key}</span>
              <span class="detail-value">{field.hidden ? '••••••••' : field.value}</span>
            </div>
          {/each}
        </div>
      {/if}
    {:else if item.kind.type === 'secure_note'}
      <SecureNoteView {item} />
    {:else if item.kind.type === 'attachment'}
      <!-- D-60：遗留独立附件条目在解锁迁移后不可达（migrate_legacy_attachments）；
           保留分支仅作类型兜底，不渲染附件视图。 -->
    {/if}

    <!-- D-60：附件区（父条目内嵌 attachments），登录/笔记条目通用。 -->
    {#if item.kind.type === 'login' || item.kind.type === 'secure_note'}
      <AttachmentView {item} />
    {/if}

    <footer class="detail-foot">
      <span>{ui.t('item.updated')}: {formatDateTime(item.updated_at, ui.locale)}</span>
      <span>{ui.t('item.rev')}: {item.rev}</span>
    </footer>
  {/if}
</section>

<style>
  .detail {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    padding: 1.1rem;
    overflow: auto;
    /* D-95 滑动返回成立的前提：默认 touch-action:auto 时，Android WebView 对触摸
       位移的滚动/手势判定会发 pointercancel——向左滑的 pointermove 序列在达到
       72px 阈值之前就被掐断（真机实测「完全没反应」）。pan-y = 垂直滚动照常由
       浏览器处理，水平方向归 JS 手势（swipe-back.ts）；桌面鼠标不受该属性影响。 */
    touch-action: pan-y;
  }
  .detail-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    color: var(--pms-muted);
  }
  .detail-empty p {
    margin: 0;
  }
  .detail-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 1rem;
  }
  .detail-titles {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 1.15rem;
    font-weight: 600;
    /* D-103：flex 子项默认 min-width:auto，长标题会把 h2 最小宽度撑到内容宽，
       ellipsis 永不触发且把右侧类型徽章挤成竖排——必须允许 h2 收缩。 */
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* D-103：类型徽章（登录/笔记）永不被标题挤压换行。 */
  .detail-titles :global(.pms-badge) {
    flex-shrink: 0;
    white-space: nowrap;
  }
  .detail-tools {
    display: flex;
    gap: 0.15rem;
  }
  /* 收藏星选中态：金色实心（与列表行 .list-fav 的 --pms-warning 一致，D-39）。 */
  .fav-on-active {
    color: var(--pms-warning);
  }
  .detail-row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.6rem 0;
    border-bottom: 1px solid var(--pms-border);
  }
  .detail-key {
    flex: 0 0 24%;
  }
  .detail-value {
    flex: 1;
    font-size: 0.9rem;
    word-break: break-all;
  }
  .detail-mono {
    font-family: var(--pms-mono);
    letter-spacing: 0.02em;
  }
  .detail-link {
    color: var(--pms-accent);
  }
  .detail-totp {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.65rem 0;
    border-bottom: 1px solid var(--pms-border);
  }
  .detail-totp-code {
    font-size: 1.3rem;
    font-weight: 700;
  }
  .detail-block {
    margin-top: 0.9rem;
  }
  .detail-block > .pms-label {
    display: block;
    margin-bottom: 0.4rem;
  }
  .detail-notes {
    white-space: pre-wrap;
    word-break: break-word;
    margin: 0;
    font-size: 0.88rem;
    line-height: 1.6;
  }
  .detail-foot {
    margin-top: auto;
    padding-top: 1rem;
    display: flex;
    gap: 1rem;
    font-size: 0.72rem;
    color: var(--pms-muted);
  }
</style>
