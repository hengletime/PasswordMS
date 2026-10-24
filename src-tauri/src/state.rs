//! 托管状态（架构文档 §4.5）。
//!
//! - `session`：`Mutex<Session>`（锁定态 / 解锁态 + 仓储）
//! - `pairing`：当前 Host 配对会话（一次性令牌）
//! - `resync`：持久重同步监听器（修复 D-11；解锁期间运行）
//! - `pending_imports`：解析后的导入预览缓存（`preview_token` 索引）
//! - `host_identity`：Host 长期身份密钥对
//! - `clipboard`：本应用最后一次写入剪贴板的**明文副本**（A-03 到期比对用；零化载体，
//!   锁定时清空）

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use passwordms_core::import::ImportPreview;
use passwordms_core::model::now_ms;
use passwordms_core::paths::portable_data_dir;
use passwordms_core::session::Session;
use passwordms_core::sync::handshake::HostIdentity;
use passwordms_core::sync::resync::ResyncServer;
use passwordms_core::sync::server::SyncServer;
use tauri::Manager;
use zeroize::Zeroizing;

use crate::error::{lock_err, IpcError, IpcResult};

/// 持久重同步监听器句柄（含停止信号）。
pub struct ResyncHandle {
    pub server: ResyncServer,
    pub stop: Arc<AtomicBool>,
}

/// 应用全局状态。
pub struct AppState {
    pub session: Mutex<Session>,
    pub pairing: Mutex<Option<SyncServer>>,
    pub resync: Mutex<Option<ResyncHandle>>,
    pub pending_imports: Mutex<HashMap<String, ImportPreview>>,
    pub host_identity: Mutex<Option<HostIdentity>>,
    /// 本应用最后一次写入剪贴板的内容（A-03 到期比对用）。
    ///
    /// 以 `Zeroizing` 托管，替换 / 清空时自动覆写旧值；锁定时一并清空。
    pub clipboard: Mutex<Option<Zeroizing<String>>>,
}

impl AppState {
    /// 新建（会话默认锁定）。
    pub fn new() -> Self {
        Self {
            session: Mutex::new(Session::locked()),
            pairing: Mutex::new(None),
            resync: Mutex::new(None),
            pending_imports: Mutex::new(HashMap::new()),
            host_identity: Mutex::new(None),
            clipboard: Mutex::new(None),
        }
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
    if fell_back {
        let _ = std::fs::write(&port_file, port.to_string());
        append_log(
            app,
            &format!("[resync] saved port unavailable, rebound to {port} (peers must re-pair)"),
        );
    }

    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut guard = state.resync.lock().map_err(lock_err)?;
        // 并发启动竞态：保留先到者。
        if let Some(handle) = guard.as_ref() {
            return Ok(handle.server.port());
        }
        *guard = Some(ResyncHandle {
            server,
            stop: stop.clone(),
        });
    }
    spawn_resync_worker(app.clone(), stop);
    Ok(port)
}

/// 停止重同步监听（锁定 / 重新解锁时调用）。
pub fn stop_resync_listener(app: &tauri::AppHandle) {
    if let Ok(mut guard) = app.state::<AppState>().resync.lock() {
        if let Some(handle) = guard.as_ref() {
            handle.stop.store(true, Ordering::SeqCst);
        }
        *guard = None;
    }
}

/// 后台轮询线程：非阻塞接受入站重同步连接，直到收到停止信号。
fn spawn_resync_worker(app: tauri::AppHandle, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let state = app.state::<AppState>();

        // 锁顺序恒为 resync → session，避免与命令层形成环。
        //
        // v1.2.4（根因 A）：轮询调用**与结果处理分处两个作用域** —— 出作用域后再 emit。
        // 前端收到 `sync://applied` 会立刻回打 `items_list`，那条 IPC 要拿 session 锁；
        // 若在持锁状态下 emit，它会白等一个轮询周期（最坏 200ms + 本次传输耗时）。
        let polled = {
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
            let mut session = match state.session.lock() {
                Ok(s) => s,
                // D-91（审查 C5）：同上，中毒留痕。
                Err(_) => {
                    append_log(&app, "[resync] session lock poisoned, worker exiting");
                    break;
                }
            };
            // 会话若已锁定则本轮跳过（等待解锁或停止信号）。
            match session.repo_mut() {
                Ok(repo) => {
                    // D-74：UDP 发现应答与入站重同步共用同一 200ms 节奏。
                    // **返回值不参与下面的 match**：发现只是增强路径，它出错绝不能让
                    // 重同步轮询循环 `break`（那会让整台设备再也不接受同步）。
                    let _ = handle.server.poll_discovery_once(repo);
                    handle.server.poll_once(repo, now_ms())
                }
                Err(_) => continue,
            }
        };

        match polled {
            Ok(Some(outcome)) => {
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
            Ok(None) => {}
            Err(e) => {
                append_log(&app, &format!("[resync] aborted: {e:?}"));
            }
        }
    });
}
