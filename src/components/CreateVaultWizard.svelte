<script lang="ts">
  // 创建保险库向导（PRD F-01 / U6）：< 8 位硬拦截；≥ 8 位仅强度提示。
  // 设计规范 §5.2：品牌区 + 警告条(.pms-alert.is-warning) + 两个**完全同构**的密码框（都带眼睛）。
  // 注意：MIN_MASTER_PASSWORD_LEN 硬拦截、acknowledged 门禁、canSubmit/tooShort/mismatch 逻辑一律未改。
  import Icon from './common/Icon.svelte';
  import Logo from './common/Logo.svelte';
  import WindowControls from './common/WindowControls.svelte';
  import StrengthMeter from './common/StrengthMeter.svelte';
  import Spinner from './common/Spinner.svelte';
  import ThemeToggle from './common/ThemeToggle.svelte';
  import LegalDialog from './LegalDialog.svelte';
  import { MIN_MASTER_PASSWORD_LEN } from '../lib/constants';
  import { session } from '../lib/stores/session.svelte';
  import { ui } from '../lib/stores/ui.svelte';

  let password = $state('');
  let confirm = $state('');
  let acknowledged = $state(false);
  let showPassword = $state(false);
  // D-63：「免责声明 / 用户协议」可点击查看（弹窗打开不改变勾选状态）。
  let legalOpen = $state(false);
  let legalKind = $state<'disclaimer' | 'terms'>('disclaimer');

  /** 打开指定法律文档弹窗（只改弹窗状态，绝不触碰 acknowledged）。 */
  function openLegal(docKind: 'disclaimer' | 'terms'): void {
    legalKind = docKind;
    legalOpen = true;
  }

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

<main class="wz-screen" data-tauri-drag-region>
  <!-- 去掉原生标题栏后的自绘顶条：拖动区 + 主题切换 + 窗口三钮 -->
  <div class="win-chrome" data-tauri-drag-region>
    <ThemeToggle />
    <WindowControls />
  </div>
  <div class="pms-card wz-card">
    <header class="wz-head">
      <span class="wz-logo"><Logo size={44} /></span>
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
        <!-- D-63：勾选文字拆成「label（仅 checkbox + 前缀）+ 链接按钮」——
             被点区域严格分离：点 label 内文字切换勾选，点链接只开弹窗、勾选状态不变
             （链接是 label 的兄弟节点，不触发 label 的隐式 toggle，无需 preventDefault）。 -->
        <label class="pms-check">
          <input type="checkbox" bind:checked={acknowledged} disabled={session.busy} />
          <span>{ui.t('create.acknowledge_prefix')}</span>
        </label>
        <button class="pms-link" type="button" onclick={() => openLegal('disclaimer')}>
          {ui.t('create.disclaimer_link')}
        </button>
        <span class="wz-ack-mid">{ui.t('create.acknowledge_middle')}</span>
        <button class="pms-link" type="button" onclick={() => openLegal('terms')}>
          {ui.t('create.terms_link')}
        </button>
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

<!-- D-63：法律条款弹窗（点击「免责声明 / 用户协议」链接打开）。 -->
<LegalDialog bind:open={legalOpen} kind={legalKind} />

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
  /* 品牌标本体自带渐变方牌（Logo.svelte），这里只负责占位对齐，不再叠自己的底色 */
  .wz-logo {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
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
    /* D-63：label（checkbox + 前缀）+ 两个链接按钮 + 连接词横排，窄屏自动换行。 */
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.25rem;
  }
  .wz-ack-mid {
    font-size: 0.86rem;
    color: var(--pms-fg);
  }
  /* D-64：窄屏（≤767px，项目统一定义）勾选行必须单行放下。
     375px 手机上原排版会折成两行（"与"落行尾、"用户协议"被挤到第二行），很难看。
     做法＝文案精炼（"我已阅读并同意"）+ 整行字号收敛到 0.8rem + 间距收紧，实测单行。 */
  @media (max-width: 767px) {
    .wz-ack {
      font-size: 0.8rem;
      gap: 0.2rem;
      padding: 0.6rem 0.7rem;
    }
    .wz-ack-mid {
      font-size: inherit;
    }
    /* 全局 .pms-check 是 0.86rem、.pms-link 用 font:inherit——统一收进本行字号 */
    .wz-ack :global(.pms-check) {
      font-size: inherit;
      gap: 0.4rem;
    }
    .wz-ack :global(.pms-check input[type='checkbox']) {
      width: 16px;
      height: 16px;
      flex: 0 0 16px;
    }
  }
  .wz-back {
    margin-top: 0.4rem;
  }
  /* 右上角常驻主题切换（未进主界面也能切）。 */
  /* 去掉原生标题栏后的自绘顶条（与 LockScreen 同款）：兼作窗口拖动区 */
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
