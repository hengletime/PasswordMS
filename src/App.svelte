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
  import { RESUME_EVENTS, createResumeGate, hasTauriHost, shouldRefreshOnResume } from './lib/app-resume';
  import { api } from './lib/ipc';
  import { QUICK_SEARCH_EVENT, TRAY_HINT_EVENT } from './lib/tray-events';
  import { session } from './lib/stores/session.svelte';
  import { settings } from './lib/stores/settings.svelte';
  import { sync } from './lib/stores/sync.svelte';
  import { ui } from './lib/stores/ui.svelte';
  import { vault } from './lib/stores/vault.svelte';

  onMount((): void => {
    void init();
    registerAutoLockListener();
    registerResumeListener();
    registerShellListener();
  });

  /**
   * v1.4.10 系统集成：托盘提示 + 全局热键。
   *
   * 两条都由宿主发起（Rust 已把窗口唤到前台），前端只做"焦点该落在哪"和"提示什么"。
   */
  function registerShellListener(): void {
    if (!hasTauriHost()) {
      return;
    }
    // 首次点 X：窗口还在，来得及把这条话说完。之后再点就直接隐藏，不再打扰。
    void listen(TRAY_HINT_EVENT, () => {
      ui.toast('info', 'tray.hint_minimized');
    });
    // 热键：只在主界面跳转焦点。锁屏态由后端唤窗，用户看到的是主密码输入框——
    // 那里不该被搜索框抢焦点（搜索框此时根本不存在）。
    void listen(QUICK_SEARCH_EVENT, () => {
      if (ui.route === 'main') {
        ui.requestSearchFocus();
      }
    });
  }

  /**
   * 托盘菜单文案跟随界面语言（Rust 侧读不到用户选的 UI 语言）。
   *
   * `ui.t()` 内部读 `locale`，所以这个 effect 会在切语言时重跑；
   * 失败静默——后端有中文默认文案，托盘依然可用，不值得为它弹错误。
   */
  $effect((): void => {
    if (!hasTauriHost()) {
      return;
    }
    void api.prefs
      .setTrayLabels({
        show: ui.t('tray.show_window'),
        lock: ui.t('tray.lock'),
        quit: ui.t('tray.quit'),
      })
      .catch((): void => {});
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
   * D-123 / D-124：宿主回到前台时重查快速解锁状态（移动端唯一可靠的时机）。
   *
   * 安卓从系统设置切回来时 WebView 的 `document` 从未失焦，设置页挂的 `focus` 监听
   * 不会触发，所以这件事只能由宿主通知。
   *
   * 注册在根组件而不是设置页：锁屏按钮的可见性也依赖这份状态，用户在系统设置里
   * 加了指纹再回来，锁屏那个按钮就该立刻能用/立刻消失，跟他停在哪个页面无关。
   *
   * **两条链路都听**（D-124）：Rust 侧转发的 `app://resumed`，加上上游自己转发给 JS 的
   * `tauri://resumed`。两条同源、互为兜底——D-123 只挂了一条，偏偏挂错在永不触发的
   * 那一层（`RunEvent::Resumed` 在安卓上从不发出），整整一版功能是死的。
   * 两条同时到达时由 `createResumeGate` 合并成一次查询。
   */
  function registerResumeListener(): void {
    if (!hasTauriHost()) {
      return;
    }
    const gate = createResumeGate(async (): Promise<void> => {
      if (!shouldRefreshOnResume({ busy: session.busy, route: ui.route })) {
        return;
      }
      await session.refreshBiometric();
    });
    for (const name of RESUME_EVENTS) {
      void listen(name, () => {
        gate.trigger();
      });
    }
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
