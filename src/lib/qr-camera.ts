// 摄像头扫码（D-162 抽取自 `sync/PairScan.svelte`；配对与两步验证扫码共用）。
//
// 抽出来的原因不是"两处代码长得像"，而是**生命周期必须由一处统一负责**：
// 摄像头是一份独占的系统资源，谁 start 谁必须 stop。原先配对页把
// `getUserMedia` / `setInterval` / `track.stop()` 全写在组件里，组件卸载时无人收尾
// —— 离开配对页后摄像头指示灯还亮着（D-162 一并修）。
//
// 与 `qr.ts` 的分工：那边只做「图像 → 文本」（纯函数，可离屏测）；这里管设备与节奏。
// 想让解码逻辑可测，就别把 DOM 设备混进同一个文件。

import { decodeQrFromVideo } from './qr';

/** 摄像头打不开的三种情形：权限被拒 / 没有设备 / 别的（驱动、被占用等）。 */
export type CameraErrorKind = 'denied' | 'unavailable' | 'other';

export interface CameraScanOptions {
  video: HTMLVideoElement;
  /**
   * 每解出一帧文本调用一次。
   *
   * 返回 `true` 表示「这一条我用了」，扫描自动停止（避免同一张码反复触发）；
   * 返回 `false` 表示「不是我要的，继续扫」——摄像头视野里常有别的二维码/条码，
   * 一律当成错误会让用户以为坏了。
   */
  onText: (text: string) => boolean;
  onError: (kind: CameraErrorKind) => void;
  /** 抓拍间隔，默认 250ms（jsQR 单帧 720p 约 3–8ms，5fps 足够跟手）。 */
  intervalMs?: number;
}

export interface CameraScan {
  /** 停止扫描并释放摄像头。可重复调用。 */
  stop(): void;
}

/**
 * 打开摄像头并开始抓帧解码。
 *
 * **必须**在组件卸载时调用返回值的 `stop()`（Svelte 里用 `$effect` 的清理函数）。
 * 即使 `getUserMedia` 还在等用户点「允许」时就被取消，也会在授权到达后立刻释放，
 * 不留悬挂的 stream。
 */
export async function startCameraScan(options: CameraScanOptions): Promise<CameraScan> {
  const { video, onText, onError, intervalMs = 250 } = options;

  let stream: MediaStream | null = null;
  let handle = 0;
  let stopped = false;

  const release = (): void => {
    if (handle !== 0) {
      clearInterval(handle);
      handle = 0;
    }
    if (stream !== null) {
      stream.getTracks().forEach((track): void => track.stop());
      stream = null;
    }
    // 断开引用：否则 video 元素会一直握着已停止的流（部分平台上表现为黑帧不刷新）。
    if (video.srcObject !== null) {
      video.srcObject = null;
    }
  };

  const stop = (): void => {
    stopped = true;
    release();
  };

  try {
    // 分辨率给 ideal 约束：桌面定焦摄像头拍手机屏幕时，高分辨率帧显著提高 jsQR 识别率（D-40）。
    stream = await navigator.mediaDevices.getUserMedia({
      video: { facingMode: 'environment', width: { ideal: 1280 }, height: { ideal: 720 } },
    });
  } catch (err) {
    const name = err instanceof DOMException ? err.name : '';
    if (name === 'NotAllowedError' || name === 'PermissionDeniedError') {
      onError('denied');
    } else if (name === 'NotFoundError' || name === 'DevicesNotFoundError') {
      onError('unavailable');
    } else {
      onError('other');
      console.error('[qr-camera] getUserMedia failed', err);
    }
    return { stop };
  }

  // 授权期间就被取消（用户已离开该界面）：立刻还回去，别让灯亮着。
  if (stopped) {
    release();
    return { stop };
  }

  video.srcObject = stream;
  try {
    await video.play();
  } catch {
    // 自动播放被拦不致命：后续 drawImage 仍会拉取已解码帧，部分环境需用户手势才出画。
  }

  if (stopped) {
    release();
    return { stop };
  }

  handle = window.setInterval((): void => {
    if (stopped || video.readyState < 2) {
      return;
    }
    const text = decodeQrFromVideo(video);
    if (text !== null && onText(text)) {
      stop();
    }
  }, intervalMs);

  return { stop };
}
