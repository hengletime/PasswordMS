// 截图时的窗口藏匿（D-164）。
//
// 为什么必须藏：用户点「截图」的动机，十有八九是「二维码就在我这个窗口后面」
// —— 网页开在浏览器里、App 开在手机投屏里。窗口不藏，覆盖层截到的就是自己的界面，
// 用户框出来的是 PasswordMS，不是那个码。
//
// 为什么要自己算恢复时机：`ms-screenclip:` 的覆盖层一旦出现，**屏幕内容就已经被冻结**
// 成一张快照，之后怎么显示都不影响用户框到的内容。所以不必等「用户按 Esc 还是松开鼠标」
// 那种拿不到的信号 —— 唤起后等一小会儿就可以把窗口放回来，用户视线这时在覆盖层上，无感。
// 这比「隐藏到截图结束」稳得多：用户取消截图时窗口不会一直消失。

import { getCurrentWindow } from '@tauri-apps/api/window';

/** 窗口消失后再唤起工具：留出动画时间，否则覆盖层截到的仍是还没消失的那一帧。 */
const HIDE_SETTLE_MS = 180;
/**
 * 唤起后等覆盖层出现并冻结画面，再恢复窗口。
 *
 * 1.2 秒是「肉眼看不到窗口消失」与「慢机器上覆盖层确实已出现」之间的折中：
 * 覆盖层通常 300ms 内出现，慢机器也很少超过 1 秒。
 */
const RESTORE_AFTER_MS = 1200;

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * 藏起主窗口 → 执行 `action`（唤起系统截图工具）→ 延时恢复窗口。
 *
 * 恢复放在 `finally` 里：`action` 抛错、被 abort、组件卸载，窗口都必须回来。
 * **一个藏起来就再也没回来的窗口，比截图失败严重得多。**
 *
 * 隐藏/显示失败不抛出：缺权限或非 Tauri 环境时，功能降级为「窗口不藏但截图照做」，
 * 由调用方按截图结果给提示。这里抛错只会让用户看到「截图不可用」——
 * 那是在用一个次要功能的失败，掩盖掉主要功能。
 */
export async function runWithWindowHidden(
  action: () => Promise<void>,
  restoreAfterMs = RESTORE_AFTER_MS,
): Promise<void> {
  const win = getCurrentWindow();
  let hidden = false;
  try {
    await win.hide();
    hidden = true;
  } catch {
    // 权限缺失 / 非 Tauri 运行时：不藏窗口也要把截图做下去。
  }
  if (hidden) {
    await delay(HIDE_SETTLE_MS);
  }
  try {
    await action();
    if (hidden) {
      await delay(restoreAfterMs);
    }
  } finally {
    if (hidden) {
      try {
        await win.show();
        await win.setFocus();
      } catch {
        // 恢复失败已经无处可退：此时窗口还在，用户至少能再点一次。
      }
    }
  }
}
