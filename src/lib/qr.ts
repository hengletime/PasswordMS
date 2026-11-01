// 二维码扫码（PRD F-09 / U3：扫码为主、手动输入兜底）。
//
// 仅做「图像 → 文本」解码；二维码**渲染**由 Rust 侧 `qr_svg`（纯矢量）完成，
// 前端不引入任何图像编解码依赖（D-162 的图片路径用浏览器原生 `createImageBitmap` + canvas，
// 不像素级解码、不加依赖；摄像头设备与抓拍节奏放在 `qr-camera.ts`）。

import jsQR from 'jsqr';

import { QR_PREFIX } from './constants';

/** 解码前的最长边上限：够看清二维码，又不至于让 `getImageData` 吃满内存。 */
const MAX_QR_EDGE = 2000;
/** 放大重试后的最长边上限。 */
const MAX_QR_UPSCALED_EDGE = 2400;

/**
 * 从像素数据解码二维码文本。
 *
 * @returns 解码文本；未识别到二维码时返回 `null`。
 */
export function decodeQrFromImageData(
  data: Uint8ClampedArray,
  width: number,
  height: number,
): string | null {
  if (width <= 0 || height <= 0 || data.length < width * height * 4) {
    return null;
  }
  const result = jsQR(data, width, height);
  return result ? result.data : null;
}

/** 判定文本是否为 PasswordMS 配对载荷（`PMS1:` 前缀）。 */
export function isPairingPayload(text: string): boolean {
  return text.trim().startsWith(QR_PREFIX);
}

/**
 * 从图片文件（截图 / 相册）解码二维码（D-162）。
 *
 * 两步验证二维码最常见的形态是**就显示在用户自己这块屏幕上**（网站设置页开在浏览器里、
 * App 设置页开在手机上）。这时用摄像头去拍自己的屏幕是最难的拍摄条件（摩尔纹、反光、对焦），
 * 所以「从图片导入」不是扫码的兜底，而是桌面端与「同屏扫码」场景的主力路径。
 *
 * 现场经验带来的两条策略：
 * 1. **先限尺寸再解**：手机照片动辄 4000×3000，`getImageData` 一次就是 48MB，
 *    而二维码目标通常只占其中几百像素 —— 先等比缩到最长边 ≤2000 再解，成本降一个量级；
 * 2. **失败后放大 2× 重试**：截图里的码常常只有一两百像素，jsQR 的二值化在这种尺寸上
 *    容易丢掉定位图案；放大重采样后定位图案更连续，命中率明显上升。上限 2400 像素，
 *    避免放大把内存又吃回去。
 *
 * @returns 解出的文本；非图片、无二维码、图片过大解码失败均返回 `null`。
 */
export async function decodeQrFromBlob(blob: Blob): Promise<string | null> {
  const source = await loadQrSource(blob);
  if (source === null) {
    return null;
  }
  try {
    const maxEdge = Math.max(source.width, source.height);
    if (maxEdge <= 0) {
      return null;
    }
    const shrink = Math.min(1, MAX_QR_EDGE / maxEdge);
    const first = decodeSourceAt(source, shrink);
    if (first !== null) {
      return first;
    }
    const enlarge = Math.min(2, MAX_QR_UPSCALED_EDGE / maxEdge);
    return enlarge > 1 ? decodeSourceAt(source, enlarge) : null;
  } finally {
    // ImageBitmap 占的是原生内存（GC 不管它）；objectURL 不释放则整段生命周期都在泄漏。
    source.dispose();
  }
}

/** 一张已解码、可画的图源及其原始尺寸（含释放钩子）。 */
interface QrSource {
  image: CanvasImageSource;
  width: number;
  height: number;
  dispose: () => void;
}

/**
 * 把 Blob 变成可画的图源。
 *
 * 首选 `createImageBitmap`（快、直接给像素）；**失败时退回 `<img>`**：
 * 有些站点两步验证页直接内嵌 SVG，另存下来就是 `.svg`，而部分 WebView 的
 * `createImageBitmap` 不接 SVG —— 用 `<img>` 走的是同一条渲染管线，反而能画出来。
 * 两条都失败就是真的不是图片（或格式不支持），返回 `null` 由上层提示「没识别到二维码」。
 */
async function loadQrSource(blob: Blob): Promise<QrSource | null> {
  try {
    const bitmap = await createImageBitmap(blob);
    return {
      image: bitmap,
      width: bitmap.width,
      height: bitmap.height,
      dispose: (): void => bitmap.close(),
    };
  } catch {
    // 继续走 <img> 兜底。
  }
  const url = URL.createObjectURL(blob);
  const img = new Image();
  try {
    await new Promise<void>((resolve, reject): void => {
      img.onload = (): void => resolve();
      img.onerror = (): void => reject(new Error('image decode failed'));
      img.src = url;
    });
  } catch {
    URL.revokeObjectURL(url);
    return null;
  }
  return {
    image: img,
    width: img.naturalWidth,
    height: img.naturalHeight,
    dispose: (): void => URL.revokeObjectURL(url),
  };
}

/** 按比例把图源画到画布上再解码。 */
function decodeSourceAt(source: QrSource, scale: number): string | null {
  const w = Math.max(1, Math.round(source.width * scale));
  const h = Math.max(1, Math.round(source.height * scale));
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  if (!ctx) {
    return null;
  }
  ctx.drawImage(source.image, 0, 0, w, h);
  const image = ctx.getImageData(0, 0, w, h);
  return decodeQrFromImageData(image.data, w, h);
}

/**
 * 从 `<video>` 当前帧抓取并解码（运行时环境无摄像头权限时返回 `null`）。
 *
 * 先解画面中心 75% 裁剪帧：二维码贴满整帧边缘时 jsQR 反而难解（官方建议二维码与画面边缘留白），
 * 中心裁剪等效放大目标，对桌面定焦摄像头拍摄手机屏幕的场景尤其有效；
 * 未命中再回退全帧——覆盖「贴太近、二维码占满整屏」的反向场景。
 */
export function decodeQrFromVideo(video: HTMLVideoElement): string | null {
  const w = video.videoWidth;
  const h = video.videoHeight;
  if (w <= 0 || h <= 0) {
    return null;
  }
  const CROP = 0.75;
  const cw = Math.round(w * CROP);
  const ch = Math.round(h * CROP);
  const canvas = document.createElement('canvas');
  canvas.width = cw;
  canvas.height = ch;
  const ctx = canvas.getContext('2d');
  if (!ctx) {
    return null;
  }
  ctx.drawImage(video, (w - cw) / 2, (h - ch) / 2, cw, ch, 0, 0, cw, ch);
  const cropped = ctx.getImageData(0, 0, cw, ch);
  const fromCrop = decodeQrFromImageData(cropped.data, cw, ch);
  if (fromCrop !== null) {
    return fromCrop;
  }
  ctx.drawImage(video, 0, 0, w, h);
  const full = ctx.getImageData(0, 0, w, h);
  return decodeQrFromImageData(full.data, w, h);
}
