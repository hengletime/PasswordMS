//! 保险库命令：创建 / 解锁 / 锁定 / 改主密码 / 导出 / 从备份恢复（架构文档 §5.2）。

use std::path::Path;

use passwordms_core::error::CoreError;
use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::store::container::restore_backup_from;

use crate::dto::StatusDto;
use crate::error::{lock_err, IpcError, IpcResult};
use crate::state::{ensure_resync_listener, stop_resync_listener, vault_path, AppState};

/// F-01 创建保险库。
#[tauri::command(rename_all = "snake_case")]
pub fn create_vault(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    master_password: String,
    strength_confirmed: bool,
) -> IpcResult<StatusDto> {
    let path = vault_path(&app)?;
    let session = Session::create(&path, &master_password, strength_confirmed)?;
    *state.lock_session().map_err(lock_err)? = session;
    // 解锁即启动持久重同步监听（D-11）。
    let _ = ensure_resync_listener(&app)?;
    Ok(StatusDto {
        has_vault: true,
        locked: false,
    })
}

/// F-01 解锁（失败统一错误码）。
#[tauri::command(rename_all = "snake_case")]
pub fn unlock(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    master_password: String,
) -> IpcResult<StatusDto> {
    let path = vault_path(&app)?;
    let session = Session::unlock(&path, &master_password)?;
    // D-91（审查 C4）：解锁期维护失败不再静默——落文件日志（内容仅错误码，
    // CoreError Debug 已脱敏，无任何密钥/明文）。
    for warning in session.maintenance_warnings() {
        crate::state::append_log(&app, &format!("[maintenance] {warning}"));
    }
    *state.lock_session().map_err(lock_err)? = session;
    // 解锁即启动持久重同步监听（D-11）。
    let _ = ensure_resync_listener(&app)?;
    Ok(StatusDto {
        has_vault: true,
        locked: false,
    })
}

/// F-02 锁定并清零敏感内存。
#[tauri::command(rename_all = "snake_case")]
pub fn lock(app: tauri::AppHandle) -> IpcResult<()> {
    perform_lock(&app)
}

/// 锁定级联的**唯一实现**（D-108 3.4-6：`lock` 命令与 auto-lock 兜底 worker 共用，
/// 保证两条入口的清理动作永不漂移）。
pub fn perform_lock(app: &tauri::AppHandle) -> IpcResult<()> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    {
        // 先释放 session 锁，避免与重同步工作线程（resync → session）形成锁序倒置。
        let mut session = state.lock_session().map_err(lock_err)?;
        session.lock();
    }
    // A-03 + §3.3「锁定兼容」：锁定即清掉本应用写入的剪贴板内容。
    // 必须在丢弃记忆值之前执行——若先置 None，30 秒倒计时到点时会因记忆值缺失
    // 而静默不清理，密码将永久残留在系统剪贴板。
    // 用户若已把剪贴板换成别的内容，clear_owned_clipboard 会正确保持不动。
    let _ = crate::commands::system_cmd::clear_owned_clipboard(app, state.inner());
    // A-04：导入预览缓存持有完整条目明文，锁定须先擦后清。
    // 顺序不能颠倒——clear() 直接 drop 值，不会覆写明文缓冲。
    // D-106（豆包审查 3.4-1）：锁中毒不得跳过清理（A-04 旁路）——
    // into_inner() 兜底拿 guard 照常擦除；仍失败则留痕（CoreError Debug 脱敏）。
    match state.pending_imports.lock() {
        Ok(mut guard) => {
            for preview in guard.values_mut() {
                preview.wipe_plaintext();
            }
            guard.clear();
        }
        Err(poisoned) => {
            let mut guard = poisoned.into_inner();
            for preview in guard.values_mut() {
                preview.wipe_plaintext();
            }
            guard.clear();
            crate::state::append_log(
                app,
                "[lock] pending_imports mutex poisoned; wiped via into_inner",
            );
        }
    }
    // 锁定后停止重同步监听，避免持有明文上下文。
    stop_resync_listener(app);
    Ok(())
}

/// O-08 修改主密码（仅重包裹 VK）。
#[tauri::command(rename_all = "snake_case")]
pub fn change_master_password(
    state: tauri::State<'_, AppState>,
    old_password: String,
    new_password: String,
) -> IpcResult<()> {
    let mut session = state.lock_session().map_err(lock_err)?;
    session.change_master_password(&old_password, &new_password)?;
    // D-113（`docs/08` §18.5 约束 2）：改密会让 `verify_mac` 变 ⇒ 现有生物识别封印**必然**
    // 被判失效（core 的 `bind` 已覆盖，不会交出错误密钥）。但「改密即失效」只对应用层成立：
    // 旧密文里的 VK 在密码学意义上仍然有效（§4.3 H-2）。真正撤销 = 删掉这条封印，
    // 否则「已失效的密文」会长期躺在数据目录里。删除失败不阻断改密（改密本身已成功）。
    let seal_path = session.repo()?.path();
    if let Err(e) = passwordms_core::biometric::SealStore::new(&seal_path).remove() {
        // 命令层无 AppHandle，走 eprintln（桌面 GUI 会丢弃，但 Android logcat 可见）。
        eprintln!("[biometric] remove seal after password change failed: {e:?}");
    }
    Ok(())
}

/// O-03 导出保险库（需重新验证主密码；二次确认与明文风险提示由前端负责）。
#[tauri::command(rename_all = "snake_case")]
pub fn export_vault(
    state: tauri::State<'_, AppState>,
    dest_path: String,
    master_password: String,
) -> IpcResult<()> {
    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    repo.export_vault(Path::new(&dest_path), &master_password)?;
    Ok(())
}

/// D-55 从备份恢复保险库文件（锁屏页「从备份恢复」入口）。
///
/// - **当前会话必须处于锁定态**：Unlocked 时内存中的 KEK/VK 属于旧库，替换库文件后
///   二者失配，后续操作必错。校验与替换全程持有 session 互斥锁（无锁序倒置风险：
///   本函数不获取 resync 锁），避免「检查时锁定、复制时被解锁」的竞态。
/// - **覆盖保护**：本机 `passwordms.vault` 已存在时，先复制为同目录
///   `passwordms.vault.bak-<unix_ms>`，恢复失败或反悔时还有后路。
/// - magic 校验与复制在核心层 [`restore_backup_from`]（`fs::copy` **保留 mtime**，
///   与导出语义一致）；不合法的备份文件返回 `ERR_VALIDATION`，不触碰目标文件。
/// - **D-94**：Android 上 `source_path` 可能是 `content://…`（SAF 选择器），经
///   [`crate::content_uri::copy_to_file`] 流式落临时文件再走同一套校验落位。
/// - 恢复后**不自动解锁**：`unlock_check` 校验的是备份创建时的主密码，解锁与失败
///   提示由前端用既有 `unlock` 流程完成。
#[tauri::command(rename_all = "snake_case")]
pub fn restore_backup(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    source_path: String,
) -> IpcResult<()> {
    let session = state.lock_session().map_err(lock_err)?;
    if session.is_unlocked() {
        return Err(CoreError::Validation("vault_is_unlocked".to_string()).into());
    }
    let dest = vault_path(&app)?;
    // 覆盖保护：原库先留 .bak 后路（unix_ms 时间戳避免同名覆盖）。
    if dest.exists() {
        let bak = dest.with_file_name(format!("passwordms.vault.bak-{}", now_ms()));
        std::fs::copy(&dest, &bak)?;
    }
    // D-94：Android 上 SAF 选择器返回 `content://…`（ContentProvider URI 没有内核
    // 路径），直接 `Path::new` 交给 std::fs 必失败 →「文件读写错误」。经 provider fd
    // 流式拷到 vault 同目录临时文件（无内存尖峰、无大小上限），再走既有
    // restore_backup_from 的尺寸/magic/整库帧流全套校验落位，成败都删临时文件。
    // 桌面该分支恒假（open() 返回真实路径），行为零变化。
    if source_path.starts_with("content://") {
        let tmp = dest.with_file_name(format!("passwordms.restore-{}.tmp", now_ms()));
        let outcome = crate::content_uri::copy_to_file(&app, &source_path, &tmp)
            .and_then(|()| restore_backup_from(&tmp, &dest).map_err(IpcError::from));
        let _ = std::fs::remove_file(&tmp);
        outcome?;
        return Ok(());
    }
    restore_backup_from(Path::new(&source_path), &dest)?;
    Ok(())
}

/// D-61 读取自动锁定档位（分钟；`Vault.auto_lock_minutes` 明文头部字段）。
#[tauri::command(rename_all = "snake_case")]
pub fn get_auto_lock(state: tauri::State<'_, AppState>) -> IpcResult<u16> {
    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    Ok(repo.auto_lock_minutes())
}

/// D-61 设置自动锁定档位（仅接受 1/5/15，非法值 `ERR_VALIDATION`；幂等）。
#[tauri::command(rename_all = "snake_case")]
pub fn set_auto_lock(state: tauri::State<'_, AppState>, minutes: u16) -> IpcResult<()> {
    let mut session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    repo.set_auto_lock_minutes(minutes)?;
    Ok(())
}
