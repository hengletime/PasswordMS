// D-108（豆包审查 3.4-6）：auto-lock 的 Rust 兜底事件。
//
// 后端 `spawn_auto_lock_worker`（state.rs）在闲置达档位时执行锁定级联并 emit
// 本事件；前端（App.svelte，常驻组件）收到后复位三个 store 并切锁屏路由——
// 与手动 `lockNow()` 的收尾动作一致（后端已锁定，故不再调 IPC）。

/** Rust 侧 auto-lock 兜底触发的事件名。 */
export const AUTOLOCK_LOCKED_EVENT = 'autolock://locked';
