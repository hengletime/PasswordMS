//! 系统类命令：状态、剪贴板、打开网址、文件信息（架构文档 §5.2）。

use passwordms_core::clipboard_policy;
use passwordms_core::session::Session;
use passwordms_core::url_safety;
use tauri::Manager;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;
use zeroize::Zeroizing;

use crate::dto::{CompactionResultDto, CompactionStatsDto, StatusDto, VaultFileInfoDto};
use crate::error::{lock_err, IpcError, IpcResult};
use crate::state::{append_log, vault_path, AppState, ClipboardMemory};

/// 启动判定：是否存在保险库、当前是否锁定。
#[tauri::command(rename_all = "snake_case")]
pub fn get_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> IpcResult<StatusDto> {
    let path = vault_path(&app)?;
    let session = state.lock_session().map_err(lock_err)?;
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
/// 返回本次写入的**代际号**（D-135）：前端倒计时到期时把它带回
/// [`clear_secret_if_own`]，后端据此确认「要清的是不是我这一代写的内容」。
///
/// **`ttl_secs == 0` 表示「不做自动清空」**（非敏感文本：用户名、配对码）。
/// 【D-135】原实现无视这个 0，照样 `sleep(0秒)` 后清理——于是「复制用户名」
/// 表现为：写入成功 → toast 报「已复制」→ 同一瞬间被自己的兜底线程清成空串，
/// 用户粘贴得到空气。这不是竞态，是必现（0 秒到期）。
///
/// 【D-96】Rust 兜底：前端倒计时可能因页面休眠 / WebView 冻结 / 事件丢失而不触发，
/// 这里在写入成功后另起 **detached 线程**等用户配置的 TTL 秒再调
/// [`clear_owned_clipboard_for_gen`]。两者是幂等协作：代际号保证同一次写入的
/// 两个清理者只有一个真正动手（另一个发现记忆值已被取走，自然空转）；用户若已复制
/// 别的内容，比对不相等则保持不动。
#[tauri::command(rename_all = "snake_case")]
pub fn copy_secret(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    text: String,
    ttl_secs: u16,
) -> IpcResult<u64> {
    // D-108（3.4-6）：复制是用户活动（本命令不走会话锁，需显式触点）。
    state.touch_activity();
    // D-106（豆包审查 3.4-5）：兜底清理按用户配置的 TTL 睡眠——原实现丢弃参数
    // 恒等 30s，用户配 60s 时密码在第二次粘贴前被提前清掉（违背配置预期）。
    app.clipboard()
        .write_text(text.clone())
        .map_err(|_| IpcError::internal("clipboard_write"))?;
    let gen = state.next_clipboard_gen();
    {
        let mut guard = state.clipboard.lock().map_err(lock_err)?;
        // 明文进 `Zeroizing` 前先 clone 一份交出去；事件顺序无所谓，`gen` 决定归属。
        *guard = Some(ClipboardMemory {
            gen,
            text: Zeroizing::new(text),
        });
    }
    // D-135：策略由 core 决定（`0` = 不做自动清空），单测钉在
    // `crates/passwordms-core/tests/clipboard_policy.rs`。
    if let Some(secs) = clipboard_policy::auto_clear_delay_secs(ttl_secs) {
        spawn_clipboard_fallback(app, gen, std::time::Duration::from_secs(secs));
    }
    Ok(gen)
}

/// 【D-96】剪贴板兜底清理线程（detached）：等 `delay` 后，**只清自己那一代**
/// （见 [`clear_owned_clipboard_for_gen`]）。不 join、不持锁跨越 sleep；
/// 应用退出时线程随之消亡，残留剪贴板由 [`crate::run`] 的 `RunEvent::Exit` 钩子清理。
/// **D-106**：TTL 改为前端传入的用户配置值（前端已钳制 5–120s；30s 常量仅作默认）。
fn spawn_clipboard_fallback(app: tauri::AppHandle, gen: u64, delay: std::time::Duration) {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if let Some(state) = app.try_state::<AppState>() {
            let _ = clear_owned_clipboard_for_gen(&app, state.inner(), Some(gen));
        }
    });
}

/// A-03 核心逻辑：**仅当**剪贴板内容仍为本应用写入的内容时才清空；并在任何分支下
/// 都消费掉记忆值（`take` 语义，避免旧值长期驻留）。
///
/// 抽成公开函数以便锁定 / 退出路径复用（安全需求 §3.3「锁定兼容」）。实现放在 Rust 侧，
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
///
/// 无代际过滤版：锁定 / 退出时「本应用写过的东西一律收走」，这正是我们要的。
pub fn clear_owned_clipboard(app: &tauri::AppHandle, state: &AppState) -> IpcResult<bool> {
    clear_owned_clipboard_for_gen(app, state, None)
}

/// 带**代际过滤**的清理（D-135）：`only_gen == Some(g)` 时，只有当前记忆值确实是
/// 第 `g` 代写入的才动手；否则直接放手。
///
/// 为什么不能「先 take 再判号」：`take` 会把**新**一代的记忆值偷走，之后新内容到期时
/// 后端已无据可比，等于永久放弃清理（安全缺陷）。所以判定必须在锁内、`take` 之前。
pub fn clear_owned_clipboard_for_gen(
    app: &tauri::AppHandle,
    state: &AppState,
    only_gen: Option<u64>,
) -> IpcResult<bool> {
    let remembered = {
        let mut guard = state.clipboard.lock().map_err(lock_err)?;
        match guard.as_ref() {
            None => return Ok(false),
            // 号不对 = 这是别人（更新的一代）写的，我们无权清理，也不许动记忆值。
            Some(m) => {
                if only_gen.is_some_and(|g| g != m.gen) {
                    return Ok(false);
                }
            }
        }
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
    if current == own.text.as_str() {
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
/// 薄壳：实际逻辑见 [`clear_owned_clipboard_for_gen`]（锁定路径亦复用同一逻辑）。
/// 【D-135】`gen` 必填：前端倒计时只能清自己那一代，否则「复制密码 A → 中途复制
/// 用户名 B → A 的定时器到期」会把 B 清掉（用户看到的就是「用户名又空了」）。
#[tauri::command(rename_all = "snake_case")]
pub fn clear_secret_if_own(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    gen: u64,
) -> IpcResult<bool> {
    clear_owned_clipboard_for_gen(&app, state.inner(), Some(gen))
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
///
/// D-108（豆包审查 3.4-8）：只回传**文件名**——绝对路径对用户无增量价值
/// （路径结构由应用决定、不可配置），却把本机目录结构暴露给前端 JS。
#[tauri::command(rename_all = "snake_case")]
pub fn vault_file_info(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> IpcResult<VaultFileInfoDto> {
    let path = vault_path(&app)?;
    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    let size_bytes = repo.file_size().map_err(IpcError::from)?;
    Ok(VaultFileInfoDto {
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "passwordms.vault".to_string()),
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
    let session = state.lock_session().map_err(lock_err)?;
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
        let mut session = state.lock_session().map_err(lock_err)?;
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
