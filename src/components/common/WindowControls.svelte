<script lang="ts">
  // 自绘窗口控制钮（最小化 / 最大化·还原 / 关闭）。
  // 为什么要自己画：主人裁定去掉原生标题栏 —— 原生标题与页面内品牌重复、图标观感差。
  // decorations:false 之后，右上角三钮与「窗口拖动」都得自己来：
  //   拖动 = 各屏头部容器挂 data-tauri-drag-region（Tauri 运行时内置处理，含双击最大化）
  //   三钮 = @tauri-apps/api/window 的 Window 实例
  //
  // ⚠️ 安卓没有"窗口控制"这回事，UA 带 Android 就整个不渲染。
  //    不能只靠 <768px 媒体查询：桌面窗口缩到 360px 宽时按钮若也消失，窗口就关不掉了。
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import type { Window as TauriWindow } from '@tauri-apps/api/window';

  // 拿不到 Window 实例（比如纯浏览器/测试环境）就整个不渲染，绝不抛错拖垮页面。
  let appWindow: TauriWindow | null = null;
  try {
    appWindow = getCurrentWindow();
  } catch {
    appWindow = null;
  }
  const isDesktop = appWindow !== null && !/Android/i.test(navigator.userAgent);

  async function minimize(): Promise<void> {
    await appWindow?.minimize();
  }
  async function toggleMaximize(): Promise<void> {
    await appWindow?.toggleMaximize();
  }
  async function closeWindow(): Promise<void> {
    await appWindow?.close();
  }
</script>

{#if isDesktop}
  <div class="win-controls">
    <button class="wc-btn" type="button" aria-label="最小化" title="最小化" onclick={minimize}>
      <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
        <path d="M1 6h10" stroke="currentColor" stroke-width="1.2" />
      </svg>
    </button>
    <button class="wc-btn" type="button" aria-label="最大化 / 还原" title="最大化 / 还原" onclick={toggleMaximize}>
      <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
        <rect x="1.6" y="1.6" width="8.8" height="8.8" rx="1" fill="none" stroke="currentColor" stroke-width="1.2" />
      </svg>
    </button>
    <button class="wc-btn is-close" type="button" aria-label="关闭" title="关闭" onclick={closeWindow}>
      <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
        <path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
      </svg>
    </button>
  </div>
{/if}

<style>
  .win-controls {
    display: flex;
    align-items: stretch;
  }
  .wc-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 46px;
    height: 40px;
    border: none;
    background: transparent;
    color: var(--pms-muted);
    cursor: pointer;
    -webkit-app-region: no-drag;
    transition: background var(--pms-dur) var(--pms-ease), color var(--pms-dur) var(--pms-ease);
  }
  .wc-btn:hover {
    background: var(--pms-bg-2);
    color: var(--pms-fg);
  }
  /* 关闭是危险动作：悬停转红（danger-ink 跟随主题，浅色白字 / 暗色深字） */
  .wc-btn.is-close:hover {
    background: var(--pms-danger);
    color: var(--pms-danger-ink);
  }
</style>
