//! 保险库命令：创建 / 解锁 / 锁定 / 改主密码 / 导出 / 从备份恢复（架构文档 §5.2）。

use std::path::Path;

use passwordms_core::error::CoreError;
use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::store::backup;
use passwordms_core::store::container::{restore_backup_from, verify_backup_password};

use crate::dto::{ChangeMasterPasswordDto, StatusDto};
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
) -> IpcResult<ChangeMasterPasswordDto> {
    let mut session = state.lock_session().map_err(lock_err)?;
    // 【R-5】返回的是旧备份的隔离结果：换密**不会**让旧备份失效（它们仍能用旧主密码
    // 打开），所以 core 已把旧代挪进 `backups/old-key/`。隔离失败不会让换密失败，
    // 但必须让用户知道「旧备份还在原处」，否则他会以为旧密码已经彻底作废。
    let result = session.change_master_password(&old_password, &new_password)?;
    // D-113（`docs/08` §18.5 约束 2）：改密会让 `verify_mac` 变 ⇒ 现有生物识别封印**必然**
    // 被判失效（core 的 `bind` 已覆盖，不会交出错误密钥）。但「改密即失效」只对应用层成立：
    // 旧密文里的 VK 在密码学意义上仍然有效（§4.3 H-2）。真正撤销 = 删掉这条封印，
    // 否则「已失效的密文」会长期躺在数据目录里。删除失败不阻断改密（改密本身已成功）。
    let seal_path = session.repo()?.path();
    if let Err(e) = passwordms_core::biometric::SealStore::new(&seal_path).remove() {
        // 命令层无 AppHandle，走 eprintln（桌面 GUI 会丢弃，但 Android logcat 可见）。
        eprintln!("[biometric] remove seal after password change failed: {e:?}");
    }
    Ok(ChangeMasterPasswordDto {
        isolated_backups: result.isolate.moved,
        isolate_failed: result.isolate.failed,
        isolate_error: result.isolate.error.is_some(),
        pre_rekey_saved: result.pre_rekey_saved,
    })
}

/// O-03 导出保险库（需重新验证主密码；二次确认与明文风险提示由前端负责）。
///
/// 返回值为**实际写入目标的字节数**（D-144）：2026-10-31 真机事故里，Android 上
/// 导出静默失败、盘上只剩 SAF 建好的 0 字节占位文件，用户以为备份成功、卸载清库后
/// 才发现无库可恢复。前端拿这个数字直接摆在导出卡片上，比一句「确定」有用得多。
#[tauri::command(rename_all = "snake_case")]
pub fn export_vault(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    dest_path: String,
    master_password: String,
) -> IpcResult<u64> {
    #[cfg(not(target_os = "android"))]
    let _ = &app;

    // D-144：Android 保存对话框返回 `content://…`，ContentProvider URI 没有内核路径。
    // 旧实现把整串当路径交给 `std::fs::copy`（见 core `Repository::export_vault`）→
    // 在 Android 上必失败，而 SAF 的 `ACTION_CREATE_DOCUMENT` **已把目标文件建好**，
    // 于是留下 0 字节文件 + 一条容易被忽略的「文件读写出错」。
    // 现在：先在数据目录生成真备份，再流式写进 provider fd，写完回读长度校验。
    #[cfg(target_os = "android")]
    if dest_path.starts_with("content://") {
        let tmp = vault_path(&app)?.with_file_name(format!("passwordms.export-{}.tmp", now_ms()));
        // 锁内只做「导出到本地」——provider 写入可能慢，不持会话锁做 I/O。
        let staged = {
            let session = state.lock_session().map_err(lock_err)?;
            let repo = session.repo()?;
            repo.export_vault(&tmp, &master_password)
                .map_err(IpcError::from)
        };
        let outcome =
            staged.and_then(|()| crate::content_uri::copy_from_file(&app, &tmp, &dest_path));
        // 临时文件是本机库的明文副本（虽仍是加密容器），成败都必须删掉。
        let _ = std::fs::remove_file(&tmp);
        return outcome;
    }

    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    repo.export_vault(Path::new(&dest_path), &master_password)?;
    // 桌面侧复制后同样回读一次：拿不到长度不算失败（路径可能是不支持 stat 的挂载点），
    // 但拿得到就如实回报，前端才有可显示的事实。
    Ok(std::fs::metadata(Path::new(&dest_path))
        .map(|m| m.len())
        .unwrap_or(0))
}

/// D-145：把「备份不可用」的三种成因翻成用户能行动的文案。
///
/// `restore_backup_from` 对空文件 / 非库文件 / 尾部截断一律返回 `Validation`，
/// 而前端旧映射把 `Validation` 全译成「输入无效。」——用户手上明明是个 `.vault`
/// 文件，却只被告知「输入无效」，无法判断该换个文件还是该重新导出。
/// 这里按成因分叉：**空（0 字节）／不是备份文件／内容损坏**。
fn backup_reject(path: &Path, err: CoreError) -> IpcError {
    let detail = match &err {
        CoreError::Validation(d) => d.as_str(),
        _ => return IpcError::from(err),
    };
    // metadata 取不到时用 MAX 兜底（落「不是备份文件」），不要误判成空文件。
    let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX);
    let key = if len == 0 {
        "err.backup_empty"
    } else if detail == "corrupt_vault_backup" {
        "err.backup_corrupt"
    } else if detail == "unsupported_vault_backup" {
        // F-02：格式版本 / 算法名不认识——用户该做的是升级应用，不是另找文件。
        "err.backup_unsupported"
    } else {
        "err.backup_not_vault"
    };
    IpcError {
        code: err.code().to_string(),
        message_key: key.to_string(),
        record_count: None,
    }
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
/// - **先问不需要凭据的问题，再问需要凭据的问题**：是不是本产品的备份、版本认不认识、
///   帧流完整不完整，这些都先判；密码最后判。这是一条**设计决定**，不是实现顺序的副产品
///   ——顺序对了却没人写下理由，下一个人会为了"先排除坏文件"把凭据校验挪到前面，
///   于是手里拿着一个 `.txt` 的用户会被告知"密码不对"，去试一个根本不存在的密码。
/// - 恢复**先校验再覆盖**：`verify_backup_password` 不过则原库一个字节都不动。
/// - 恢复后**不自动解锁**：解锁与失败提示由前端用既有 `unlock` 流程完成。
#[tauri::command(rename_all = "snake_case")]
pub fn restore_backup(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    source_path: String,
    master_password: String,
    // 用户已在二次确认里明确接受"这次恢复没有自动回滚点"。
    proceed_without_retreat: bool,
) -> IpcResult<()> {
    let session = state.lock_session().map_err(lock_err)?;
    if session.is_unlocked() {
        return Err(CoreError::Validation("vault_is_unlocked".to_string()).into());
    }
    let dest = vault_path(&app)?;

    // D-94：Android 上 SAF 选择器返回 `content://…`（ContentProvider URI 没有内核
    // 路径），直接 `Path::new` 交给 std::fs 必失败 →「文件读写错误」。经 provider fd
    // 流式拷到 vault 同目录临时文件（无内存尖峰、无大小上限），后续**校验与落位都
    // 走这一份**。桌面该分支恒假（open() 返回真实路径），行为零变化。
    // **无条件**先落一份暂存副本，后面的校验与落位都只用它。三条理由指向同一个动作：
    //
    // ① D-94：Android SAF 的 `content://` 没有内核路径，本来就必须先落盘；
    // ② 【F-02】源文件若**就在备份目录里**，`pre-restore` 快照结尾的 `prune()` 会按
    //    "保留最近 N 份"把最旧的那份删掉——用户选中的往往正是它，等于快照亲手删掉
    //    他要救的东西，紧接着 `restore_backup_from` 拿到 NotFound；
    // ③ 【F-06】校验读一次源文件、`restore_backup_from` 再读一次，两次之间若源被换掉
    //    （同步工具、另一个进程、用户手滑动了文件），就是"验的是 A、装的是 B"的
    //    TOCTOU 窗口。校验的全部意义就是不装错东西，那就必须让两次读的是同一份字节。
    //
    // 不用"只在需要时拷贝"的分支：分支少一条，将来漏的就少一处；而代价只是一次
    // 本地文件复制，比"验的和装的不是一份"便宜得多。
    let tmp = dest.with_file_name(format!("passwordms.restore-{}.tmp", now_ms()));
    let staged_copy = if source_path.starts_with("content://") {
        crate::content_uri::copy_to_file(&app, &source_path, &tmp)
    } else {
        std::fs::copy(&source_path, &tmp)
            .map(|_| ())
            .map_err(|e| e.into())
    };
    if let Err(e) = staged_copy {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    let src = tmp;

    let outcome = (|| -> IpcResult<()> {
        // 【P0】先用密码校验这份备份，**顺序不能挪到覆盖之后**：恢复是整容器覆盖，
        // 覆盖完才发现密码不对，原库已经被换掉，用户被挡在门外，而唯一能救他的
        // 是同目录那份 `.bak-<ms>`——界面上没有任何入口指向它。
        // 也刻意排在 `pre-restore` 快照**之前**：这次恢复注定失败，不该为它复制整库。
        verify_backup_password(&src, &master_password)
            .map_err(|e| backup_pwd_reject(&src, e))?;
        // 恢复前的快照必须拍在 [`backup::RestoreGuard`] **之前**：
        // 守卫一旦生效，备份目录整体暂停写入，那时再拍会被直接跳过。
        let pre = backup::snapshot_event(&dest, backup::EventTag::PreRestore, now_ms());
        if let Some(kind) = pre.error_kind {
            // 不阻断（恢复是用户此刻唯一的出路）。留痕由 `.state` 承担——
            // `snapshot_event` 在配额与复制两条失败路径上都写了 `last_error_ms` /
            // `last_error_kind`，设置页「最近一次回滚点没有存成」会显示出来；
            // 这里只补一条 stderr 方便 Android logcat 定位。
            eprintln!(
                "[backup] pre-restore 快照失败 kind={kind:?} stage={:?}",
                pre.error_stage
            );
        }
        // `restore_safety` 此前**一个调用点都没有**——判据写好摆着没人用，等于没有。
        // 这里用它决定要不要**问一句**（不是拦死）：`Blocked` = 那份自动回滚点没拍成。
        //
        // 措辞边界（重要）：`Blocked` **不等于**"完全没退路"——下面**可能**把原库整份
        // 复制成 `passwordms.vault.bak-<ms>`，**但那是 best-effort**：写失败只降级不中止，
        // 而且界面上没有任何入口指向它。`.bak` 落在**库目录**，不经过 `backups/` 的配额，
        // 所以配额挡住快照时它多半写得成、磁盘满或只读时它也写不成——两种情形都真实存在。
        // 因此确认文案**既不许承诺它，也不许断言它不存在**（早先这里写的是"原库一个字节
        // 都不会丢"，等于拿一件不保证的事给文案背书）。只说保证得了的两件事：
        // 这次没有自动回滚点、当前保险库会被覆盖。
        if let backup::RestoreSafety::Blocked(kind) = backup::restore_safety(&dest, &pre) {
            if !proceed_without_retreat {
                return Err(IpcError {
                    code: CoreError::RestoreNoRetreat.code().to_string(),
                    message_key: CoreError::RestoreNoRetreat.message_key().to_string(),
                    record_count: None,
                });
            }
            eprintln!("[backup] restore 在没有自动回滚点的情况下由用户确认继续 kind={kind:?}");
        }

        // 覆盖动作与「用本机备份退回去」共用同一段代码：同一个文件、同一套判据。
        overwrite_vault_with(&src, &dest)?;
        Ok(())
    })();

    // 成败都删临时文件：留在库目录旁边会被人当成备份，而它随时可能是一份半成品。
    let _ = std::fs::remove_file(&src);
    outcome
}

/// 给"即将被覆盖的当前库"留一份 `.bak` 退路（best-effort，**失败只降级不中止**）。
///
/// 【F-07】**必须 sync 并核对长度**：`fs::copy` 返回 `Ok` 不等于字节已经落盘，而紧接着
/// 就要覆盖目标文件。若这时掉电，`.bak` 可能停在"名字有了、内容还在 page cache"的
/// 半份状态——它本来就是"最后一条人工退路"，半份的退路比没有更坏（用户以为能用，
/// 试的时候才发现不对）。长度核对挡住"写了半截"，`sync_all` 挡住"没刷盘"。
///
/// 不做逐字节回读：落位流程对**它自己**的候选文件做逐字节复核，是因为那份要直接顶替
/// 当前库；这里多读一整遍只是把 IO 翻倍，而要防的失败模式（页缓存未刷）已被
/// `sync_all` 覆盖。
///
/// 失败不中止的理由：走到这里说明用户正在救一个打不开的库，中止等于把他锁在门外，
/// 而他手里那份备份本来是好的。半成品必须当场删掉——留着它，将来会被当成可用退路。
fn leave_bak_copy(dest: &Path) {
    if !dest.exists() {
        return;
    }
    let bak = dest.with_file_name(format!("passwordms.vault.bak-{}", now_ms()));
    let ok = std::fs::copy(dest, &bak).is_ok()
        && std::fs::OpenOptions::new()
            .write(true)
            .open(&bak)
            .and_then(|f| f.sync_all())
            .is_ok()
        && std::fs::metadata(dest).map(|m| m.len()).ok()
            == std::fs::metadata(&bak).map(|m| m.len()).ok();
    if !ok {
        let _ = std::fs::remove_file(&bak);
        eprintln!("[backup] restore: 覆盖保护副本写失败，本次恢复没有 .bak 退路");
    }
}

/// 把 `src` 整份覆盖到 `dest`：备份目录守卫 + `.bak` 退路 + 原子落位。
///
/// **两条路必须共用这一段**：「从备份恢复」（用户挑的文件）与「用本机备份退回去」
/// （应用自己拍的 `pre-restore`）覆盖的是同一个文件。任何一条单独演化，另一条就会
/// 悄悄少一次 `sync_all`、少一个守卫，而两边都叫"恢复"——用户没有理由怀疑其中一条
/// 更不安全。判据只有一处，才谈得上"两边一样安全"。
///
/// 调用方负责：`src` 必须是**已暂存的副本**（不能是备份目录里的原文件——下面的
/// 守卫会暂停轮转，但调用方在此之前拍的快照已经裁过一轮），并且已经用密码校验过。
pub(crate) fn overwrite_vault_with(src: &Path, dest: &Path) -> IpcResult<()> {
    // 守卫挡住恢复期间的一切轮转：正在被读的那一份不能被 prune 删掉。
    let _restore_guard = backup::RestoreGuard::new(dest);
    leave_bak_copy(dest);
    restore_backup_from(src, dest).map_err(|e| backup_reject(src, e))
}

/// 「用本机备份退回去」：把库恢复成**最近一次恢复之前**的样子。
///
/// 与「从备份恢复」的区别只有两处，其余全部共用同一段代码：
/// - 源文件由应用自己指定（`backups/pre-restore.<ms>`），用户不必去文件对话框里找；
///   这也是必要的——那个文件没有 `.vault` 后缀，恢复对话框的过滤器根本列不出它。
/// - 调用时界面是**已解锁**的（按钮在设置页），所以本命令要自己把会话锁掉再覆盖。
///
/// 返回恢复点的时刻（毫秒），供界面回显"回到了哪个时间点"。
#[tauri::command(rename_all = "snake_case")]
pub fn restore_latest_point(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    master_password: String,
) -> IpcResult<i64> {
    let dest = vault_path(&app)?;
    // `latest_point` 为 `None` 时**不编造一份"差不多能用的"**（不拿 `pre-rekey` 顶上）：
    // 界面在没有可退的东西时本来就不渲染按钮，走到这里只可能是两次调用之间被删掉了。
    let point = match backup::latest_point(&dest) {
        Some(p) => p,
        None => {
            let e = CoreError::Validation("restore_point_absent".to_string());
            return Err(IpcError {
                code: e.code().to_string(),
                message_key: "settings.restore_point_nothing".to_string(),
                record_count: None,
            });
        }
    };

    // ① 先把源复制到库目录旁边。`pre-restore` 快照自己就躺在 `backups/` 里，而紧接着
    //    那次快照会按配额裁一轮，被裁的优先是最旧的一份——"最旧的一份"恰恰可能就是
    //    用户此刻要用的这一份（第一次恢复之后再没有更早的了）。暂存之后，后面的校验
    //    与落位只读副本，裁剪碰不到它。
    let tmp = dest.with_file_name(format!("passwordms.restore-{}.tmp", now_ms()));
    if std::fs::copy(&point.path, &tmp).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err(CoreError::Io("restore_latest_point: stage".to_string()).into());
    }

    let outcome = (|| -> IpcResult<i64> {
        // ② 先验密码。验不过**一个字节都不动**，而且**还没锁屏**——用户只是输错了密码，
        //    不该被踢回锁屏页。顺序与「从备份恢复」一致：校验永远在覆盖之前。
        verify_backup_password(&tmp, &master_password).map_err(|e| backup_pwd_reject(&tmp, e))?;
        // ③ 给"退回去"这个动作本身也拍一份快照：退错了还能再退回来。
        let pre = backup::snapshot_event(&dest, backup::EventTag::PreRestore, now_ms());
        if let Some(kind) = pre.error_kind {
            // 不阻断：用户此刻在救自己，快照没拍成不该把他挡在这里。
            // 留痕走 `.state` → 设置页常驻行；这里只补一条 stderr 供 Android logcat 定位。
            eprintln!(
                "[backup] 退回去之前的快照失败 kind={kind:?} stage={:?}",
                pre.error_stage
            );
        }
        // ④ **必须先锁再覆盖**。反过来的顺序（覆盖完再锁）会在中间留下一瞬
        //    "内存里是旧库、磁盘上是新库"的分叉，而 `.bak` 与备份目录守卫的语义
        //    都假定这一刻不存在。守卫持有到覆盖结束，也顺带挡住"覆盖到一半有人来解锁"。
        let mut session = state.lock_session().map_err(lock_err)?;
        session.lock();
        overwrite_vault_with(&tmp, &dest)?;
        Ok(point.created_ms)
    })();

    let _ = std::fs::remove_file(&tmp);
    outcome
}

/// 恢复前的密码预检失败 → 用恢复场景自己的文案，而不是通用的「解锁失败」。
///
/// 「解锁失败，请重试」在恢复卡片里会让用户以为自己被锁在外面——他明明还没进门，
/// 真正的问题是手里这份备份不是这个密码加密的。码仍用 `ERR_WRONG_MASTER_PASSWORD`
/// （语义确实是"密码不对"），只换文案 key。
fn backup_pwd_reject(path: &Path, err: CoreError) -> IpcError {
    match err {
        CoreError::WrongMasterPassword => IpcError {
            code: CoreError::WrongMasterPassword.code().to_string(),
            message_key: "lock.restore_password_cannot_open".to_string(),
            record_count: None,
        },
        // 结构性问题（不是备份 / 版本不认识 / 损坏）**必须**走原有分流：那三类各有
        // 各的文案，一律折成"密码不对"等于告诉用户换个密码试试——而他该换的是文件。
        other => backup_reject(path, other),
    }
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
