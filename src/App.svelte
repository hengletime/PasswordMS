<script lang="ts">
  // 根组件：路由（锁屏 / 创建向导 / 主界面）+ 全局 Toast / 确认框。
  import { onMount } from 'svelte';
  import CreateVaultWizard from './components/CreateVaultWizard.svelte';
  import LockScreen from './components/LockScreen.svelte';
  import MainLayout from './components/MainLayout.svelte';
  import ConfirmDialog from './components/common/ConfirmDialog.svelte';
  import Toast from './components/common/Toast.svelte';
  import { session } from './lib/stores/session.svelte';
  import { ui } from './lib/stores/ui.svelte';

  onMount((): void => {
    void session.refresh().then((): void => {
      ui.setRoute(session.status.has_vault ? 'lock' : 'create');
    });
  });

  $effect((): void => {
    // 解锁完成即进入主界面（幂等）。
    if (!session.status.locked) {
      ui.setRoute('main');
    }
  });
</script>

{#if ui.route === 'main'}
  <MainLayout />
{:else if ui.route === 'create'}
  <CreateVaultWizard />
{:else}
  <LockScreen />
{/if}

<Toast />
<ConfirmDialog />
