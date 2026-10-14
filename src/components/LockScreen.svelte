<script lang="ts">
  // 解锁界面（PRD F-01）。解锁失败统一提示，不区分原因（A-05）。
  // 设计规范 §5.1：品牌区 + .pms-field（纵向）⇒ .pms-input-group（横向，眼睛固定右端）。
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
  <div class="pms-card lock-card">
    <div class="lock-brand">
      <span class="lock-logo"><Icon name="shield" size={24} /></span>
      <h1>{ui.t('app.title')}</h1>
      <p class="pms-meta">{ui.t('app.tagline')}</p>
    </div>

    <form class="lock-form" onsubmit={submit}>
      <div class="pms-field">
        <label class="pms-label" for="master-password">{ui.t('lock.password_label')}</label>
        <div class="pms-input-group">
          <input
            id="master-password"
            class="pms-input"
            type={showPassword ? 'text' : 'password'}
            autocomplete="current-password"
            placeholder={ui.t('lock.password_placeholder')}
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
        {#if session.unlockError}
          <p class="pms-hint is-danger" role="alert">{ui.t('lock.error')}</p>
        {/if}
      </div>

      <button
        class="pms-btn is-primary is-block is-lg"
        type="submit"
        disabled={session.busy || password.length === 0}
      >
        {#if session.busy}
          <Spinner size={16} label={ui.t('lock.busy')} />
        {:else}
          {ui.t('lock.unlock')}
        {/if}
      </button>
    </form>

    <button class="pms-btn is-ghost is-block lock-create" type="button" onclick={() => ui.setRoute('create')}>
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
    /* 极淡的青绿径向光晕，呼应暗色科技感。 */
    background:
      radial-gradient(52rem 34rem at 50% -12%, var(--pms-accent-soft), transparent 62%),
      var(--pms-bg);
  }
  .lock-card {
    width: min(400px, 100%);
    padding: 2rem;
  }
  .lock-brand {
    text-align: center;
    margin-bottom: 1.4rem;
  }
  .lock-logo {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    border-radius: var(--pms-r);
    background: var(--pms-accent-soft);
    color: var(--pms-accent);
  }
  .lock-brand h1 {
    font-size: 1.35rem;
    font-weight: 600;
    margin: 0.6rem 0 0.2rem;
  }
  .lock-brand p {
    margin: 0;
  }
  .lock-form {
    margin-top: 1.2rem;
  }
  .lock-create {
    margin-top: 0.5rem;
  }
</style>
