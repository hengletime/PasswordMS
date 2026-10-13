<script lang="ts">
  // 创建保险库向导（PRD F-01 / U6）：< 8 位硬拦截；≥ 8 位仅强度提示。
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

<main class="wizard">
  <div class="card">
    <h1>{ui.t('create.title')}</h1>
    <p class="warning">
      <Icon name="alert" size={16} />
      <span>{ui.t('create.warning_body')}</span>
    </p>

    <form onsubmit={submit}>
      <label for="create-password">{ui.t('create.master_password')}</label>
      <div class="field">
        <input
          id="create-password"
          type={showPassword ? 'text' : 'password'}
          autocomplete="new-password"
          placeholder={ui.t('create.password_placeholder', { n: MIN_MASTER_PASSWORD_LEN })}
          bind:value={password}
          disabled={session.busy}
        />
        <button
          class="icon-btn"
          type="button"
          aria-label={showPassword ? ui.t('lock.hide_password') : ui.t('lock.show_password')}
          onclick={() => (showPassword = !showPassword)}
        >
          <Icon name={showPassword ? 'eye_off' : 'eye'} />
        </button>
      </div>
      <StrengthMeter {password} />
      {#if tooShort}
        <p class="error" role="alert">{ui.t('create.too_short', { n: MIN_MASTER_PASSWORD_LEN })}</p>
      {/if}

      <label for="create-confirm">{ui.t('create.confirm_password')}</label>
      <input
        id="create-confirm"
        type={showPassword ? 'text' : 'password'}
        autocomplete="new-password"
        bind:value={confirm}
        disabled={session.busy}
      />
      {#if mismatch}
        <p class="error" role="alert">{ui.t('create.mismatch')}</p>
      {/if}

      <label class="check">
        <input type="checkbox" bind:checked={acknowledged} disabled={session.busy} />
        <span>{ui.t('create.acknowledge')}</span>
      </label>

      <button class="btn primary block" type="submit" disabled={!canSubmit}>
        {#if session.busy}
          <Spinner size={16} />
        {:else}
          {ui.t('create.submit')}
        {/if}
      </button>
      <button class="link" type="button" onclick={() => ui.setRoute('lock')} disabled={session.busy}>
        {ui.t('common.back')}
      </button>
    </form>
  </div>
</main>

<style>
  .wizard {
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1.5rem;
  }
  .card {
    width: min(420px, 100%);
    background: var(--pms-surface);
    border: 1px solid var(--pms-border);
    border-radius: 14px;
    padding: 1.75rem;
  }
  h1 {
    font-size: 1.2rem;
    margin: 0 0 0.75rem;
  }
  .warning {
    display: flex;
    gap: 0.5rem;
    align-items: flex-start;
    color: var(--pms-warning);
    font-size: 0.82rem;
    line-height: 1.45;
    margin: 0 0 1rem;
  }
  label {
    display: block;
    font-size: 0.8rem;
    color: var(--pms-muted);
    margin: 0.75rem 0 0.35rem;
  }
  .field {
    display: flex;
    align-items: center;
    border: 1px solid var(--pms-border);
    border-radius: 8px;
    background: var(--pms-bg);
    padding-right: 0.25rem;
  }
  .field input,
  #create-confirm {
    width: 100%;
    box-sizing: border-box;
    border: 1px solid var(--pms-border);
    border-radius: 8px;
    background: var(--pms-bg);
    color: var(--pms-fg);
    padding: 0.6rem 0.75rem;
    font-size: 0.95rem;
    outline: none;
  }
  .field input {
    border: none;
  }
  .icon-btn {
    background: transparent;
    border: none;
    color: var(--pms-muted);
    cursor: pointer;
    padding: 0.35rem;
    line-height: 0;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0.9rem 0;
    font-size: 0.85rem;
    color: var(--pms-fg);
  }
  .error {
    color: var(--pms-danger);
    font-size: 0.82rem;
    margin: 0.5rem 0 0;
  }
  .link {
    display: block;
    width: 100%;
    margin-top: 0.75rem;
    background: transparent;
    border: none;
    color: var(--pms-accent);
    cursor: pointer;
    font-size: 0.85rem;
  }
</style>
