<script lang="ts">
  // 根组件：路由（锁屏 / 创建向导 / 主界面）+ 全局 Toast / 确认框。
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import CreateVaultWizard from './components/CreateVaultWizard.svelte';
  import LockScreen from './components/LockScreen.svelte';
  import MainLayout from './components/MainLayout.svelte';
  import ConfirmDialog from './components/common/ConfirmDialog.svelte';
  import Toast from './components/common/Toast.svelte';
  import { AUTOLOCK_LOCKED_EVENT } from './lib/auto-lock-event';
  import { session } from './lib/stores/session.svelte';
  import { settings } from './lib/stores/settings.svelte';
  import { sync } from './lib/stores/sync.svelte';
  import { ui } from './lib/stores/ui.svelte';
  import { vault } from './lib/stores/vault.svelte';

  onMount((): void => {
    void init();
    registerAutoLockListener();
  });

  /**
   * D-108（豆包审查 3.4-6）：后端 auto-lock 兜底触发后的前端收尾。
   * 后端已完成锁定级联（session.lock + 清剪贴板 + 擦导入预览 + 停重同步监听），
   * 这里只做前端侧的 store 复位与路由切换（与 lockNow 成功路径一致，不再发 IPC）。
   * 仅在 Tauri WebView 环境注册（Node 测试环境无 @tauri-apps API）。
   */
  function registerAutoLockListener(): void {
    if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) {
      return;
    }
    void listen(AUTOLOCK_LOCKED_EVENT, () => {
      vault.reset();
      settings.reset();
      sync.reset();
      ui.setRoute('lock');
    });
  }

  /**
   * D-106（豆包审查 3.5-5）：启动状态查询失败时**不得**路由到 create——
   * 那会把瞬态 IPC 故障伪装成「还没建库」，诱导用户走进有写库路径的向导。
   * 改为停在当前路由并显示「启动失败 + 重试」。
   */
  let startupError = $state(false);

  async function init(): Promise<void> {
    startupError = false;
    await session.refresh();
    if (session.statusError) {
      startupError = true;
      return;
    }
    ui.setRoute(session.status.has_vault ? 'lock' : 'create');
  }

  $effect((): void => {
    // 解锁完成即进入主界面（幂等）。
    if (!session.status.locked) {
      ui.setRoute('main');
    }
  });
</script>

{#if startupError}
  <div class="startup-error" role="alert">
    <p>{ui.t('err.startup_failed')}</p>
    <button class="pms-btn is-primary" type="button" onclick={() => void init()}>
      {ui.t('common.retry')}
    </button>
  </div>
{:else if ui.route === 'main'}
  <MainLayout />
{:else if ui.route === 'create'}
  <CreateVaultWizard />
{:else}
  <LockScreen />
{/if}

<Toast />
<ConfirmDialog />

<style>
  /* D-106：启动失败面板（避免误入创建向导的兜底界面）。 */
  .startup-error {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    min-height: 100vh;
    padding: 2rem;
    text-align: center;
  }
  .startup-error p {
    margin: 0;
    color: var(--pms-fg);
  }
</style>
