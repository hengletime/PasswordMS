//! 系统类命令：状态、剪贴板、打开网址、文件信息（架构文档 §5.2）。

use passwordms_core::session::Session;
use passwordms_core::url_safety;
use tauri::Manager;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;
use zeroize::Zeroizing;

use crate::dto::{CompactionResultDto, CompactionStatsDto, StatusDto, VaultFileInfoDto};
use crate::error::{lock_err, IpcError, IpcResult};
use crate::state::{append_log, vault_path, AppState};

/// 启动判定：是否存在保险库、当前是否锁定。
#[tauri::command(rename_all = "snake_case")]
pub fn get_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> IpcResult<StatusDto> {
    let path = vault_path(&app)?;
    let session = state.session.lock().map_err(lock_err)?;
    Ok(StatusDto {
        has_vault: Session::has_vault(&path),
        locked: !session.is_unlocked(),
    })
}

/// 写入剪贴板（F-06）。
///
/// 写入的同时把该内容以 [`Zeroizing`] 载体记入应用状态，供到期时**比对**用（A-03：
/// 仅清空本应用写入的内容）。30 秒倒计时由前端 `clipboard.ts` 负责调度。
///
/// 【D-96】Rust 兜底：前端倒计时可能因页面休眠 / WebView 冻结 / 事件丢失而不触发，
/// 这里在写入成功后另起 **detached 线程**等用户配置的 TTL 秒再调
/// [`clear_owned_clipboard`]。两者是幂等协作：take 语义保证只有第一个到期者真正
/// 清理，另一侧自然空转；用户若已复制别的内容，比对不相等则保持不动。
#[tauri::command(rename_all = "snake_case")]
pub fn copy_secret(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    text: String,
    ttl_secs: u16,
) -> IpcResult<()> {
    // D-106（豆包审查 3.4-5）：兜底清理按用户配置的 TTL 睡眠——原实现丢弃参数
    // 恒等 30s，用户配 60s 时密码在第二次粘贴前被提前清掉（违背配置预期）。
    app.clipboard()
        .write_text(text.clone())
        .map_err(|_| IpcError::internal("clipboard_write"))?;
    let mut guard = state.clipboard.lock().map_err(lock_err)?;
    *guard = Some(Zeroizing::new(text));
    drop(guard);
    spawn_clipboard_fallback(app, ttl_secs);
    Ok(())
}

/// 【D-96】剪贴板兜底清理线程（detached）：等 `ttl_secs` 秒后调
/// [`clear_owned_clipboard`]。不 join、不持锁跨越 sleep；应用退出时线程随之消亡，
/// 残留剪贴板由 [`crate::run`] 的 `RunEvent::Exit` 钩子清理。
/// **D-106**：TTL 改为前端传入的用户配置值（前端已钳制 5–120s；30s 常量仅作默认）。
fn spawn_clipboard_fallback(app: tauri::AppHandle, ttl_secs: u16) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(u64::from(ttl_secs)));
        if let Some(state) = app.try_state::<AppState>() {
            let _ = clear_owned_clipboard(&app, state.inner());
        }
    });
}

/// A-03 核心逻辑：**仅当**剪贴板内容仍为本应用写入的内容时才清空；并在任何分支下
/// 都消费掉记忆值（`take` 语义，避免旧值长期驻留）。
///
/// 抽成公开函数以便锁定路径复用（安全需求 §3.3「锁定兼容」）。实现放在 Rust 侧，
/// 避免把剪贴板明文再回传 JS（缩小泄露面）：
/// 1. 取出并清空「上次写入」的记忆值；
/// 2. 读取当前剪贴板文本；
/// 3. 相等 → 写空串并返回 `true`；不等 → **不动**用户内容，返回 `false`。
///
/// 语义（既有行为，QA 已复核，不得更改）：
/// - 记忆值为 `None` → `Ok(false)`；
/// - 读取剪贴板失败 → `Ok(false)` 保守不清；
/// - 内容不等 → 保持不动、`Ok(false)`；
/// - 内容相等 → 写空串、`Ok(true)`。
pub fn clear_owned_clipboard(app: &tauri::AppHandle, state: &AppState) -> IpcResult<bool> {
    let remembered = {
        let mut guard = state.clipboard.lock().map_err(lock_err)?;
        guard.take()
    };
    let Some(own) = remembered else {
        return Ok(false);
    };
    let current = match app.clipboard().read_text() {
        Ok(text) => text,
        // 读取失败（无权限 / 平台不支持）时不误清，保守返回 false。
        Err(_) => return Ok(false),
    };
    if current == own.as_str() {
        app.clipboard()
            .write_text(String::new())
            .map_err(|_| IpcError::internal("clipboard_write"))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// A-03：到期清空剪贴板——**仅当剪贴板内容仍为本应用写入的内容**时才清空。
///
/// 薄壳：实际逻辑见 [`clear_owned_clipboard`]（锁定路径亦复用同一逻辑）。
#[tauri::command(rename_all = "snake_case")]
pub fn clear_secret_if_own(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> IpcResult<bool> {
    clear_owned_clipboard(&app, state.inner())
}

/// 打开外部网址（F-05）：仅放行 http/https。
#[tauri::command(rename_all = "snake_case")]
pub fn open_url(app: tauri::AppHandle, url: String) -> IpcResult<()> {
    let safe = url_safety::normalize_and_validate(&url)?;
    app.opener()
        .open_url(safe, None::<&str>)
        .map_err(|_| IpcError::internal("open_url"))
}

/// 保险库文件信息（P-10 展示）。
#[tauri::command(rename_all = "snake_case")]
pub fn vault_file_info(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> IpcResult<VaultFileInfoDto> {
    let path = vault_path(&app)?;
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    let size_bytes = repo.file_size().map_err(IpcError::from)?;
    Ok(VaultFileInfoDto {
        path: path.to_string_lossy().to_string(),
        size_bytes,
    })
}

/// 保险库体积构成统计（D-76，**只读**：不解密 blob、不写任何字节）。
///
/// 回答的是「设置里显示的 24 MB 里，有多少是已删附件留下的死数据」——它不需要压缩
/// 就能给结论，因此先于压缩上线。与 `vault_file_info` 一样是廉价调用（只读文件元数据
/// + 解密少量墓碑帧取 rev），不必 `spawn_blocking`。
#[tauri::command(rename_all = "snake_case")]
pub fn vault_compaction_stats(state: tauri::State<'_, AppState>) -> IpcResult<CompactionStatsDto> {
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    let stats = repo.compaction_stats().map_err(IpcError::from)?;
    Ok(CompactionStatsDto::from(stats))
}

/// 压缩保险库（D-76）：重写容器，只保留活跃帧 + 必要的删除墓碑，然后原子替换。
///
/// - 语义：**不改任何业务数据、不需要主密码**（头部逐字段沿用，只重算两个计数）；
/// - 崩溃安全：同目录 tmp → `sync_all` → 校验 → 原子 rename（见
///   `Container::rewrite_with_frames` 文档）；失败自动从 `.precompact-<ms>` 回滚；
/// - 并发：整个操作在 `state.session` 锁内完成，与同步/其它写操作**结构上互斥**；
///   代价是若有一轮同步正在进行，压缩会排队等它结束（反之亦然）。
///
/// 用 `spawn_blocking` 与 `sync_now` 保持一致：重写 200 MB 级容器时不应该占住 IPC 线程。
#[tauri::command(rename_all = "snake_case")]
pub async fn compact_vault(app: tauri::AppHandle) -> IpcResult<CompactionResultDto> {
    let app_log = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut session = state.session.lock().map_err(lock_err)?;
        let repo = session.repo_mut()?;
        let outcome = repo.compact().map_err(IpcError::from)?;
        Ok(CompactionResultDto::from(outcome))
    })
    .await
    .map_err(|e| IpcError::internal(&format!("spawn_blocking compact_vault: {e}")))?;
    if let Err(err) = &result {
        // 失败必须留痕：压缩是唯一会整体替换库文件的操作，静默失败不可接受。
        append_log(&app_log, &format!("[compact] failed: {err:?}"));
    }
    result
}
