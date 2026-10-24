//! O2：三段式同步的仓储访问器抽象。
//!
//! 背景（审查 🟠-2）：被动/主动同步全程持有全局 session 锁横跨网络 I/O（握手 30s/片
//! 读超时 + blob 分钟级），而 Tauri 同步命令跑主线程 ⇒ 同步期间一切 IPC 排队，UI 整体
//! 冻结。
//!
//! 三段式改造后，交换过程只在**需要读写库的瞬间**通过本 trait 进入锁内段（毫秒级），
//! 网络 I/O 全程不持任何会话锁。中途会话被锁（自动锁定竞态）⇒ `with_repo` 返回
//! `Err(NotUnlocked)` ⇒ 整段交换丢弃、对端下轮重发（宁多勿丢；密文暂存随连接丢弃，
//! 无明文残留）。

use crate::error::CoreResult;
use crate::store::repository::Repository;

/// 一次锁内仓储作业。返回值经闭包捕获的输出变量带出（`FnOnce`，可 move 进持久化
/// 所需的状态）。
pub type RepoJob<'a> = Box<dyn FnOnce(&mut Repository) -> CoreResult<()> + 'a>;

/// 仓储访问器：锁内段入口。
pub trait RepoAccess {
    /// 在会话锁内执行一次 `job`。会话未解锁 / 锁中毒 → `Err`（交换整体放弃）。
    fn with_repo(&mut self, job: RepoJob<'_>) -> CoreResult<()>;
}

/// 直连实现：repo 已由调用方持锁（主动路径 `sync_now` / 既有单段式调用方 / 测试用）。
pub struct DirectAccess<'a>(pub &'a mut Repository);

impl RepoAccess for DirectAccess<'_> {
    fn with_repo(&mut self, job: RepoJob<'_>) -> CoreResult<()> {
        job(self.0)
    }
}
