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
  import { ErrorCode, toAppError } from '../lib/errors';
  import { api } from '../lib/ipc';
  import { session } from '../lib/stores/session.svelte';
  import { bioPlatformOf, bioTextSuffix } from '../lib/bio-platform';
  import { ui } from '../lib/stores/ui.svelte';

  let password = $state('');
  let showPassword = $state(false);
  // D-55：备份恢复流程进行中（选文件 → 确认 → 替换库文件），期间禁止并发解锁/再次恢复。
  let restoring = $state(false);

  /**
   * **D-211**：把数据目录压成「…\最后两段」再显示。
   *
   * 为什么不直接铺完整路径：Windows 路径里没有空格，浏览器只能在任意字符处硬断——
   * 实测把 `Local\Temp` 断成了 `Local\Te` + `mp\...`，反而比不显示更难读。
   * 而真正能区分「哪一份库」的恰恰是末尾两段（`Tools\PasswordMS` 对
   * `Roaming\com.passwordms.app`）。完整路径挂在 `title` 上，悬停即见。
   */
  function shortPath(p: string): string {
    const parts = p.split(/[\\/]+/).filter(Boolean);
    if (parts.length <= 2) return p;
    return `…\\${parts.slice(-2).join('\\')}`;
  }

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

  /** §23：按钮文案按平台取词（「Windows Hello」/「指纹 · 人脸」）。 */
  const bioSuffix = $derived(bioTextSuffix(bioPlatformOf(session.biometric)));

  /**
   * 恢复失败的提示**留在页面上**，不用 toast：toast 三秒就消失，而这条要说明
   * "原库没有任何改动"——用户必须读到它才敢重试。空串表示没有错误。
   */
  let restoreErrorKey = $state('');

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
    restoreErrorKey = '';
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
    // 2. 主密码为空先提示：这个密码的**主要用途是后端拿它预检所选的这份文件**
    //    （`verify_backup_password`：先验结构、再验凭据，验不过原库一个字节都不动），
    //    恢复成功后才顺带用它解锁（`unlock_check` 校验的是备份创建时的主密码）。
    //    别把预检删成"只留解锁"——那样密码不对时，用户要等到覆盖之后才知道。
    if (password.length === 0) {
      ui.toast('error', 'lock.restore_password_required');
      return;
    }
    // 3. 警告确认：备份将替换本机当前库。**不承诺有 .bak**——它是 best-effort，
    //    用户可见文案里不再提它（提了就等于给一张可能兑现不了的支票）。
    const ok = await ui.confirm('lock.restore_confirm_title', 'lock.restore_confirm_body');
    if (!ok) {
      return;
    }
    restoring = true;
    restoreErrorKey = '';
    try {
      try {
        // 密码一起送：后端先拿它验这份备份，验不过原库一个字节都不动。
        await api.vault.restoreBackup(sourcePath, password, false);
      } catch (err) {
        const appErr = toAppError(err);
        if (appErr.code !== ErrorCode.ERR_RESTORE_NO_RETREAT) {
          throw appErr;
        }
        // 走到这里说明**原库一个字节都没动**：后端在覆盖之前就停下了。
        // 确认框说的只是"这次没有自动回滚点"，不能说成"数据会丢失"。
        //
        // **别拿 `.bak` 当理由**：它随后**可能**写一份 `passwordms.vault.bak-<ms>`，但那是
        // best-effort，而且界面里没有任何入口指向它——所以正文既不承诺它、也不否认它。
        // 谁按"反正有 .bak"去改文案，就等于把这一轮刚删掉的那张空头支票写回去。
        const ok = await ui.confirm(
          'lock.restore_no_retreat_title',
          'lock.restore_no_retreat_body',
        );
        if (!ok) {
          restoring = false;
          return;
        }
        await api.vault.restoreBackup(sourcePath, password, true);
      }
    } catch (err) {
      restoring = false;
      restoreErrorKey = toAppError(err).messageKey;
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
        {#if restoreErrorKey}
          <!-- 恢复失败留在原地：它说清了"原库没有任何改动"，用户要读到才敢重试。 -->
          <p class="pms-hint is-danger" role="alert">{ui.t(restoreErrorKey)}</p>
        {:else if session.unlockError}
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

    <!-- 另一处还有库：这不是错误，是「你可能开错门了」。用告警色、左对齐、不淡化 ——
         它必须被看见；文案给的是出路，不是责怪。 -->
    {#if session.status.other_vault_dir}
      <p class="lock-data-dir is-warn" role="alert" title={session.status.other_vault_dir}>
        {ui.t('lock.other_vault', { dir: shortPath(session.status.other_vault_dir) })}
      </p>
    {/if}

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
          <Spinner size={16} label={ui.t(`lock.biometric_waiting${bioSuffix}`)} />
          {ui.t(`lock.biometric_waiting${bioSuffix}`)}
        {:else}
          <Icon name="unlock" size={15} />{ui.t(`lock.biometric_unlock${bioSuffix}`)}
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

    <!-- D-211：当前数据目录 —— 卡片页脚，排在所有操作之后。
         便携版的数据在 exe 旁、安装版在 `%APPDATA%` —— 两个目录里可以是两份完全
         不同的库，而界面一模一样；用户换了启动入口（改名后自启失效、只好去点桌面
         图标是最常见的路径）就会对着另一份库反复试主密码，失败提示又被 A-05 统一成
         一句「解锁失败，请重试」，他自己没有任何线索。这一行就是那条线索。
         刻意放在最底部并压淡：它是"需要时才去查"的信息，不是每次解锁都要读的提示。 -->
    {#if session.status.data_dir}
      <p class="lock-data-dir" title={session.status.data_dir}>
        {ui.t('lock.data_dir', { dir: shortPath(session.status.data_dir) })}
      </p>
    {/if}
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

  /* D-211：数据目录 —— 卡片页脚。
     ① 压到最底下并居中，像"注脚"而不是"提示"；
     ② 刻意最淡（opacity 0.62）：它是需要时才翻的信息，抢眼反而让人以为出了问题；
     ③ 只显示末尾两段（见 `shortPath`），完整路径挂在 title 上。 */
  .lock-data-dir {
    margin: 1.2rem 0 0;
    padding: 0 0.15rem;
    font-size: 0.7rem;
    line-height: 1.5;
    letter-spacing: 0.01em;
    color: var(--pms-muted);
    text-align: center;
    opacity: 0.62;
    overflow-wrap: break-word;
  }
  /* 「另一处还有库」是**必须被看见**的一条，不能跟着淡化：
     左对齐、告警色、字号略大、不透明度拉满。 */
  .lock-data-dir.is-warn {
    margin-top: 0.9rem;
    font-size: 0.75rem;
    font-weight: 500;
    color: var(--pms-danger);
    text-align: left;
    opacity: 1;
  }
</style>
