//! 同步与配对命令（架构文档 §5.2、F-09、§3）。
//!
//! **配对**：Host 以非阻塞轮询线程等待 Joiner 连接；配对时双方互相通告各自的**持久重同步
//! 监听端口**（随二维码 / HELLO 传递），并写入 `PairedDevice`。
//! **直连重同步**：`sync_now` 读取已配对设备地址 → 建立连接 → 复用 sync 层握手（会话密钥
//! 派生 + 双向确认）→ 拉取/推送变更 → 合并（冲突入冲突箱）→ 更新游标（修复 D-11）。

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use tauri::Manager;

use passwordms_core::crypto::rng;
use passwordms_core::model::conflict::ConflictChoice;
use passwordms_core::model::now_ms;
use passwordms_core::sync::client;
use passwordms_core::sync::handshake::HostIdentity;
use passwordms_core::sync::protocol::{qr_svg, QRPayload};
use passwordms_core::sync::resync;
use passwordms_core::sync::server::SyncServer;
use passwordms_core::constants::{SYNC_PORT_MAX, SYNC_PORT_MIN};

use crate::dto::{ConflictDto, DeviceDto, PairResultDto, PairingDto, SyncResultDto, SyncStatusDto};
use crate::error::{lock_err, IpcError, IpcResult};
use crate::state::{ensure_resync_listener, identity_path, AppState};

/// F-09 Host：生成一次性令牌并展示二维码。
#[tauri::command(rename_all = "snake_case")]
pub fn sync_start_pairing(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> IpcResult<PairingDto> {
    // 必须处于解锁态。
    let host_id = {
        let session = state.session.lock().map_err(lock_err)?;
        session.repo()?.device_id().to_string()
    };

    // 确保持久重同步监听器在线，并把其端口通告给 Joiner（D-11）。
    let sync_port = ensure_resync_listener(&app)?;

    let identity = load_or_create_identity(&app)?;
    // 若已有配对会话，先作废。
    *state.pairing.lock().map_err(lock_err)? = None;

    let host_name = host_display_name(&host_id);
    let port = random_port();
    let server = SyncServer::start(port, host_id, host_name, identity, sync_port, now_ms())?;
    // D-44：二维码携带全部候选局域网 IPv4（多网卡机器上主选地址可能手机路由不到）。
    let ips = passwordms_core::sync::transport::local_ipv4s();
    let qr = server.qr_payload(ips.clone())?;
    let qr_text = qr.encode_string()?;
    let qr_markup = qr_svg(&qr_text)?;
    let dto = PairingDto {
        qr_svg: qr_markup,
        qr_text,
        ip: qr.ip.clone(),
        ips,
        port: qr.port,
        expires_at: qr.exp,
    };
    *state.pairing.lock().map_err(lock_err)? = Some(server);

    spawn_pairing_worker(app.clone());
    Ok(dto)
}

/// F-09 取消配对并作废令牌。
#[tauri::command(rename_all = "snake_case")]
pub fn sync_cancel_pairing(state: tauri::State<'_, AppState>) -> IpcResult<()> {
    let mut guard = state.pairing.lock().map_err(lock_err)?;
    if let Some(server) = guard.as_mut() {
        server.cancel();
    }
    *guard = None;
    Ok(())
}

/// F-09 Joiner：扫码/粘贴二维码文本后发起配对并执行首次同步。
#[tauri::command(rename_all = "snake_case")]
pub fn sync_join(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    qr_payload: String,
) -> IpcResult<PairResultDto> {
    let result = sync_join_impl(&app, &state, &qr_payload);
    if let Err(e) = &result {
        // 诊断留痕（数据目录 passwordms.log + stderr）：IpcError 的 Debug 只有 code + message_key，无隐私载荷。
        eprintln!("[sync_join] failed: {e:?}");
        crate::state::append_log(&app, &format!("[sync_join] failed: {e:?}"));
    }
    result
}

fn sync_join_impl(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, AppState>,
    qr_payload: &str,
) -> IpcResult<PairResultDto> {
    let qr = QRPayload::decode_string(qr_payload)?;
    let now = now_ms();

    // 确保持久重同步监听器在线，并把本机端口随 HELLO 通告给 Host（D-11）。
    let sync_port = ensure_resync_listener(app)?;

    let mut session = state.session.lock().map_err(lock_err)?;
    let device_id = session.repo()?.device_id().to_string();
    let device_name = host_display_name(&device_id);

    // 1) 握手（不接触本地数据）。
    let mut joiner = client::pair(&qr, &device_id, &device_name, sync_port, now)?;

    // 2) 采用 Host 的 VK（U1/U2 裁定：本地库非空则拒绝，由前端处理提示）。
    //    D-47：**同组重配对豁免**——本机已与该 Host 配对 = 本地 VK 已在首次配对时采纳
    //    （与对端一致）。此时重复扫码（重装、重复操作、想手动触发反向同步）不应再被
    //    「非空即拒」拦下，直接复用既有密钥完成本次交换；真正的异组保险库仍被拒绝。
    let already_paired = session
        .repo()?
        .list_paired_devices()
        .iter()
        .any(|d| d.device_id == joiner.host_id().to_string());
    if !already_paired {
        session.adopt_vault_key(joiner.host_vault_key())?;
    }

    // 3) 持久化配对关系（含对端地址）并执行双向同步交换。
    let repo = session.repo_mut()?;
    joiner.persist_host_device(repo)?;
    let result = joiner.push_and_sync(repo, now)?;

    Ok(PairResultDto {
        device_id: joiner.host_id().to_string(),
        device_name: joiner.host_name().to_string(),
        fingerprint: joiner.host_fingerprint().to_string(),
        applied: result.applied,
        conflicts: result.conflicts,
    })
}

/// F-09 Host：确认配对（本实现的握手在轮询线程内自动完成，此处为契约占位）。
#[tauri::command(rename_all = "snake_case")]
pub fn sync_accept_pair(_session_id: String) -> IpcResult<()> {
    Ok(())
}

/// F-09 列出已配对设备。
#[tauri::command(rename_all = "snake_case")]
pub fn list_paired_devices(state: tauri::State<'_, AppState>) -> IpcResult<Vec<DeviceDto>> {
    let session = state.session.lock().map_err(lock_err)?;
    Ok(session
        .repo()?
        .list_paired_devices()
        .into_iter()
        .map(DeviceDto::from)
        .collect())
}

/// O-07 解除配对（废弃共享密钥）。
#[tauri::command(rename_all = "snake_case")]
pub fn unpair(state: tauri::State<'_, AppState>, device_id: String) -> IpcResult<()> {
    let mut session = state.session.lock().map_err(lock_err)?;
    session.repo_mut()?.remove_paired_device(&device_id)?;
    Ok(())
}

/// F-09 手动同步：直连已配对设备执行一次增量重同步（修复 D-11）。
///
/// 失败路径映射到既定错误码：未配对 → `ERR_NOT_PAIRED`；密钥确认失败 → `ERR_FINGERPRINT_MISMATCH`；
/// 序号/时间窗异常 → `ERR_REPLAY_DETECTED`；连接/协议中断 → `ERR_SYNC_ABORTED`。
#[tauri::command(rename_all = "snake_case")]
pub fn sync_now(state: tauri::State<'_, AppState>, device_id: String) -> IpcResult<SyncResultDto> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    let outcome = resync::resync_with_peer(repo, &device_id, now_ms())?;
    Ok(SyncResultDto {
        applied: outcome.applied,
        conflicts: outcome.conflicts,
    })
}

/// F-09 同步状态查询。
#[tauri::command(rename_all = "snake_case")]
pub fn sync_status(state: tauri::State<'_, AppState>) -> IpcResult<SyncStatusDto> {
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    let devices = repo.list_paired_devices();
    let last_sync_at = devices.iter().filter_map(|d| d.last_sync_at).max();
    Ok(SyncStatusDto {
        last_sync_at,
        pending_changes: repo.export_changes().len(),
        device_count: devices.len(),
    })
}

/// 列出未仲裁冲突。
#[tauri::command(rename_all = "snake_case")]
pub fn list_conflicts(state: tauri::State<'_, AppState>) -> IpcResult<Vec<ConflictDto>> {
    let session = state.session.lock().map_err(lock_err)?;
    Ok(session
        .repo()?
        .list_conflicts()
        .into_iter()
        .map(ConflictDto::from)
        .collect())
}

/// 三选一仲裁（`keep_local` / `take_remote` / `keep_both`）。
#[tauri::command(rename_all = "snake_case")]
pub fn resolve_conflict(
    state: tauri::State<'_, AppState>,
    conflict_id: String,
    choice: String,
) -> IpcResult<()> {
    let parsed = match choice.as_str() {
        "keep_local" => ConflictChoice::KeepLocal,
        "take_remote" => ConflictChoice::TakeRemote,
        "keep_both" => ConflictChoice::KeepBoth,
        _ => {
            return Err(IpcError::from(passwordms_core::CoreError::Validation(
                "choice".to_string(),
            )))
        }
    };
    let mut session = state.session.lock().map_err(lock_err)?;
    session.repo_mut()?.resolve_conflict(&conflict_id, parsed)?;
    Ok(())
}

// ===== 内部辅助 =====

/// 加载或创建 Host 长期身份密钥对（持久化于应用数据目录）。
fn load_or_create_identity(app: &tauri::AppHandle) -> IpcResult<HostIdentity> {
    let path = identity_path(app)?;
    if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        if let Ok(bytes) = STANDARD.decode(raw.trim().as_bytes()) {
            if bytes.len() == 32 {
                let mut secret = [0u8; 32];
                secret.copy_from_slice(&bytes);
                return Ok(HostIdentity::from_secret_bytes(&secret));
            }
        }
    }
    let identity = HostIdentity::generate()?;
    std::fs::write(&path, STANDARD.encode(identity.secret_bytes()).as_bytes())?;
    Ok(identity)
}

/// 便于用户识别的本机展示名（不含任何隐私信息）。
fn host_display_name(device_id: &str) -> String {
    let short: String = device_id.chars().take(8).collect();
    format!("PasswordMS-{short}")
}

/// 在协议端口范围内选取随机端口。
fn random_port() -> u16 {
    let span = (SYNC_PORT_MAX - SYNC_PORT_MIN) as u32 + 1;
    match rng::random_bytes(4) {
        Ok(bytes) => {
            let v = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            SYNC_PORT_MIN + (v % span) as u16
        }
        Err(_) => SYNC_PORT_MIN,
    }
}

/// 后台轮询线程：非阻塞等待 Joiner 连接，处理一次配对 + 同步后退出。
fn spawn_pairing_worker(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));

            let state = app.state::<AppState>();
            let has_server = match state.pairing.lock() {
                Ok(guard) => guard.is_some(),
                Err(_) => false,
            };
            if !has_server {
                break;
            }

            let outcome = {
                let mut guard = match state.pairing.lock() {
                    Ok(g) => g,
                    Err(_) => break,
                };
                match guard.as_mut() {
                    Some(server) => {
                        let mut session = match state.session.lock() {
                            Ok(s) => s,
                            Err(_) => break,
                        };
                        match session.repo_mut() {
                            Ok(repo) => server.poll_accept_and_sync(repo, now_ms()),
                            // 会话已锁定 → 终止等待。落盘留痕：这类静默退出曾让"二维码还挂着但会话已死"无从排查。
                            Err(_) => {
                                crate::state::append_log(&app, "[pairing] worker exiting: session lock busy");
                                break;
                            }
                        }
                    }
                    None => break,
                }
            };

            match outcome {
                Ok(None) => continue,
                Ok(Some(result)) => {
                    eprintln!("[pairing] host session completed: applied={}", result.applied);
                    crate::state::append_log(&app, &format!("[pairing] host session completed: applied={}", result.applied));
                    if let Ok(mut g) = state.pairing.lock() {
                        *g = None;
                    }
                    // D-44：通知前端会话已结束，收回二维码（否则"二维码还挂着但会话已死"）。
                    let _ = tauri::Emitter::emit(&app, "pairing://ended", "completed");
                    break;
                }
                Err(e) => {
                    // 诊断留痕：CoreError 的 Debug 只有错误码（Io 额外带 stage|kind），无隐私载荷。落盘到数据目录 passwordms.log。
                    eprintln!("[pairing] host session aborted: {e:?}");
                    crate::state::append_log(&app, &format!("[pairing] host session aborted: {e:?}"));
                    if let Ok(mut g) = state.pairing.lock() {
                        *g = None;
                    }
                    let _ = tauri::Emitter::emit(&app, "pairing://ended", "aborted");
                    break;
                }
            }
        }
    });
}
