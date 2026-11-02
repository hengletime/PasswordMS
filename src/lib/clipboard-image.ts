// 剪贴板图片读取（D-163 / D-164）：桌面端「粘贴截图」入口与 Ctrl+V 兜底的公共底层。
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

/** 一次剪贴板读图的结果。调用方必须为每一种都给出可见反馈，不许静默。 */
export type ClipboardImage =
  | { kind: 'image'; blob: Blob }
  /** 剪贴板里没有位图（最常见：用户复制的是文字）。 */
  | { kind: 'empty' }
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
    const rgba = await image.rgba();
    const blob = await rgbaToPngBlob(width, height, rgba);
    return blob === null ? { kind: 'failed' } : { kind: 'image', blob };
  } catch {
    // 剪贴板没有位图时插件以 reject 表达。对用户来说「空的」和「读失败」
    // 是同一句话，于是统一成 empty；真正的异常（canvas 不可用）在上面单列。
    return { kind: 'empty' };
  }
}
