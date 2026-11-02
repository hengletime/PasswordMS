// 剪贴板图片读取（D-163 / D-164）：桌面端「粘贴截图 / 截图」两个入口的公共底层。
//
// 为什么不只靠 paste 事件：paste 事件能不能拿到图，取决于两件都不由我们决定的事 ——
// ①宿主有没有把 Ctrl+V 派发给页面；②宿主有没有把剪贴板里的位图转成 `image/*` 文件。
// 桌面 WebView2 上第二条尤其不可靠：**粘贴事件照常派发，但 `DataTransfer.items` 里空无一物**。
// 原生 `readImage()` 不依赖任何事件，是桌面主力（D-164 的根因就在这里）。
//
// ⚠️ `readImage()` 返回的是**未压缩的 RGBA 像素**，不是 PNG 文件内容。
// 插件文档示例把那串像素直接包进 Blob 当图片用 —— 那是错的：造出来的不是图片文件，
// `createImageBitmap` 会拒收。必须经 canvas 编码成真 PNG。
// （本条描述刻意不写那个字面量：守门测试用「全文件不出现该写法」作判据。）

import { readImage } from '@tauri-apps/plugin-clipboard-manager';

import { system } from './ipc';

/** 一次剪贴板读图的结果。调用方必须为每一种都给出可见反馈，不许静默。 */
export type ClipboardImage =
  /** `fingerprint` 用来判断「这是不是截图前那张旧图」，见 `waitForClipboardImage`。 */
  | { kind: 'image'; blob: Blob; fingerprint: string }
  /** 剪贴板里没有位图（最常见：用户复制的是文字）。 */
  | { kind: 'empty' }
  /** 等新图等到超时（截图流程专用）。 */
  | { kind: 'timeout' }
  /** 当前环境读不了剪贴板图片（Android / 浏览器 dev）。 */
  | { kind: 'unsupported' }
  /** 平台支持但这次失败了（canvas 不可用等）。 */
  | { kind: 'failed' };

/** 唤起截图工具前的剪贴板状态，用来判断「之后读到的是不是新图」。 */
export interface ClipboardBaseline {
  /** 剪贴板序号；非 Windows 或命令失败时为 null（此时改用指纹轮询）。 */
  seq: number | null;
  /** 唤起前那张图的指纹；原本没有图为 null。 */
  fingerprint: string | null;
}

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * 当前环境能不能直接读剪贴板图片。
 *
 * Android / iOS 必须排除：插件文档对 `readImage` 明确标注
 * **Platform-specific: Not supported**。手机上的替代路径是「从图片」——
 * 手机截图本来就落在相册里，那条路反而更短。
 */
export function clipboardImageSupported(): boolean {
  if (!isTauriRuntime()) {
    return false;
  }
  return !/Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
}

/**
 * 位图指纹：尺寸 + 均匀抽样的若干像素的 FNV-1a。
 *
 * 目的只有一个 —— 回答「这两次读到的是同一张图吗」。所以**不需要密码学强度**，
 * 也不做全量哈希：4K 一张 33MB，逐字节算一次就是几十毫秒且毫无必要。
 * 抽样步长按总面积除以 256 取，保证任何尺寸都只碰 256 个点。
 *
 * 导出是为了能写行为测试：只断言「代码里调了 mix(rgba)」挡不住有人加一行
 * 提前 return —— 那种改动文本判据看不见，但用户会看到「截图永远被当成旧图」。
 */
export function fingerprintOf(width: number, height: number, rgba: Uint8Array): string {
  const pixels = width * height;
  if (pixels <= 0 || rgba.length < pixels * 4) {
    return `invalid:${width}x${height}`;
  }
  let hash = 2166136261;
  const mix = (value: number): void => {
    hash ^= value;
    hash = Math.imul(hash, 16777619);
  };
  mix(width);
  mix(height);
  const step = Math.max(1, Math.floor(pixels / 256));
  for (let i = 0; i < pixels; i += step) {
    const at = i * 4;
    mix(rgba[at]);
    mix(rgba[at + 1]);
    mix(rgba[at + 2]);
  }
  return `${(hash >>> 0).toString(16)}:${width}x${height}`;
}

/** 把未压缩 RGBA 像素编码成 PNG。返回 `null` 表示本环境拿不到 2D 上下文。 */
async function rgbaToPngBlob(
  width: number,
  height: number,
  rgba: Uint8Array,
): Promise<Blob | null> {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('2d');
  if (ctx === null) {
    return null;
  }
  // `ImageData` 要求 Uint8ClampedArray，插件给的是 Uint8Array（运行期同构，
  // 类型不同）。共享同一块缓冲，不做二次拷贝 —— 4K 一张就是 33MB。
  // `as ArrayBuffer`：`Uint8Array.buffer` 的类型是 `ArrayBufferLike`，包含
  // SharedArrayBuffer；插件给的是普通 ArrayBuffer，断言只是把这个 TS 无法
  // 静态排除的可能性按实际情况收窄，不是掩盖错误。
  const pixels = new Uint8ClampedArray(
    rgba.buffer as ArrayBuffer,
    rgba.byteOffset,
    rgba.byteLength,
  );
  try {
    ctx.putImageData(new ImageData(pixels, width, height), 0, 0);
  } catch {
    // 长度与 w*h*4 不符（插件侧异常）→ 当作读不到，别把半张图送去解码。
    return null;
  }
  return await new Promise<Blob | null>((resolve) => {
    canvas.toBlob((blob) => resolve(blob), 'image/png');
  });
}

/** 读一次剪贴板里的图片。 */
export async function readClipboardImage(): Promise<ClipboardImage> {
  if (!clipboardImageSupported()) {
    return { kind: 'unsupported' };
  }
  try {
    const image = await readImage();
    const { width, height } = await image.size();
    if (width <= 0 || height <= 0) {
      return { kind: 'empty' };
    }
    const rgba = await image.rgba();
    const blob = await rgbaToPngBlob(width, height, rgba);
    return blob === null
      ? { kind: 'failed' }
      : { kind: 'image', blob, fingerprint: fingerprintOf(width, height, rgba) };
  } catch {
    // 剪贴板没有位图时插件以 reject 表达。对用户来说「空的」和「读失败」
    // 是同一句话，于是统一成 empty；真正的异常（canvas 不可用）在上面单列。
    return { kind: 'empty' };
  }
}

/**
 * 记下「唤起截图工具之前」的剪贴板状态。
 *
 * 必须**在唤起之前**调用：取晚了会把刚截好的新图当成旧图，于是死等到超时 ——
 * 一次「偶尔失灵」的经典来路。
 */
export async function captureClipboardBaseline(): Promise<ClipboardBaseline> {
  let seq: number | null = null;
  try {
    seq = await system.clipboardSequenceNumber();
  } catch {
    // 序号只是加速信号，拿不到就走指纹轮询，不该让整条流程失败。
    seq = null;
  }
  const before = await readClipboardImage();
  return { seq, fingerprint: before.kind === 'image' ? before.fingerprint : null };
}

/** 轮询间隔：比人眼察觉慢一点，比反复读整张位图便宜得多。 */
const POLL_INTERVAL_MS = 400;
/** 序号变化后的小延时：截图工具是先置位、再填数据，抢跑会读到空。 */
const SETTLE_MS = 150;
/** 没有序号信号时的直读节奏。读一次 4K 就是 33MB，不能按 400ms 来。 */
const FALLBACK_READ_INTERVAL_MS = 1200;
/** 等待上限。窗口此时已经恢复，用户随时能重试，不必陪跑一分钟。 */
const WAIT_TIMEOUT_MS = 30_000;

/**
 * 等「剪贴板里出现一张新图」（截图流程专用）。
 *
 * 两级判据，**任何一级单独失效都不至于卡死**：
 * 1. 首选剪贴板序号（O(1)）。它变了才去读像素 —— 读整图代价太大，不能盲读。
 * 2. 序号拿不到（非 Windows / 命令失败 / 权限变更）时，退化为按时直读 + 指纹比对。
 *
 * D-164 之前的写法是「序号为 null 就硬等 800ms 读一次」——用户框选要几秒，
 * 800ms 必然读到空剪贴板，于是「点了截图、框完、没反应」。**退化路径不能只有一次机会。**
 *
 * 另一个必答的问题：序号变了但读到的还是截图前那张图（工具先置位后填数据）。
 * 这时不能把旧图当结果交出去，要继续等 —— 否则用户会看到「上一个二维码」被填进来。
 */
export async function waitForClipboardImage(
  baseline: ClipboardBaseline,
  signal?: AbortSignal,
  timeoutMs = WAIT_TIMEOUT_MS,
): Promise<ClipboardImage> {
  const aborted = (): boolean => signal?.aborted === true;
  const deadline = Date.now() + timeoutMs;
  // 没有序号信号 → 从一开始就直读。有序号 → 等它变化后才开始读。
  let armed = baseline.seq === null;
  let lastRead = 0;

  while (Date.now() < deadline) {
    if (aborted()) {
      return { kind: 'timeout' };
    }
    await sleep(POLL_INTERVAL_MS, signal);
    if (aborted()) {
      return { kind: 'timeout' };
    }

    if (!armed && baseline.seq !== null) {
      let seq: number | null = null;
      try {
        seq = await system.clipboardSequenceNumber();
      } catch {
        // 单次失败不该判死：下一轮再试。真的一直失败就等满超时，由调用方提示。
        continue;
      }
      if (seq === null || seq === baseline.seq) {
        continue;
      }
      armed = true;
      await sleep(SETTLE_MS, signal);
      if (aborted()) {
        return { kind: 'timeout' };
      }
    }

    const interval =
      baseline.seq === null ? FALLBACK_READ_INTERVAL_MS : POLL_INTERVAL_MS;
    if (Date.now() - lastRead < interval) {
      continue;
    }
    lastRead = Date.now();

    const got = await readClipboardImage();
    if (got.kind === 'empty') {
      // 序号刚变、数据还没落盘；或者用户还没框选。继续等，别急着报「没有图片」。
      continue;
    }
    if (got.kind === 'image' && got.fingerprint === baseline.fingerprint) {
      // 还是截图前那张：说明变化来自别处（复制了文字等），继续等。
      continue;
    }
    return got;
  }
  return { kind: 'timeout' };
}

/** 可被 abort 打断的等待（关闭面板时不必陪跑到超时）。 */
function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal?.aborted === true) {
      resolve();
      return;
    }
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', onAbort);
      resolve();
    }, ms);
    function onAbort(): void {
      clearTimeout(timer);
      resolve();
    }
    signal?.addEventListener('abort', onAbort, { once: true });
  });
}
