<script lang="ts">
  // 创建保险库向导（PRD F-01 / U6）：< 8 位硬拦截；≥ 8 位仅强度提示。
  // 设计规范 §5.2：品牌区 + 警告条(.pms-alert.is-warning) + 两个**完全同构**的密码框（都带眼睛）。
  // 注意：MIN_MASTER_PASSWORD_LEN 硬拦截、acknowledged 门禁、canSubmit/tooShort/mismatch 逻辑一律未改。
  import Icon from './common/Icon.svelte';
  import StrengthMeter from './common/StrengthMeter.svelte';
  import Spinner from './common/Spinner.svelte';
  import { MIN_MASTER_PASSWORD_LEN } from '../lib/constants';
  import { session } from '../lib/stores/session.svelte';
  import { ui } from '../lib/stores/ui.svelte';

  let password = $state('');
  let confirm = $state('');
  let acknowledged = $state(false);
  let showPassword = $state(false);

  let tooShort = $derived(password.length > 0 && password.length < MIN_MASTER_PASSWORD_LEN);
  let mismatch = $derived(confirm.length > 0 && confirm !== password);
  let canSubmit = $derived(
    password.length >= MIN_MASTER_PASSWORD_LEN && confirm === password && acknowledged && !session.busy,
  );

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (!canSubmit) {
      return;
    }
    const ok = await session.create(password, true);
    password = '';
    confirm = '';
    if (ok) {
      ui.setRoute('main');
    }
  }
</script>

<main class="wz-screen">
  <div class="pms-card wz-card">
    <header class="wz-head">
      <span class="wz-logo"><Icon name="shield" size={24} /></span>
      <h1>{ui.t('create.title')}</h1>
      <p class="pms-meta">{ui.t('app.tagline')}</p>
    </header>

    <p class="pms-alert is-warning">
      <Icon name="alert" size={16} />
      <span>{ui.t('create.warning_body')}</span>
    </p>

    <form class="wz-form" onsubmit={submit}>
      <div class="pms-field">
        <label class="pms-label" for="create-password">{ui.t('create.password_label')}</label>
        <div class="pms-input-group">
          <input
            id="create-password"
            class="pms-input"
            type={showPassword ? 'text' : 'password'}
            autocomplete="new-password"
            placeholder={ui.t('create.password_placeholder', { n: MIN_MASTER_PASSWORD_LEN })}
            bind:value={password}
            disabled={session.busy}
          />
          <button
            class="pms-icon-btn"
            type="button"
            aria-label={showPassword ? ui.t('lock.hide_password') : ui.t('lock.show_password')}
            onclick={() => (showPassword = !showPassword)}
          >
            <Icon name={showPassword ? 'eye' : 'eye_off'} />
          </button>
        </div>
        <StrengthMeter {password} />
        <p class="pms-hint">{ui.t('create.hint')}</p>
        {#if tooShort}
          <p class="pms-hint is-danger" role="alert">{ui.t('create.too_short', { n: MIN_MASTER_PASSWORD_LEN })}</p>
        {/if}
      </div>

      <div class="pms-field">
        <label class="pms-label" for="create-confirm">{ui.t('create.confirm_password')}</label>
        <div class="pms-input-group">
          <input
            id="create-confirm"
            class="pms-input"
            type={showPassword ? 'text' : 'password'}
            autocomplete="new-password"
            placeholder={ui.t('create.confirm_placeholder')}
            bind:value={confirm}
            disabled={session.busy}
          />
          <button
            class="pms-icon-btn"
            type="button"
            aria-label={showPassword ? ui.t('lock.hide_password') : ui.t('lock.show_password')}
            onclick={() => (showPassword = !showPassword)}
          >
            <Icon name={showPassword ? 'eye' : 'eye_off'} />
          </button>
        </div>
        {#if mismatch}
          <p class="pms-hint is-danger" role="alert">{ui.t('create.mismatch')}</p>
        {/if}
      </div>

      <div class="wz-ack">
        <label class="pms-check">
          <input type="checkbox" bind:checked={acknowledged} disabled={session.busy} />
          <span>{ui.t('create.acknowledge')}</span>
        </label>
      </div>

      <button class="pms-btn is-primary is-block is-lg" type="submit" disabled={!canSubmit}>
        {#if session.busy}
          <Spinner size={16} />
        {:else}
          {ui.t('create.submit')}
        {/if}
      </button>
      <button
        class="pms-btn is-ghost is-block wz-back"
        type="button"
        onclick={() => ui.setRoute('lock')}
        disabled={session.busy}
      >
        {ui.t('common.back')}
      </button>
    </form>
  </div>
</main>

<style>
  .wz-screen {
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1.5rem;
    background:
      radial-gradient(52rem 34rem at 50% -12%, var(--pms-accent-soft), transparent 62%),
      var(--pms-bg);
  }
  .wz-card {
    width: min(460px, 100%);
    padding: 2rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  .wz-head {
    text-align: center;
  }
  .wz-logo {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    border-radius: var(--pms-r);
    background: var(--pms-accent-soft);
    color: var(--pms-accent);
  }
  .wz-head h1 {
    font-size: 1.35rem;
    font-weight: 600;
    margin: 0.6rem 0 0.2rem;
  }
  .wz-head p {
    margin: 0;
  }
  .wz-form {
    display: flex;
    flex-direction: column;
  }
  .wz-ack {
    background: var(--pms-bg-2);
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r);
    padding: 0.7rem 0.85rem;
    margin-bottom: 0.9rem;
  }
  .wz-back {
    margin-top: 0.4rem;
  }
</style>
