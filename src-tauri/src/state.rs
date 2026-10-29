//! 托管状态（架构文档 §4.5）。
//!
//! - `session`：`Mutex<Session>`（锁定态 / 解锁态 + 仓储）
//! - `pairing`：当前 Host 配对会话（一次性令牌）
//! - `resync`：持久重同步监听器（修复 D-11；解锁期间运行）
//! - `pending_imports`：解析后的导入预览缓存（`preview_token` 索引）
//! - `clipboard`：本应用最后一次写入剪贴板的**明文副本**（A-03 到期比对用；零化载体，
//!   锁定时清空）

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use passwordms_core::import::ImportPreview;
use passwordms_core::model::now_ms;
use passwordms_core::paths::portable_data_dir;
use passwordms_core::session::Session;
use passwordms_core::sync::resync::ResyncServer;
use passwordms_core::sync::server::SyncServer;
use tauri::Manager;
use zeroize::Zeroizing;

use crate::error::{lock_err, IpcError, IpcResult};

/// 持久重同步监听器句柄。
///
/// **D-106（豆包审查 3.4-2）**：停止信号提升为 [`AppState::resync_stop`]——
/// 停止不再依赖「能拿到 resync 锁 + handle 还在」，且被动同步整场网络 I/O
/// 可以在锁外进行（worker 锁内 `take`、锁外 `handle_connection`）。
pub struct ResyncHandle {
    pub server: ResyncServer,
}

/// 应用全局状态。
pub struct AppState {
    pub session: Mutex<Session>,
    pub pairing: Mutex<Option<SyncServer>>,
    pub resync: Mutex<Option<ResyncHandle>>,
    /// 重同步 worker 的停止信号（提升到状态层，见 [`ResyncHandle`] 文档）。
    pub resync_stop: Arc<AtomicBool>,
    pub pending_imports: Mutex<HashMap<String, ImportPreview>>,
    /// 本应用最后一次写入剪贴板的内容（A-03 到期比对用）。
    ///
    /// 以 `Zeroizing` 托管，替换 / 清空时自动覆写旧值；锁定时一并清空。
    pub clipboard: Mutex<Option<Zeroizing<String>>>,
    /// O2：同一时刻仅允许一场同步（被动 worker 与主动 `sync_now` 互斥）。
    ///
    /// 三段式去持锁化后，被动与主动交换可并发——两者会各自加载/推进同一份
    /// `DeviceSyncState`（seq/base/水位），产生序号竞争与重复导出。用本标志
    /// 串行化：compare_exchange 抢不到的一侧快速失败（`ERR_SYNC_BUSY`）。
    pub sync_busy: AtomicBool,
    /// **生物识别命令互斥**（`docs/08` §18.5 约束 8）。
    ///
    /// 生物识别命令会弹系统对话框（可能停留数十秒），期间若同一入口被连点两次，第二次
    /// 会拿到上一次的封印状态、或在「解封成功但会话尚未装入」的窗口里重入。用 CAS 串行化：
    /// 抢不到的一侧直接失败，前端静默忽略即可（用户本来就在等前一次）。
    pub biometric_busy: AtomicBool,
    /// **D-108（豆包审查 3.4-6）**：最近一次用户活动的 Unix 毫秒时间戳。
    ///
    /// auto-lock 此前完全由前端 JS 定时器驱动——WebView 后台/休眠时节流暂停计时，
    /// 保险库保持解锁态。Rust 兜底线程（[`spawn_auto_lock_worker`]）以本时间戳为
    /// 闲置基准：任何触碰会话的 IPC 经 [`AppState::lock_session`] 刷新它，
    /// 不走会话锁的用户命令（复制 / 生成器 / TOTP 等）显式调 [`AppState::touch_activity`]。
    /// **worker 自身的轮询不刷新它**（否则永不闲置）。
    pub last_activity_ms: AtomicI64,
    /// 应用级配置（v1.4.10）：热键键位 / 开机自启 / 托盘提示已读。
    ///
    /// **不放进保险库**：锁屏态和冷启动时就要用它（详见 `prefs.rs` 模块文档）。
    /// 启动时由 `lib.rs` 从磁盘载入并覆盖这里的默认值。
    pub prefs: Mutex<crate::prefs::AppPrefs>,
}

impl AppState {
    /// 新建（会话默认锁定）。
    pub fn new() -> Self {
        Self {
            session: Mutex::new(Session::locked()),
            pairing: Mutex::new(None),
            resync: Mutex::new(None),
            resync_stop: Arc::new(AtomicBool::new(false)),
            pending_imports: Mutex::new(HashMap::new()),
            clipboard: Mutex::new(None),
            sync_busy: AtomicBool::new(false),
            biometric_busy: AtomicBool::new(false),
            last_activity_ms: AtomicI64::new(now_ms()),
            prefs: Mutex::new(crate::prefs::AppPrefs::default()),
        }
    }

    /// 取配置副本（锁中毒时回落默认值——配置读不到不该让调用方失败）。
    pub fn prefs_snapshot(&self) -> crate::prefs::AppPrefs {
        self.prefs.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// 记录一次用户活动（auto-lock 闲置基准，D-108 3.4-6）。
    pub fn touch_activity(&self) {
        self.last_activity_ms.store(now_ms(), Ordering::SeqCst);
    }

    /// **触碰会话锁**：刷新用户活动时间戳后取锁。
    ///
    /// D-108（3.4-6）：全部「触碰保险库会话」的 IPC 命令统一改走本方法，
    /// auto-lock 兜底线程据此判断闲置。返回值语义与 `session.lock()` 完全一致，
    /// 锁中毒仍由调用点 `map_err(lock_err)?` 处理。
    pub fn lock_session(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, Session>> {
        self.touch_activity();
        self.session.lock()
    }

    /// 抢占生物识别互斥位；返回 `None` 表示已有一次生物识别操作在进行中。
    ///
    /// 守卫在 `Drop` 时自动释放——包括「Hello 弹框中用户直接关掉应用」这类提前返回路径，
    /// 不需要每个分支手写复位。
    pub fn try_begin_biometric(&self) -> Option<BiometricGuard<'_>> {
        self.biometric_busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| BiometricGuard {
                flag: &self.biometric_busy,
            })
    }
}

/// [`AppState::try_begin_biometric`] 的 RAII 守卫。
pub struct BiometricGuard<'a> {
    flag: &'a AtomicBool,
}

impl Drop for BiometricGuard<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// 数据目录（便携模式优先）：exe 同目录存在 `passwordms.portable` 标记时，
/// 全部数据文件随 exe 走（绿色版/便携发布渠道）；否则使用系统应用数据目录
/// （Windows: `%APPDATA%\com.passwordms.app`）。
pub fn data_dir(app: &tauri::AppHandle) -> IpcResult<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    if let Some(dir) = portable_data_dir(exe_dir.as_deref()) {
        // 便携目录即 exe 所在目录，必定存在。
        return Ok(dir);
    }
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| IpcError::io("app_data_dir"))?;
    std::fs::create_dir_all(&dir).map_err(|_| IpcError::io("create_app_data_dir"))?;
    Ok(dir)
}

/// 保险库文件路径（数据目录 / `passwordms.vault`）。
pub fn vault_path(app: &tauri::AppHandle) -> IpcResult<PathBuf> {
    Ok(data_dir(app)?.join("passwordms.vault"))
}

/// 应用级配置路径（数据目录 / `prefs.json`，v1.4.10）。
///
/// 刻意用 [`data_dir`] 而不是 `app_config_dir`：便携版（exe 旁有
/// `passwordms.portable`）的语义是「换台机器拷走目录就能继续用」，
/// 配置留在系统盘会让热键/自启设置莫名其妙地"丢失"。
pub fn prefs_path(app: &tauri::AppHandle) -> IpcResult<PathBuf> {
    Ok(data_dir(app)?.join("prefs.json"))
}

/// 轻量文件日志（数据目录 / `passwordms.log`）。
///
/// 为什么落盘：Windows GUI 程序没有 stderr，`eprintln!` 直接被丢弃——
/// 配对失败这类疑难问题只有文件日志才能事后取证（D-43）。
/// 内容仅限错误码/错误消息（`Debug` 已脱敏，无任何密钥或明文）；超过 1 MB 自动重建。
pub fn append_log(app: &tauri::AppHandle, msg: &str) {
    use std::io::Write;
    let Ok(dir) = data_dir(app) else { return };
    let path = dir.join("passwordms.log");
    let ts = now_ms();
    let line = format!("[{ts}] {msg}\n");
    // 超 1MB：截断重建（保留最后可见问题，不追求日志轮转完备性）。
    let need_truncate = std::fs::metadata(&path)
        .map(|m| m.len() > 1_000_000)
        .unwrap_or(false);
    let open = if need_truncate {
        std::fs::File::create(&path)
    } else {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
    };
    if let Ok(mut f) = open {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Host 身份密钥文件路径。
pub fn identity_path(app: &tauri::AppHandle) -> IpcResult<PathBuf> {
    Ok(data_dir(app)?.join("passwordms.identity"))
}

/// 启动（或复用）持久重同步监听器，返回其监听端口。
///
/// 该监听器用于「已配对设备」的直连重同步（D-11）：对端在配对时记录本端口，
/// 之后调用 `sync_now` 即可直连本机完成增量同步。
///
/// **D-48 端口稳定性**：此前 `bind(0)` 每次启动随机分配端口——本机一旦重启
/// （如升级重装），对端持久化的地址即失效，`sync_now` 必败且只能重新配对。
/// 现在把选定的端口写入数据目录 `resync.port`，后续启动优先绑定同一端口；
/// 被占用时回退随机端口并更新文件（此时对端需重新配对，罕见路径留日志）。
pub fn ensure_resync_listener(app: &tauri::AppHandle) -> IpcResult<u16> {
    let state = app.state::<AppState>();
    {
        let guard = state.resync.lock().map_err(lock_err)?;
        if let Some(handle) = guard.as_ref() {
            return Ok(handle.server.port());
        }
    }

    let port_file = data_dir(app)?.join("resync.port");
    let mut fell_back = false;
    let server = match std::fs::read_to_string(&port_file)
        .ok()
        .and_then(|t| t.trim().parse::<u16>().ok())
        .filter(|p| *p != 0)
        .and_then(|saved| ResyncServer::start(saved, "PasswordMS".to_string()).ok())
    {
        Some(s) => s,
        None => {
            let s = ResyncServer::start(0, "PasswordMS".to_string())?;
            fell_back = true;
            s
        }
    };
    let port = server.port();
    if !server.discovery_enabled() {
        // D-74：UDP 发现口 bind 失败只降级（不阻断解锁）。留痕以便真机排查
        // "为什么换网后自动发现没生效"。
        append_log(app, "[discovery] udp bind failed, auto-discovery disabled");
    }

    let stop = state.resync_stop.clone();
    // D-106：stop 提升到状态层——上一轮 lock() 已把它置 true，重启监听前必须复位。
    stop.store(false, Ordering::SeqCst);
    {
        let mut guard = state.resync.lock().map_err(lock_err)?;
        // 并发启动竞态：保留先到者。
        if let Some(handle) = guard.as_ref() {
            return Ok(handle.server.port());
        }
        *guard = Some(ResyncHandle { server });
    }
    // D-108（豆包审查 3.4-10）：端口文件**在本 server 确认被采纳后**才落盘——
    // 原实现先写文件再进第二锁段，竞态败者会把自己的（已丢弃）端口写进文件，
    // 下一轮启动优先绑一个没人监听的端口。
    if fell_back {
        let _ = std::fs::write(&port_file, port.to_string());
        append_log(
            app,
            &format!("[resync] saved port unavailable, rebound to {port} (peers must re-pair)"),
        );
    }
    spawn_resync_worker(app.clone(), stop);
    Ok(port)
}

/// 停止重同步监听（锁定 / 重新解锁时调用）。
///
/// **D-106（豆包审查 3.4-3）**：先**无条件**置停止信号（不再依赖能否拿到锁），
/// 再清理 handle——锁中毒时 `into_inner()` 兜底照常清空并留痕。原实现中毒即
/// 静默返回，worker 会带着 socket 继续接受连接（虽然业务被 NotUnlocked 拒绝）。
pub fn stop_resync_listener(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    state.resync_stop.store(true, Ordering::SeqCst);
    // match 语句以分号结尾，让 lock() 的临时值在函数尾之前 drop（borrow checker 要求）。
    match state.resync.lock() {
        Ok(mut guard) => *guard = None,
        Err(poisoned) => {
            let mut guard = poisoned.into_inner();
            *guard = None;
            append_log(
                app,
                "[resync] stop: resync lock poisoned, cleared via into_inner",
            );
        }
    };
}

/// 后台轮询线程：非阻塞接受入站重同步连接，直到收到停止信号。
///
/// **O2 三段式**：`poll_accept` 与 UDP 发现应答**不持 session 锁**；连接建立后
/// 由 [`SessionAccess`] 在「读写库的瞬间」短暂取锁，网络 I/O 全程锁外——
/// 同步期间 UI 的一切 IPC（列表/复制/自动锁定）不再排队等待。
/// **D-106（豆包审查 3.4-2）**：resync 锁同样不再跨网络 I/O——锁内 `take` handle、
/// 锁外整场 `handle_connection`，结束后放回（停止信号在状态层，见 [`AppState`]）。
fn spawn_resync_worker(app: tauri::AppHandle, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let state = app.state::<AppState>();

        // 锁顺序恒为 resync → session，避免与命令层形成环。
        let accepted = {
            let mut guard = match state.resync.lock() {
                Ok(g) => g,
                // D-91（审查 C5）：锁中毒不再静默退出——此前本机会从此不接受
                // 被动同步直至重启且无线索可查。
                Err(_) => {
                    append_log(&app, "[resync] resync lock poisoned, worker exiting");
                    break;
                }
            };
            let handle = match guard.as_mut() {
                Some(h) => h,
                None => break,
            };
            let accepted = match handle.server.poll_accept() {
                Ok(a) => a,
                Err(e) => {
                    append_log(&app, &format!("[resync] accept failed: {e:?}"));
                    continue;
                }
            };
            // D-74：UDP 发现应答与入站重同步共用同一 200ms 节奏（只读、毫秒级锁内段）。
            // **返回值不参与任何 match**：发现只是增强路径，它出错绝不能让
            // 重同步轮询循环 `break`（那会让整台设备再也不接受同步）。
            if accepted.is_none() {
                if let Ok(session) = state.session.lock() {
                    if let Ok(repo) = session.repo() {
                        let _ = handle.server.poll_discovery_once(repo);
                    }
                }
            }
            accepted
        };
        let Some((stream, peer_ip)) = accepted else {
            continue;
        };

        // O2：同一时刻仅一场同步（与主动 sync_now 互斥，防 seq/base 竞态）。
        if state
            .sync_busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            append_log(&app, "[resync] skipped: another sync is in flight");
            continue;
        }
        let polled = {
            let mut guard = match state.resync.lock() {
                Ok(g) => g,
                Err(_) => {
                    state.sync_busy.store(false, Ordering::SeqCst);
                    append_log(&app, "[resync] resync lock poisoned, worker exiting");
                    break;
                }
            };
            // D-106（豆包审查 3.4-2）：`take` 出 handle 后**立即释放 resync 锁**——
            // `handle_connection` 是整场被动同步网络 I/O（毫秒到秒级），原实现持锁
            // 跨越它，锁定 / `ensure_resync_listener` / `sync_now` 在此期间全部阻塞。
            match guard.take() {
                Some(mut handle) => {
                    drop(guard);
                    let mut access = SessionAccess {
                        session: &state.session,
                    };
                    let result =
                        handle
                            .server
                            .handle_connection(stream, &peer_ip, now_ms(), &mut access);
                    // 放回 handle：仅当期间没有被 stop 清掉（stop 置位则 worker 即将
                    // 退出，drop 即关闭监听）。放不回（竞态窗口内 handle 位被占）也
                    // 直接 drop——旧监听关闭，port 文件语义不变，下轮 ensure 重建。
                    if !stop.load(Ordering::SeqCst) {
                        if let Ok(mut g) = state.resync.lock() {
                            if g.is_none() {
                                *g = Some(handle);
                            }
                        }
                    }
                    result
                }
                None => {
                    // handle 已被 stop_resync_listener 清掉：复位互斥位后退出。
                    state.sync_busy.store(false, Ordering::SeqCst);
                    break;
                }
            }
        };
        state.sync_busy.store(false, Ordering::SeqCst);

        match polled {
            Ok(outcome) => {
                // D-48：错误必须留痕——此前 `let _ =` 把断连原因全部吞掉，
                // 对端只看到笼统的「文件读写错误」，PC 侧却无线索可查。
                append_log(
                    &app,
                    &format!(
                        "[resync] completed: applied={} blobs={} conflicts={}",
                        outcome.applied,
                        outcome.blobs,
                        outcome.conflicts.len()
                    ),
                );
                // v1.2.4 根因 A：**被动同步命中后必须通知前端**。
                // 对端（手机）发起的重同步会把变更写进本机库，但本机前端 store 仍持有
                // 同步前的条目列表 → 主人实测「手机删了附件、电脑上附件还在」。
                // 事件载荷只带计数（不含任何明文/ID），前端据此决定是否 reload。
                let _ = tauri::Emitter::emit(
                    &app,
                    "sync://applied",
                    serde_json::json!({
                        "applied": outcome.applied,
                        "blobs": outcome.blobs,
                        "conflicts": outcome.conflicts.len(),
                    }),
                );
            }
            Err(e) => {
                append_log(&app, &format!("[resync] aborted: {e:?}"));
            }
        }
    });
}

/// O2：三段式同步的会话访问器（被动 worker / 主动 `sync_now` / 配对交换共用）——
/// 只在锁内段短暂持有 session 锁，网络 I/O 全程锁外。
pub struct SessionAccess<'a> {
    pub session: &'a Mutex<Session>,
}

impl passwordms_core::sync::access::RepoAccess for SessionAccess<'_> {
    fn with_repo(
        &mut self,
        job: passwordms_core::sync::access::RepoJob<'_>,
    ) -> passwordms_core::error::CoreResult<()> {
        let mut session = self
            .session
            .lock()
            // 锁中毒：放弃本场交换（宁多勿丢；对端下轮重发）。
            .map_err(|_| passwordms_core::error::CoreError::SyncAborted)?;
        // 会话已锁定（如自动锁定在传输中途触发）→ 放弃本轮：密文暂存随连接丢弃，
        // 对端下轮重发（宁多勿丢）。
        let repo = session.repo_mut()?;
        job(repo)
    }
}

/// **D-108（豆包审查 3.4-6）**：auto-lock 的 Rust 兜底线程。
///
/// 此前自动锁定完全由前端 JS 定时器驱动——WebView 进入后台 / 休眠 / 被节流时
/// 计时暂停，保险库保持解锁态（安全模型里唯一只有前端守的环节）。本 worker 每
/// 15 秒醒来一次：会话解锁中且「距 [`AppState::last_activity_ms`] 已达档位」即执行
/// 与 `lock` 命令**同一套**锁定级联（[`crate::commands::vault_cmd::perform_lock`]），
/// 并 emit `autolock://locked` 让前端复位到锁屏。
///
/// 纪律：
/// - 判定段的 session 锁只读 `is_unlocked` + 档位，毫秒级；锁定级联**全程锁外**
///   （`perform_lock` 内部自取锁），与锁序约定（resync → session）无冲突；
/// - worker 轮询**不得**刷新 `last_activity_ms`（不走 `lock_session`）；
/// - 15s 轮询粒度 → 实际锁定时点最多滞后档位 15 秒（偏安全侧，可接受）。
pub fn spawn_auto_lock_worker(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(15));
        let Some(state) = app.try_state::<AppState>() else {
            break; // 应用退出中
        };
        // 判定段：未解锁直接跳过；解锁则顺带读档位（D-61：头部明文字段）。
        let (unlocked, minutes) = match state.session.lock() {
            Ok(s) => (
                s.is_unlocked(),
                s.repo().map(|r| r.auto_lock_minutes()).unwrap_or(0),
            ),
            Err(_) => continue, // 锁中毒：本轮放弃（业务路径会留痕）
        };
        if !unlocked || minutes == 0 {
            continue;
        }
        let idle_ms = now_ms() - state.last_activity_ms.load(Ordering::SeqCst);
        if idle_ms < i64::from(minutes) * 60_000 {
            continue;
        }
        // 锁外执行完整锁定级联 + 通知前端。
        if crate::commands::vault_cmd::perform_lock(&app).is_ok() {
            append_log(
                &app,
                &format!("[autolock] idle {idle_ms}ms >= {minutes}min, locked by backend fallback"),
            );
            let _ = tauri::Emitter::emit(&app, "autolock://locked", "idle");
        }
    });
}
