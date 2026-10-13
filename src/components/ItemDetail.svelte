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
  let totpCode = $state('');
  let totpRemaining = $state(0);

  // 切换条目时复位秘密展示状态。
  $effect((): void => {
    const id = item?.item_id ?? null;
    revealed = false;
    totpCode = '';
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
      if (cancelled) {
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
          }
        } catch (err) {
          if (!cancelled) {
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

  async function copyText(text: string, withCountdown: boolean): Promise<void> {
    if (text.length === 0) {
      return;
    }
    if (withCountdown) {
      const ttl = settings.value.clipboard_clear_secs;
      await copySecretWithAutoClear(text, ttl, (remaining: number): void => {
        copyCountdown = remaining;
      });
      ui.toast('success', 'common.copied_countdown', { n: ttl });
    } else {
      await api.system.copySecret(text, 0);
      ui.toast('success', 'common.copied');
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
</script>

<section class="detail">
  {#if item === null}
    <div class="empty"><Icon name="key" size={28} /><p>{ui.t('common.none')}</p></div>
  {:else}
    <header>
      <div class="titles">
        <h2>{item.kind.title}</h2>
        <span class="badge">
          {item.kind.type === 'login'
            ? ui.t('item.type_login')
            : item.kind.type === 'secure_note'
              ? ui.t('item.type_note')
              : ui.t('item.type_attachment')}
        </span>
      </div>
      <div class="tools">
        <button
          class="icon-btn"
          type="button"
          aria-label={item.favorite ? ui.t('item.unfavorite') : ui.t('item.favorite')}
          onclick={() => void toggleFavorite()}
        >
          <Icon name="star" size={17} />
        </button>
        <button class="icon-btn" type="button" aria-label={ui.t('common.edit')} onclick={onedit}>
          <Icon name="settings" size={17} />
        </button>
        <button class="icon-btn" type="button" aria-label={ui.t('item.delete')} onclick={() => void remove()}>
          <Icon name="trash" size={17} />
        </button>
      </div>
    </header>

    {#if item.kind.type === 'login'}
      {#if item.kind.url}
        <div class="row">
          <span class="label">{ui.t('editor.field_url')}</span>
          <span class="value link">{item.kind.url}</span>
          <button class="icon-btn" type="button" aria-label={ui.t('item.open_url')} onclick={() => void openUrl()}>
            <Icon name="external" size={16} />
          </button>
        </div>
      {/if}

      {#if item.kind.username}
        <div class="row">
          <span class="label">{ui.t('editor.field_username')}</span>
          <span class="value">{item.kind.username}</span>
          <button
            class="icon-btn"
            type="button"
            aria-label={ui.t('item.copy_username')}
            onclick={() => void copyText(item.kind.type === 'login' ? item.kind.username ?? '' : '', false)}
          >
            <Icon name="copy" size={16} />
          </button>
        </div>
      {/if}

      {#if item.kind.password}
        <div class="row">
          <span class="label">{ui.t('editor.field_password')}</span>
          <span class="value mono">{revealed ? item.kind.password : '••••••••'}</span>
          <button
            class="icon-btn"
            type="button"
            aria-label={revealed ? ui.t('editor.hide_password') : ui.t('editor.show_password')}
            onclick={() => void revealPassword()}
          >
            <Icon name={revealed ? 'eye_off' : 'eye'} size={16} />
          </button>
          <button
            class="icon-btn"
            type="button"
            aria-label={ui.t('item.copy_password')}
            onclick={() => void copyText(item.kind.type === 'login' ? item.kind.password ?? '' : '', true)}
          >
            <Icon name="copy" size={16} />
          </button>
        </div>
        {#if copyCountdown > 0}
          <p class="countdown">{ui.t('common.copied_countdown', { n: copyCountdown })}</p>
        {/if}
      {/if}

      {#if totpSecret}
        <div class="totp">
          <span class="label">{ui.t('editor.totp_now')}</span>
          <strong class="mono">{formatCode(totpCode)}</strong>
          <span class="remain">{ui.t('common.seconds', { n: totpRemaining })}</span>
        </div>
      {/if}

      {#if item.kind.notes}
        <div class="block">
          <span class="label">{ui.t('editor.field_notes')}</span>
          <p class="notes">{item.kind.notes}</p>
        </div>
      {/if}

      {#if item.kind.custom_fields.length > 0}
        <div class="block">
          <span class="label">{ui.t('editor.custom_fields')}</span>
          {#each item.kind.custom_fields as field (field.key)}
            <div class="row">
              <span class="label">{field.key}</span>
              <span class="value">{field.hidden ? '••••••••' : field.value}</span>
            </div>
          {/each}
        </div>
      {/if}
    {:else if item.kind.type === 'secure_note'}
      <SecureNoteView {item} />
    {:else}
      <AttachmentView {item} />
    {/if}

    <footer>
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
    padding: 1rem;
    overflow: auto;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    color: var(--pms-muted);
  }
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 1rem;
  }
  .titles {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 1.1rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    font-size: 0.7rem;
    color: var(--pms-accent);
    background: var(--pms-accent-soft);
    border-radius: 6px;
    padding: 0.15rem 0.45rem;
    white-space: nowrap;
  }
  .tools {
    display: flex;
    gap: 0.15rem;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.55rem 0;
    border-bottom: 1px solid var(--pms-border);
  }
  .label {
    flex: 0 0 22%;
    font-size: 0.78rem;
    color: var(--pms-muted);
  }
  .value {
    flex: 1;
    font-size: 0.9rem;
    word-break: break-all;
  }
  .value.mono,
  .mono {
    font-family: var(--pms-mono);
    letter-spacing: 0.02em;
  }
  .link {
    color: var(--pms-accent);
  }
  .countdown {
    margin: 0.35rem 0 0;
    font-size: 0.76rem;
    color: var(--pms-warning);
  }
  .totp {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.6rem 0;
    border-bottom: 1px solid var(--pms-border);
  }
  .totp strong {
    font-size: 1.25rem;
  }
  .remain {
    font-size: 0.76rem;
    color: var(--pms-muted);
  }
  .block {
    margin-top: 0.85rem;
  }
  .block > .label {
    display: block;
    margin-bottom: 0.35rem;
  }
  .notes {
    white-space: pre-wrap;
    word-break: break-word;
    margin: 0;
    font-size: 0.88rem;
    line-height: 1.6;
  }
  footer {
    margin-top: auto;
    padding-top: 1rem;
    display: flex;
    gap: 1rem;
    font-size: 0.72rem;
    color: var(--pms-muted);
  }
</style>
