// 剪贴板图片读取（D-163）：桌面端「粘贴截图 / 截图」两个入口的公共底层。
//
// 为什么不只靠 paste 事件：paste 事件能不能拿到图，取决于两件都不由我们决定的事 ——
// ①宿主有没有把 Ctrl+V 派发给页面；②剪贴板里的 CF_DIB 有没有被转成 `image/png`。
// 桌面 WebView2 上这两条都不保证。原生 `readImage()` 不依赖任何事件，是桌面主力；
// paste 事件退化为「手机与浏览器 dev 的兜底」。
//
// ⚠️ `readImage()` 返回的是**未压缩的 RGBA 像素**，不是 PNG 文件内容。
// 插件文档示例把那串像素直接包进 Blob 当图片用 —— 那是错的：造出来的不是图片文件，
// `createImageBitmap` 会拒收。必须经 canvas 编码成真 PNG。
// （本条描述刻意不写那个字面量：守门测试用「全文件不出现该写法」作判据。）

import { readImage } from '@tauri-apps/plugin-clipboard-manager';

import { system } from './ipc';

/** 一次剪贴板读图的结果。调用方必须为每一种都给出可见反馈，不许静默。 */
export type ClipboardImage =
  | { kind: 'image'; blob: Blob }
  /** 剪贴板里没有位图（最常见：用户复制的是文字）。 */
  | { kind: 'empty' }
  /** 等新图等到超时（截图流程专用）。 */
  | { kind: 'timeout' }
  /** 当前环境读不了剪贴板图片（Android / 浏览器 dev）。 */
  | { kind: 'unsupported' }
  /** 平台支持但这次失败了（canvas 不可用等）。 */
  | { kind: 'failed' };

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
    const blob = await rgbaToPngBlob(width, height, await image.rgba());
    return blob === null ? { kind: 'failed' } : { kind: 'image', blob };
  } catch {
    // 剪贴板没有位图时插件以 reject 表达。对用户来说「空的」和「读失败」
    // 是同一句话，于是统一成 empty；真正的异常（canvas 不可用）在上面单列。
    return { kind: 'empty' };
  }
}

/** 轮询间隔：比人眼察觉慢一点，比反复读整张位图便宜得多。 */
const POLL_INTERVAL_MS = 400;
/** 序号变化后的小延时：截图工具是先置位、再填数据，抢跑会读到空。 */
const SETTLE_MS = 150;

/**
 * 等「剪贴板里出现一张新图」（截图流程专用）。
 *
 * 判据是**剪贴板序号变化**，不是定时读图：4K 截图一张 33MB，
 * 每秒读两次会把解码线程拖住。序号拿不到（非 Windows）时退化为
 * 「固定等一小会儿再读一次」—— 不精确，但不假装精确。
 *
 * `baseline` 由调用方在**唤起截图工具之前**取，取晚了就把新图当成旧图。
 */
export async function waitForClipboardImage(
  baseline: number | null,
  signal?: AbortSignal,
  timeoutMs = 60_000,
): Promise<ClipboardImage> {
  const aborted = (): boolean => signal?.aborted === true;
  if (baseline === null) {
    await sleep(800, signal);
    return aborted() ? { kind: 'timeout' } : await readClipboardImage();
  }
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (aborted()) {
      return { kind: 'timeout' };
    }
    await sleep(POLL_INTERVAL_MS, signal);
    if (aborted()) {
      return { kind: 'timeout' };
    }
    let seq: number | null = null;
    try {
      seq = await system.clipboardSequenceNumber();
    } catch {
      // 序号读取失败不该让整条流程失败：下一次轮询再试。
      continue;
    }
    if (seq !== null && seq !== baseline) {
      await sleep(SETTLE_MS, signal);
      return aborted() ? { kind: 'timeout' } : await readClipboardImage();
    }
  }
  return { kind: 'timeout' };
}

/** 可被 abort 打断的等待（关闭面板时不必陪跑到 60 秒）。 */
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
