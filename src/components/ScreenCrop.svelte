<script lang="ts">
  // 冻结快照上的框选层（D-165）。
  //
  // 这张图是在**窗口还原之前**抓下来的，所以它是死的：用户框哪儿就是哪儿，
  // 窗口什么时候回来都不会改变画面内容。系统截图工具做不到这一点 ——
  // 它的覆盖层是活画面，窗口一还原就被框进去了（1.5.20 的实际表现）。
  //
  // 交互刻意与系统截图一致：按下拖、松开出结果、Esc 取消。松开后即便识别失败
  // 也**不退出**，用户可以直接再框一次 —— 退出去再点「截图」要重新藏窗口抓屏，
  // 为了一个拖偏的手势付一次整屏抓取的代价，不值。
  import { clampRect, type CropRect, type ScreenShot } from '../lib/screen-capture';
  import { ui } from '../lib/stores/ui.svelte';

  interface Props {
    shot: ScreenShot;
    /** 上一轮识别的结果（失败原因）；`null` 表示还没有结果。 */
    hint: string | null;
    onpick: (rect: CropRect) => void;
    oncancel: () => void;
  }
  const { shot, hint, onpick, oncancel }: Props = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let stage = $state<HTMLDivElement | null>(null);
  /** 当前框选（图像像素）。`null` = 还没开始拖。 */
  let selection = $state<CropRect | null>(null);
  /**
   * 框选块的定位样式（相对舞台的 CSS 像素）。
   *
   * 为什么在 JS 里算而不是写百分比：图片是 `max-width/max-height` 约束下缩放显示的，
   * 它的实际渲染尺寸与舞台尺寸并不相同，百分比会按舞台算 —— 框就会整体偏大偏移。
   * `getBoundingClientRect()` 拿到的才是真的那一块。
   */
  let boxStyle = $state<string | null>(null);
  /** 指针是否按下。**不能**放 `$state`：它只在事件之间用，改了也不需要重渲染。 */
  let dragging = false;
  let anchorX = 0;
  let anchorY = 0;

  // 把快照画进 canvas：canvas 的**固有尺寸**就是图像尺寸，缩放交给 CSS。
  $effect((): void => {
    const el = canvas;
    if (el === null) {
      return;
    }
    el.width = shot.width;
    el.height = shot.height;
    const ctx = el.getContext('2d');
    if (ctx !== null) {
      ctx.drawImage(shot.bitmap, 0, 0);
    }
  });

  // Esc 取消：这是个全屏遮罩，没有别的出口，键盘必须有一条。
  $effect((): (() => void) => {
    const onKey = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape') {
        return;
      }
      event.preventDefault();
      oncancel();
    };
    window.addEventListener('keydown', onKey);
    return (): void => window.removeEventListener('keydown', onKey);
  });

  /** 屏幕坐标 → 图像像素，并夹在图像范围内（拖到窗口外也不能变成负数）。 */
  function pointAt(event: PointerEvent): { x: number; y: number } | null {
    const el = canvas;
    if (el === null) {
      return null;
    }
    const box = el.getBoundingClientRect();
    if (box.width <= 0 || box.height <= 0) {
      return null;
    }
    const x = ((event.clientX - box.left) / box.width) * shot.width;
    const y = ((event.clientY - box.top) / box.height) * shot.height;
    return {
      x: Math.max(0, Math.min(shot.width, x)),
      y: Math.max(0, Math.min(shot.height, y)),
    };
  }

  /** 把图像像素的框换算成屏幕上那一块的位置。 */
  function refreshBox(): void {
    const el = canvas;
    const stageEl = stage;
    const rect = selection;
    if (el === null || stageEl === null || rect === null) {
      boxStyle = null;
      return;
    }
    const shotBox = el.getBoundingClientRect();
    const stageBox = stageEl.getBoundingClientRect();
    const kx = shotBox.width / shot.width;
    const ky = shotBox.height / shot.height;
    const left = shotBox.left - stageBox.left + rect.x * kx;
    const top = shotBox.top - stageBox.top + rect.y * ky;
    boxStyle = `left:${left}px;top:${top}px;width:${rect.width * kx}px;height:${rect.height * ky}px`;
  }

  function onPointerDown(event: PointerEvent): void {
    if (event.button !== 0) {
      return;
    }
    const at = pointAt(event);
    if (at === null) {
      return;
    }
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    dragging = true;
    anchorX = at.x;
    anchorY = at.y;
    selection = { x: at.x, y: at.y, width: 0, height: 0 };
    refreshBox();
  }

  function onPointerMove(event: PointerEvent): void {
    if (!dragging) {
      return;
    }
    const at = pointAt(event);
    if (at === null) {
      return;
    }
    // 反向拖（从右下拖到左上）同样成立：取两点构成的正矩形。
    selection = {
      x: Math.min(anchorX, at.x),
      y: Math.min(anchorY, at.y),
      width: Math.abs(at.x - anchorX),
      height: Math.abs(at.y - anchorY),
    };
    refreshBox();
  }

  function onPointerUp(event: PointerEvent): void {
    if (!dragging) {
      return;
    }
    dragging = false;
    onPointerMove(event);
    const picked = selection === null ? null : clampRect(selection, shot.width, shot.height);
    if (picked === null) {
      // 误触（点了一下、拖了一条线）：清掉框，让用户重来，不送空图去解码。
      selection = null;
      boxStyle = null;
      return;
    }
    onpick(picked);
  }
</script>

<div class="crop-layer" role="dialog" aria-modal="true" aria-label={ui.t('editor.crop_hint')}>
  <div class="crop-bar">
    <span class="crop-hint">{ui.t('editor.crop_hint')}</span>
    <button class="pms-btn is-ghost is-sm" type="button" onclick={oncancel}>
      {ui.t('common.cancel')}
    </button>
  </div>
  {#if hint}
    <p class="pms-hint is-danger crop-warn">{hint}</p>
  {/if}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="crop-stage"
    bind:this={stage}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
  >
    <canvas class="crop-shot" bind:this={canvas}></canvas>
    {#if selection && boxStyle}
      <div class="crop-box" style={boxStyle}></div>
    {/if}
  </div>
</div>

<style>
  /* 层级高于通用模态（100）：这是模态里的模态，必须在最上面。 */
  .crop-layer {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.75rem;
    background: var(--pms-bg);
  }
  .crop-bar {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }
  .crop-hint {
    color: var(--pms-ink-2);
    font-size: 0.85rem;
  }
  .crop-warn {
    flex-shrink: 0;
    margin: 0;
  }
  .crop-stage {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    overflow: hidden;
    touch-action: none;
    cursor: crosshair;
    user-select: none;
  }
  /* 固有尺寸由 canvas.width/height 决定，这里只约束显示上限 —— 不拉伸、不裁切。 */
  .crop-shot {
    max-width: 100%;
    max-height: 100%;
    display: block;
    border-radius: var(--pms-r);
    box-shadow: var(--pms-shadow);
  }
  /* 框选外的区域压暗：用超大外阴影而不是四个遮片，少四个元素也少四份定位计算。 */
  .crop-box {
    position: absolute;
    border: 1px solid var(--pms-accent);
    box-shadow: 0 0 0 100vmax rgba(0, 0, 0, 0.45);
    pointer-events: none;
  }
</style>
