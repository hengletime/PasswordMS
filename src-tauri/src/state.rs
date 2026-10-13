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

/// 保险库文件路径（应用数据目录 / `passwordms.vault`）。
pub fn vault_path(app: &tauri::AppHandle) -> IpcResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| IpcError::io("app_data_dir"))?;
    std::fs::create_dir_all(&dir).map_err(|_| IpcError::io("create_app_data_dir"))?;
    Ok(dir.join("passwordms.vault"))
}

/// Host 身份密钥文件路径。
pub fn identity_path(app: &tauri::AppHandle) -> IpcResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| IpcError::io("app_data_dir"))?;
    std::fs::create_dir_all(&dir).map_err(|_| IpcError::io("create_app_data_dir"))?;
    Ok(dir.join("passwordms.identity"))
}

/// 启动（或复用）持久重同步监听器，返回其监听端口。
///
/// 该监听器用于「已配对设备」的直连重同步（D-11）：对端在配对时记录本端口，
/// 之后调用 `sync_now` 即可直连本机完成增量同步。
pub fn ensure_resync_listener(app: &tauri::AppHandle) -> IpcResult<u16> {
    let state = app.state::<AppState>();
    {
        let guard = state.resync.lock().map_err(lock_err)?;
        if let Some(handle) = guard.as_ref() {
            return Ok(handle.server.port());
        }
    }

    // 绑定临时端口（0）由系统分配，随后持久通告给对端。
    let server = ResyncServer::start(0, "PasswordMS".to_string())?;
    let port = server.port();
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
        let mut guard = match state.resync.lock() {
            Ok(g) => g,
            Err(_) => break,
        };
        let handle = match guard.as_mut() {
            Some(h) => h,
            None => break,
        };
        let mut session = match state.session.lock() {
            Ok(s) => s,
            Err(_) => break,
        };
        // 会话若已锁定则本轮跳过（等待解锁或停止信号）。
        match session.repo_mut() {
            Ok(repo) => {
                let _ = handle
                    .server
                    .poll_once(repo, passwordms_core::model::now_ms());
            }
            Err(_) => continue,
        }
    });
}
