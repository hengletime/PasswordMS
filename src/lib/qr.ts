// 二维码扫码（PRD F-09 / U3：扫码为主、手动输入兜底）。
//
// 仅做「图像 → 文本」解码；二维码**渲染**由 Rust 侧 `qr_svg`（纯矢量）完成，
// 前端不引入任何图像编解码依赖。

import jsQR from 'jsqr';

import { QR_PREFIX } from './constants';

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
 * 从 `<video>` 当前帧抓取并解码（运行时环境无摄像头权限时返回 `null`）。
 */
export function decodeQrFromVideo(video: HTMLVideoElement): string | null {
  const w = video.videoWidth;
  const h = video.videoHeight;
  if (w <= 0 || h <= 0) {
    return null;
  }
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext('2d');
  if (!ctx) {
    return null;
  }
  ctx.drawImage(video, 0, 0, w, h);
  const image = ctx.getImageData(0, 0, w, h);
  return decodeQrFromImageData(image.data, w, h);
}
