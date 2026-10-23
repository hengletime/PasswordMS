<script lang="ts">
  // #531 自绘上下文菜单（桌面右键 / 移动端长按）。零依赖：fixed 定位卡片 + 全局点击/按键关闭。
  // 不用原生 <select> / alert —— WebView 上不可控（§5.8 的教训：取消与失败必须自己建模）。
  import { onMount, untrack } from 'svelte';

  export interface MenuEntry {
    key: string;
    label: string;
    danger?: boolean;
  }

  interface Props {
    x: number;
    y: number;
    entries: MenuEntry[];
    onselect: (key: string) => void;
    onclose: () => void;
  }
  const { x, y, entries, onselect, onclose }: Props = $props();

  let menuEl = $state<HTMLElement | null>(null);
  // 组件按「每次开菜单挂载一次」用（调用方 {#if open} 包裹），只取初始坐标是有意为之 —— untrack 表意并消警。
  let adjustedX = $state(untrack(() => x));
  let adjustedY = $state(untrack(() => y));

  onMount(() => {
    const el = menuEl;
    if (el !== null) {
      const rect = el.getBoundingClientRect();
      if (rect.right > window.innerWidth - 8) {
        adjustedX = Math.max(8, window.innerWidth - rect.width - 8);
      }
      if (rect.bottom > window.innerHeight - 8) {
        adjustedY = Math.max(8, window.innerHeight - rect.height - 8);
      }
    }
    const onPointerDown = (event: PointerEvent): void => {
      const target = event.target as HTMLElement | null;
      if (target !== null && !el?.contains(target)) {
        onclose();
      }
    };
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        onclose();
      }
    };
    const onScrollOrResize = (): void => onclose();
    window.addEventListener('pointerdown', onPointerDown, true);
    window.addEventListener('keydown', onKey);
    window.addEventListener('scroll', onScrollOrResize, true);
    window.addEventListener('resize', onScrollOrResize);
    return (): void => {
      window.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('scroll', onScrollOrResize, true);
      window.removeEventListener('resize', onScrollOrResize);
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="ctx-root" role="presentation">
  <ul class="ctx-menu" bind:this={menuEl} style:left="{adjustedX}px" style:top="{adjustedY}px" role="menu">
    {#each entries as entry (entry.key)}
      <li>
        <button
          class="ctx-item"
          class:is-danger={entry.danger === true}
          type="button"
          role="menuitem"
          onclick={() => onselect(entry.key)}
        >
          {entry.label}
        </button>
      </li>
    {/each}
  </ul>
</div>

<style>
  .ctx-root {
    position: fixed;
    inset: 0;
    z-index: 90;
  }
  .ctx-menu {
    position: absolute;
    min-width: 168px;
    margin: 0;
    padding: 0.3rem;
    list-style: none;
    background: var(--pms-surface);
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r);
    box-shadow: var(--pms-shadow);
  }
  .ctx-item {
    display: block;
    width: 100%;
    padding: 0.5rem 0.7rem;
    border: none;
    border-radius: var(--pms-r-sm);
    background: transparent;
    color: var(--pms-fg);
    font: inherit;
    font-size: 0.88rem;
    text-align: left;
    cursor: pointer;
  }
  .ctx-item:hover {
    background: var(--pms-bg-2);
  }
  .ctx-item.is-danger {
    color: var(--pms-danger);
  }
</style>
