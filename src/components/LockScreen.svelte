<script lang="ts">
  // 解锁界面（PRD F-01）。解锁失败统一提示，不区分原因（A-05）。
  import Icon from './common/Icon.svelte';
  import Spinner from './common/Spinner.svelte';
  import { session } from '../lib/stores/session.svelte';
  import { ui } from '../lib/stores/ui.svelte';

  let password = $state('');
  let showPassword = $state(false);

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (session.busy || password.length === 0) {
      return;
    }
    const ok = await session.unlock(password);
    password = '';
    if (ok) {
      ui.setRoute('main');
    }
  }
</script>

<main class="lock-screen">
  <div class="card">
    <div class="brand">
      <Icon name="shield" size={30} />
      <h1>{ui.t('app.title')}</h1>
      <p>{ui.t('app.tagline')}</p>
    </div>

    <form onsubmit={submit}>
      <label for="master-password">{ui.t('lock.password_placeholder')}</label>
      <div class="field">
        <input
          id="master-password"
          type={showPassword ? 'text' : 'password'}
          autocomplete="current-password"
          placeholder={ui.t('lock.password_placeholder')}
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

      {#if session.unlockError}
        <p class="error" role="alert">{ui.t('lock.error')}</p>
      {/if}

      <button class="btn primary block" type="submit" disabled={session.busy || password.length === 0}>
        {#if session.busy}
          <Spinner size={16} label={ui.t('lock.busy')} />
        {:else}
          {ui.t('lock.unlock')}
        {/if}
      </button>
    </form>

    <button class="link" type="button" onclick={() => ui.setRoute('create')}>
      {ui.t('lock.create_new')}
    </button>
  </div>
</main>

<style>
  .lock-screen {
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1.5rem;
  }
  .card {
    width: min(380px, 100%);
    background: var(--pms-surface);
    border: 1px solid var(--pms-border);
    border-radius: 14px;
    padding: 1.75rem;
  }
  .brand {
    text-align: center;
    margin-bottom: 1.25rem;
  }
  .brand h1 {
    font-size: 1.25rem;
    margin: 0.5rem 0 0.25rem;
  }
  .brand p {
    margin: 0;
    color: var(--pms-muted);
    font-size: 0.85rem;
  }
  label {
    display: block;
    font-size: 0.8rem;
    color: var(--pms-muted);
    margin-bottom: 0.35rem;
  }
  .field {
    display: flex;
    align-items: center;
    border: 1px solid var(--pms-border);
    border-radius: 8px;
    background: var(--pms-bg);
    padding-right: 0.25rem;
  }
  .field input {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--pms-fg);
    padding: 0.6rem 0.75rem;
    font-size: 0.95rem;
    outline: none;
  }
  .icon-btn {
    background: transparent;
    border: none;
    color: var(--pms-muted);
    cursor: pointer;
    padding: 0.35rem;
    line-height: 0;
  }
  .error {
    color: var(--pms-danger);
    font-size: 0.82rem;
    margin: 0.5rem 0 0;
  }
  .link {
    display: block;
    width: 100%;
    margin-top: 1rem;
    background: transparent;
    border: none;
    color: var(--pms-accent);
    cursor: pointer;
    font-size: 0.85rem;
  }
</style>
