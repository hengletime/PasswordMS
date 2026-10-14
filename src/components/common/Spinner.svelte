<script lang="ts">
  // 加载指示器（纯 CSS 动画）。设计规范 §5.4：使用 currentColor，随宿主文字色走。
  interface Props {
    size?: number;
    label?: string;
  }
  const { size = 20, label = '' }: Props = $props();
</script>

<span class="spinner" style={`--size:${size}px`} role="status" aria-live="polite">
  <span class="ring"></span>
  {#if label}<span class="label">{label}</span>{/if}
</span>

<style>
  .spinner {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    color: inherit;
  }
  .ring {
    width: var(--size);
    height: var(--size);
    border: 2px solid currentColor;
    border-top-color: transparent;
    border-radius: 50%;
    opacity: 0.85;
    animation: pms-spin 0.8s linear infinite;
  }
  .label {
    font-size: 0.85rem;
  }
  @keyframes pms-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
