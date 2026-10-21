// 被动同步命中后的前端刷新判定（v1.2.4 根因 A）。
//
// 缺陷回顾：Host（电脑）侧的持久重同步监听器在 `spawn_resync_worker` 里轮询入站连接，
// 对端（手机）发起的重同步**把变更写进了本机库**，但当时只写了一条日志、没有通知前端。
// 结果：vault store 仍是同步前的条目列表 → 主人实测「手机删了某个附件、电脑上该附件还在」。
// 数据层完全正确（`crates/passwordms-core/tests/attachment_sync.rs` 已用双端 loopback 证明），
// 缺的是「后端告诉我库变了」这根线。
//
// 本模块把「事件名 + 载荷归一化 + 是否要刷新」抽成纯函数：
// 事件回调里只做一次判断，逻辑本身可被 vitest 直接覆盖（不依赖 window / tauri）。

/** 后端在被动同步完成后发出的事件名（`src-tauri/src/state.rs`）。 */
export const PEER_SYNC_APPLIED_EVENT = 'sync://applied';

/** 事件载荷：只带计数，不含任何明文或记录 ID。 */
export interface PeerSyncAppliedPayload {
  /** 本端应用（写入）的远端变更条数。 */
  applied: number;
  /** 新入库的附件 blob 数。 */
  blobs: number;
  /** 本次产生的冲突条数（`report.conflicts.len()`）。 */
  conflicts: number;
}

/** 非负整数归一化：缺字段 / 负数 / NaN / 非数字一律按 0 计。 */
function toCount(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return 0;
  }
  return Math.floor(value);
}

/** 把事件载荷归一化成结构化计数（对未知载荷保持健壮，不抛异常）。 */
export function parsePeerSyncApplied(payload: unknown): PeerSyncAppliedPayload {
  if (payload === null || typeof payload !== 'object') {
    return { applied: 0, blobs: 0, conflicts: 0 };
  }
  const record = payload as Record<string, unknown>;
  return {
    applied: toCount(record.applied),
    blobs: toCount(record.blobs),
    conflicts: toCount(record.conflicts),
  };
}

/**
 * 是否需要刷新本机 store。
 *
 * 判据：`applied` / `blobs` / `conflicts` 任一非零。三项全零 = 对端连上了但没有任何
 * 本端可见的变化（例如纯粹是本端把数据推给对方），此时无条件 reload 只会白刷一次列表
 * 并打断用户正在做的编辑。
 */
export function shouldReloadAfterPeerSync(payload: unknown): boolean {
  const counts = parsePeerSyncApplied(payload);
  return counts.applied > 0 || counts.blobs > 0 || counts.conflicts > 0;
}
