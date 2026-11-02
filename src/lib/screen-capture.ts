// 应用内抓屏（D-165）：桌面端「截图」入口的底层。
//
// 为什么不再用系统截图工具（`ms-screenclip:`）：它的覆盖层是**活的**。
// 上一版以为"覆盖层一出现，屏幕就冻结成快照"，据此在唤起后 1.2 秒把窗口还原 ——
// 结果窗口就出现在覆盖层底下，用户还没开始框，画面里已经有它了。
// 系统工具没有任何"截完通知我"的接口，我们只能在「窗口藏到底」和「窗口及时回来」之间二选一。
//
// 自己抓屏把这对矛盾解开：**先抓、再还原**。抓到的像素是死的，用户在它上面框选，
// 窗口什么时候回来都不影响框到的内容。

import { system } from './ipc';

/** 头 8 字节：宽、高各 u32 小端；其后是 RGBA 像素。 */
const HEADER_BYTES = 8;

/** 框选短边小于这个像素数，视为误触而不是一次框选（二维码不可能这么小）。 */
export const MIN_CROP_PX = 8;

/** 一张已经抓下来、**不会变**的屏幕快照。 */
export interface ScreenShot {
  /** 物理像素宽高（等于虚拟桌面尺寸，不是 CSS 像素）。 */
  width: number;
  height: number;
  /** 冻结的画面本身。裁切与解码都基于它。 */
  bitmap: ImageBitmap;
}

/** 框选区域，单位是**图像像素**（不是屏幕坐标，也不是 CSS 像素）。 */
export interface CropRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** 一次抓屏的结局。调用方必须为每一种给出可见反馈（不许静默）。 */
export type CaptureResult =
  | { kind: 'ok'; shot: ScreenShot }
  /** 当前平台没有这条路（非 Windows / 浏览器 dev）。 */
  | { kind: 'unsupported' }
  /** 平台支持但这次失败（GDI 失败、内存不足）。 */
  | { kind: 'failed' };

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * 抓一张整屏快照。
 *
 * 调用方**必须先把自己的窗口藏起来**再调：抓的是屏幕内容，窗口还在就抓到自己了。
 * 这里不做隐藏 —— 窗口操作属于调用方的时序（藏 → 等 → 抓 → 还原），
 * 在这里再藏一次会让两处各自以为自己在管这件事。
 */
export async function captureScreen(): Promise<CaptureResult> {
  if (!isTauriRuntime()) {
    return { kind: 'unsupported' };
  }
  let buffer: ArrayBuffer;
  try {
    buffer = await system.captureScreen();
  } catch {
    // 非 Windows 是 ERR_NOT_SUPPORTED，Windows 上是 GDI 失败。对用户都是
    // 「这次没截到」，分成两句只会让人以为其中一种有救。
    return { kind: 'failed' };
  }
  if (buffer.byteLength < HEADER_BYTES) {
    return { kind: 'failed' };
  }
  const view = new DataView(buffer);
  const width = view.getUint32(0, true);
  const height = view.getUint32(4, true);
  if (width === 0 || height === 0) {
    return { kind: 'failed' };
  }
  const expected = width * height * 4;
  // 长度必须**正好够**：短了说明 IPC 被截断，硬解码会读出半张图（下半截是黑）。
  if (buffer.byteLength - HEADER_BYTES < expected) {
    return { kind: 'failed' };
  }
  try {
    const pixels = new Uint8ClampedArray(buffer, HEADER_BYTES, expected);
    const bitmap = await createImageBitmap(new ImageData(pixels, width, height));
    return { kind: 'ok', shot: { width, height, bitmap } };
  } catch {
    return { kind: 'failed' };
  }
}

/**
 * 把框选区域裁到图像范围内，并挡掉误触。
 *
 * 返回 `null` 表示「这次框选不成立」（太小，或者完全在图外），调用方据此
 * **保留框选层**让用户重框 —— 而不是把空 blob 送去解码、报一句「没识别到二维码」，
 * 让人以为是二维码的问题。
 *
 * 单独拆成纯函数是为了能写行为测试：拖到图外、拖成一条线、反向拖，
 * 这几种都只有真喂坐标才测得出。
 *
 * **反向拖也要成立**（从右下拖回左上时宽高是负数）：调用方虽然已经归一化过，
 * 这里仍按两个角点取 min/max —— 一个「只接受正数」的函数会让反向拖静默失效，
 * 而那种失效在界面上的表现是「拖了但没反应」。
 */
export function clampRect(rect: CropRect, width: number, height: number): CropRect | null {
  const ax = Math.max(0, Math.min(width, Math.round(rect.x)));
  const ay = Math.max(0, Math.min(height, Math.round(rect.y)));
  const bx = Math.max(0, Math.min(width, Math.round(rect.x + rect.width)));
  const by = Math.max(0, Math.min(height, Math.round(rect.y + rect.height)));
  const left = Math.min(ax, bx);
  const top = Math.min(ay, by);
  const w = Math.abs(bx - ax);
  const h = Math.abs(by - ay);
  if (w < MIN_CROP_PX || h < MIN_CROP_PX) {
    return null;
  }
  return { x: left, y: top, width: w, height: h };
}

/** 裁出框选区域并编码成 PNG。`null` 表示框选不成立或本环境拿不到 2D 上下文。 */
export async function cropShotToBlob(shot: ScreenShot, rect: CropRect): Promise<Blob | null> {
  const area = clampRect(rect, shot.width, shot.height);
  if (area === null) {
    return null;
  }
  const canvas = document.createElement('canvas');
  canvas.width = area.width;
  canvas.height = area.height;
  const ctx = canvas.getContext('2d');
  if (ctx === null) {
    return null;
  }
  ctx.drawImage(
    shot.bitmap,
    area.x,
    area.y,
    area.width,
    area.height,
    0,
    0,
    area.width,
    area.height,
  );
  return await new Promise<Blob | null>((resolve) => {
    canvas.toBlob((blob) => resolve(blob), 'image/png');
  });
}

/**
 * 释放快照占的像素内存（4K 一张 33MB）。
 *
 * 必须在三条路径上都调：填入成功、用户取消、组件卸载。漏一条就是一块
 * 33MB 的裸内存挂在 `ImageBitmap` 上等 GC —— 而它不会立刻被回收。
 */
export function disposeShot(shot: ScreenShot | null): void {
  shot?.bitmap.close();
}
