// D-75：`pairing://ended` 事件 → 前端下一个状态。
//
// 缺陷（主人真机实测）：手机扫码成功后，手机端弹「请核对电脑上的会话码」，而**电脑上的
// 配对面板在这一瞬间已经关闭了**，会话码随之消失；重新生成二维码 → 又是新会话、新码。
// ⇒ D-69 的「两屏显示同一次会话的会话码、防中间人」**从未真正可用**。
//
// 根因不是显示 bug，是**时序互斥**：后端在两种结局都发同一个事件
//   `crates` 侧 sync_cmd.rs：`emit("pairing://ended", "completed")`（成功）
//                              `emit("pairing://ended", "aborted")`（失败/超时）
// 前端原先**无条件** `sync.pairing = null`。而会话码挂在 `sync.pairing.session_code` 上 ⇒
// 「会话结束」这个事件把「核对会话码」这个动作的前提一起删掉了。
//
// 修法：把「payload → 下一个状态」的判断抽成**纯函数**（本文件），store 只负责套用。
// 这样既能单测，也不会再有人把「无条件清空」写回去 —— 那条路径在 `pairing-state.test.ts`
// 里有结构守门（handler 必须先 `nextPairingState(...)` 再决定是否清空）。

/** `pairing://ended` 的事件名（前端监听与后端 emit 必须逐字一致）。 */
export const PAIRING_ENDED_EVENT = 'pairing://ended';

/** 「配对成功」的载荷取值（后端 `sync_cmd.rs` 成功分支）。 */
export const PAIRING_PAYLOAD_COMPLETED = 'completed';

/**
 * 事件载荷 → 前端应进入的状态。
 *
 * 三个字段**互斥且必须成组解释**，不许只看其中一个：
 * - `clearPairing`：是否清空 `sync.pairing`（= 收回二维码 / 配对文本 / 有效期）。
 * - `completed`：是否进入「配对成功，待人工核对会话码」态。
 * - `ended`：是否进入「会话已结束，请重新生成二维码」态。
 */
export interface NextPairingState {
  /** 是否清空 `sync.pairing`。成功时为 `false`（会话码要留在屏上供核对）。 */
  clearPairing: boolean;
  /** 是否进入成功态（`pairCompleted`）。 */
  completed: boolean;
  /** 是否进入失败/结束态（`pairEnded`）。 */
  ended: boolean;
}

/**
 * 把 `pairing://ended` 的载荷翻译成前端下一个状态。
 *
 * @param payload 事件载荷（后端只发 `"completed"` / `"aborted"`；类型收 `unknown`
 *                是因为 Tauri 的 `event.payload` 本身是 `unknown`，这里顺手把
 *                「运行期可能不是字符串」也一起兜住）
 * @returns 下一个状态；**未知载荷一律按失败处理**（清屏、不进成功态）
 */
export function nextPairingState(payload: unknown): NextPairingState {
  // 成功：**保留** pairing —— 会话码还要给用户逐字核对。
  if (payload === PAIRING_PAYLOAD_COMPLETED) {
    return { clearPairing: false, completed: true, ended: false };
  }
  // 其余（`"aborted"` / 空串 / null / 未知识别符）→ 保守处理：
  // 宁可让用户重新生成一次二维码，也不留一个「看起来成功了、其实没核对」的假成功态。
  return { clearPairing: true, completed: false, ended: true };
}
