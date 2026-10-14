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
