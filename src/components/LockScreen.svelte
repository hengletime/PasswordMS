<script lang="ts">
  // 解锁界面（PRD F-01）。解锁失败统一提示，不区分原因（A-05）。
  // 设计规范 §5.1：品牌区 + .pms-field（纵向）⇒ .pms-input-group（横向，眼睛固定右端）。
  import { open } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';
  import Icon from './common/Icon.svelte';
  import Logo from './common/Logo.svelte';
  import WindowControls from './common/WindowControls.svelte';
  import Spinner from './common/Spinner.svelte';
  import ThemeToggle from './common/ThemeToggle.svelte';
  import { toAppError } from '../lib/errors';
  import { api } from '../lib/ipc';
  import { session } from '../lib/stores/session.svelte';
  import { ui } from '../lib/stores/ui.svelte';

  let password = $state('');
  let showPassword = $state(false);
  // D-55：备份恢复流程进行中（选文件 → 确认 → 替换库文件），期间禁止并发解锁/再次恢复。
  let restoring = $state(false);

  /**
   * 快速解锁按钮的显示条件（`docs/08` §4.5）。
   *
   * **只有 `ready` 才显示**：`stale` / `corrupt` / `absent` 一律不显示——失效的封印点了
   * 也只会弹一句「已失效」，反而让用户以为按钮坏了。重新开启的入口在设置页（那里能看到
   * 具体失效原因）。`availability !== 'available'` 同理（平台没配置 Hello 时按钮没有意义）。
   */
  const canBiometricUnlock = $derived(
    session.biometric?.availability === 'available' && session.biometric?.state === 'ready',
  );

  onMount(() => {
    // 锁定态可查（后端只读头部 + 旁路文件）；查不到时 store 会置 null，按钮自然不出现。
    void session.refreshBiometric();
  });

  /** 用快速解锁进入主界面（失败提示由 store 按错误码分流，这里只管路由）。 */
  async function unlockWithBiometric(): Promise<void> {
    if (session.busy || restoring) {
      return;
    }
    const ok = await session.unlockWithBiometric();
    if (ok) {
      ui.setRoute('main');
    }
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (session.busy || restoring || password.length === 0) {
      return;
    }
    const ok = await session.unlock(password);
    password = '';
    if (ok) {
      ui.setRoute('main');
    }
  }

  /** D-55 从备份恢复：选 .vault 备份 → 警告确认 → 替换本机库 → 用密码框的值走既有解锁流程。 */
  async function restoreFromBackup(): Promise<void> {
    if (session.busy || restoring) {
      return;
    }
    // 1. 选备份文件（只收 PasswordMS 导出的 .vault）。
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'PasswordMS', extensions: ['vault'] }],
    });
    if (selected === null) {
      return;
    }
    const sourcePath = Array.isArray(selected) ? selected[0] : selected;
    if (!sourcePath) {
      return;
    }
    // 2. 主密码为空先提示：恢复后要用密码框的值解锁（unlock_check 校验的是备份
    //    创建时的主密码），不发无谓的 IPC。
    if (password.length === 0) {
      ui.toast('error', 'lock.restore_password_required');
      return;
    }
    // 3. 警告确认：备份将替换本机当前库（原库自动留 .bak 后路）。
    const ok = await ui.confirm('lock.restore_confirm_title', 'lock.restore_confirm_body');
    if (!ok) {
      return;
    }
    restoring = true;
    try {
      await api.vault.restoreBackup(sourcePath);
    } catch (err) {
      restoring = false;
      ui.toast('error', toAppError(err).messageKey);
      return;
    }
    restoring = false;
    // 4. 走既有解锁成功路径：失败时 session.unlock 置 unlockError（统一提示，A-05）。
    const unlocked = await session.unlock(password);
    password = '';
    if (unlocked) {
      ui.setRoute('main');
    }
  }
</script>

<main class="lock-screen" data-tauri-drag-region>
  <!-- 去掉原生标题栏后的自绘顶条：拖动区 + 主题切换 + 窗口三钮 -->
  <div class="win-chrome" data-tauri-drag-region>
    <ThemeToggle />
    <WindowControls />
  </div>
  <div class="pms-card lock-card">
    <div class="lock-brand">
      <span class="lock-logo"><Logo size={44} /></span>
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
            disabled={session.busy || restoring}
          />
          <button
            class="pms-icon-btn"
            type="button"
            aria-label={showPassword ? ui.t('lock.hide_password') : ui.t('lock.show_password')}
            title={showPassword ? ui.t('lock.hide_password') : ui.t('lock.show_password')}
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
        disabled={session.busy || restoring || password.length === 0}
      >
        {#if session.busy}
          <Spinner size={16} label={ui.t('lock.busy')} />
        {:else}
          {ui.t('lock.unlock')}
        {/if}
      </button>
    </form>

    {#if canBiometricUnlock}
      <!-- 快速解锁：只在「本机已开启且平台可用」时出现，位置紧贴主解锁按钮之下
           （视觉上属于同一组入口，而不是和「创建新库」并列的次要动作）。 -->
      <button
        class="pms-btn is-ghost is-block lock-bio"
        type="button"
        disabled={session.busy || restoring}
        onclick={() => void unlockWithBiometric()}
      >
        {#if session.busy}
          <Spinner size={16} label={ui.t('lock.biometric_waiting')} />
          {ui.t('lock.biometric_waiting')}
        {:else}
          <Icon name="unlock" size={15} />{ui.t('lock.biometric_unlock')}
        {/if}
      </button>
    {/if}

    <button class="pms-btn is-ghost is-block lock-create" type="button" disabled={session.busy || restoring} onclick={() => ui.setRoute('create')}>
      {ui.t('lock.create_new')}
    </button>
    <!-- D-55：恢复后用上面密码框里的值解锁（备份创建时的主密码），不新增第二个密码框。 -->
    <button class="pms-btn is-ghost is-block lock-restore" type="button" disabled={session.busy || restoring} onclick={() => void restoreFromBackup()}>
      <Icon name="upload" size={15} />{ui.t('lock.restore_button')}
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
  /* 品牌标本体自带渐变方牌（Logo.svelte），这里只负责占位对齐，不再叠自己的底色 */
  .lock-logo {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
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
  /* 快速解锁：与主解锁按钮同组（0.5rem），与下面的「创建 / 恢复」拉开距离。 */
  .lock-bio {
    margin-top: 0.5rem;
    gap: 0.4rem;
  }
  /* D-55「从备份恢复」：与「创建新保险库」同构的 ghost 块按钮，紧随其后。 */
  .lock-restore {
    margin-top: 0.25rem;
  }
  /* 右上角常驻主题切换（未进主界面也能切）。 */
  /* 去掉原生标题栏后的自绘顶条：兼作窗口拖动区（含双击最大化）。
     背景透明 —— 底下就是锁屏的渐变背景，不需要再铺一层。 */
  .win-chrome {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    height: 40px;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 0.35rem;
    padding-right: 2px;
  }
</style>
