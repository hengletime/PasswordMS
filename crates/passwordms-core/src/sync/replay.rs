//! 防重放（架构文档 §3.7，S4 / A-08）。
//!
//! 接收端维护 `last_recv_seq`，拒绝任何 `seq ≤ last_recv_seq` 的报文；
//! `ts` 必须落在 `[now-window, now+window]`；越界一律 [`CoreError::ReplayDetected`]。

use crate::error::{CoreError, CoreResult};

/// 序号 / 时间窗守卫。
#[derive(Debug, Clone)]
pub struct ReplayGuard {
    last_recv_seq: u64,
    window_secs: i64,
}

impl ReplayGuard {
    /// 新建（`last_recv_seq` 从 0 起）。
    pub fn new(window_secs: i64) -> Self {
        Self {
            last_recv_seq: 0,
            window_secs,
        }
    }

    /// 从持久化状态恢复。
    pub fn with_state(last_recv_seq: u64, window_secs: i64) -> Self {
        Self {
            last_recv_seq,
            window_secs,
        }
    }

    /// 当前已处理的最大序号。
    pub fn last_recv_seq(&self) -> u64 {
        self.last_recv_seq
    }

    /// 校验序号单调递增（`seq ≤ last` 即重放）。
    pub fn check_seq(&self, seq: u64) -> CoreResult<()> {
        if seq <= self.last_recv_seq {
            return Err(CoreError::ReplayDetected);
        }
        Ok(())
    }

    /// 校验时间窗（±`window_secs`）。
    pub fn check_ts(&self, ts_ms: i64, now_ms: i64) -> CoreResult<()> {
        let window_ms = self.window_secs.saturating_mul(1000);
        if ts_ms < now_ms - window_ms || ts_ms > now_ms + window_ms {
            return Err(CoreError::ReplayDetected);
        }
        Ok(())
    }

    /// 组合校验并在通过后推进 `last_recv_seq`。
    pub fn accept(&mut self, seq: u64, ts_ms: i64, now_ms: i64) -> CoreResult<()> {
        self.check_seq(seq)?;
        self.check_ts(ts_ms, now_ms)?;
        self.last_recv_seq = seq;
        Ok(())
    }
}
