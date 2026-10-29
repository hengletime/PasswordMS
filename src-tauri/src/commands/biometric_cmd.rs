//! 生物识别快速解锁命令（`docs/08-biometric-unlock.md` §5 / §4.5）。
//!
//! 四个入口，职责刻意分开：
//!
//! | 命令 | 何时可调 | 是否要主密码 | 说明 |
//! |---|---|---|---|
//! | [`biometric_status`] | 锁定态 / 解锁态都可 | 否 | 只读：平台可用性 + 封印状态，解锁页据此决定显示什么 |
//! | [`biometric_enable`] | 仅解锁态 | **是** | 防「解锁后离开座位，旁人顺手开一条永久免密通道」 |
//! | [`biometric_disable`] | 任意 | 否 | 降级操作、不泄露数据；忘掉主密码的用户必须能自救 |
//! | [`biometric_unlock`] | 仅锁定态 | 否 | 平台解封 VK → 建会话。失败原因不折叠，按 §10 分类上报 |
//!
//! ## 这一层的三条硬纪律
//!
//! 1. **绝不把 VK 交给前端**：`VaultKey` 全程留在 Rust 侧，IPC 只回 `StatusDto`
//!    （§18.5 约束 6）。
//! 2. **封印身份只从一处取**：`SealTarget` 与 `VaultKey` 都从**同一个** `Repository`
//!    读出来，结构上不可能拼出「A 库的头 + B 库的 VK」（§18.5 约束 9 / 复审 L-6）。
//! 3. **容器只 open 一次**：解锁路径把已经打开的 `Container` 直接交给会话，杜绝
//!    「查封印时是旧头部、建会话时是新头部」（§18.5 约束 10）。

use passwordms_core::biometric::{
    self, Availability, InvalidatedReason, SealRecord, SealState, SealStore, SealTarget,
    VaultSealer,
};
use passwordms_core::error::CoreError;
use passwordms_core::session::Session;
use passwordms_core::store::container::Container;

use crate::dto::{BiometricStatusDto, StatusDto};
use crate::error::{lock_err, IpcResult};
use crate::state::{append_log, ensure_resync_listener, vault_path, AppState};

/// 读取快速解锁状态（解锁页 / 设置页共用；**锁定态可安全调用**）。
///
/// 锁定态下没有 `Repository`，只好读容器头部；这一读是**必然**的——`bind` 的三个输入
/// 全在明文头部里。头部读不出来（无库 / 损坏）时一律报「未开启」，不报错：解锁页不该
/// 因为一个旁路文件而无法显示主密码输入框。
///
/// `(async)` 不是可选项：Android 侧的 `run_mobile_plugin` 会阻塞等 Kotlin 回复，而
/// Tauri 的**同步**命令是直接在 IPC 线程上跑函数体的（`body_blocking` 不派发）。万一
/// 那个线程就是主线程，就会和「`run_command` 把 JNI 调用 post 到主线程」撞成死锁。
/// 标了 `(async)` 后同步函数体的落点是 `sync_threadpool`，彻底避开。
#[tauri::command(rename_all = "snake_case", async)]
pub fn biometric_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> IpcResult<BiometricStatusDto> {
    let path = vault_path(&app)?;
    let sealer = crate::platform::platform_sealer(&app);
    let availability = sealer.availability();
    let store = SealStore::new(&path);

    // 头部来源二选一：解锁态直接用仓储里的（与其它命令同一份），锁定态才读文件。
    // 注意用 `state.session.lock()` 而不是 `lock_session()`：状态查询不算用户活动，
    // 否则解锁页每 2 秒轮询一次就会把自动锁定永远推后。
    let header = {
        let session = state.session.lock().map_err(lock_err)?;
        match session.repo() {
            Ok(repo) => Some(repo.header().clone()),
            Err(_) => None,
        }
    };
    let header = match header {
        Some(h) => Some(h),
        None => Container::open(&path).ok().map(|c| c.header().clone()),
    };

    let Some(header) = header else {
        return Ok(status_dto(availability, "absent", None, None));
    };

    let target = SealTarget::from_header(&header);
    Ok(match store.inspect(&target)? {
        SealState::Absent => status_dto(availability, "absent", None, None),
        SealState::Corrupt => status_dto(availability, "corrupt", None, None),
        SealState::Ready(rec) => status_dto(availability, "ready", None, Some(rec.created_at)),
        SealState::Stale(reason) => {
            status_dto(availability, "stale", Some(reason_name(reason)), None)
        }
    })
}

/// 开启快速解锁：**必须**重新输入主密码（§4.5）。
#[tauri::command(rename_all = "snake_case", async)]
pub fn biometric_enable(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    master_password: String,
) -> IpcResult<BiometricStatusDto> {
    let Some(_busy) = state.try_begin_biometric() else {
        return Err(busy_error());
    };
    let sealer = crate::platform::platform_sealer(&app);
    let availability = sealer.availability();
    if availability != Availability::Available {
        // 开启入口在 UI 上本应已置灰（§18.5 约束 7 的另一半），这里再兜一次：
        // 绝不允许「平台不可用」时写出一条当场用不了的封印。
        return Err(CoreError::BiometricUnavailable.into());
    }

    let mut session = state.lock_session().map_err(lock_err)?;
    // ① 证明知道主密码——生物识别会话（kek=None）同样要过这一关。
    session.verify_master_password(&master_password)?;

    let repo = session.repo_mut()?;
    // ② 空库不给开：没有任何帧能证实 VK，解封出来必被 `unlock_with_opened_container`
    //    以 `NeedMasterPassword` 拒绝 ⇒ 那是一条**永远解不开**的封印（复审 L-2）。
    //
    // 错误码仍用 `ERR_VALIDATION`，但**自带专门的文案 key**：笼统的「输入无效。」
    // 会让用户以为是密码输了，实际要做的是「先建一条记录」。`IpcError` 允许
    // 显式给 message_key，这是它唯一正当的用法——精确化提示而不新增错误码。
    if repo.record_count() == 0 {
        return Err(crate::error::IpcError {
            code: CoreError::Validation(String::new()).code().to_string(),
            message_key: "settings.bio_empty".to_string(),
            record_count: None,
        });
    }
    // ③ 封印身份与 VK **同源**（§18.5 约束 9）：两次都从同一个 `repo` 取，
    //    不可能出现「A 库的头部 + B 库的 VK」。
    let target = SealTarget::from_header(repo.header());
    let vault_key = repo.vault_key();
    let store = SealStore::new(&repo.path());
    let record: SealRecord = biometric::enable(&store, &sealer, &target, vault_key)?;

    Ok(status_dto(
        availability,
        "ready",
        None,
        Some(record.created_at),
    ))
}

/// 关闭快速解锁：删除封印（**撤销的唯一动作**）与残留 tmp。
///
/// 不要求主密码、不要求解锁态：这是降级操作，且「忘掉主密码的人必须能自救」。
///
/// 撤销必须**彻底**：除了删 `biometric.json`，还要删平台侧的密钥材料。
/// Android 的 Keystore 条目如果不删，文件虽没了、密钥还留在系统里——那就不叫撤销。
/// `bind` 要在**删文件之前**算好（删完就读不到头部了）。
#[tauri::command(rename_all = "snake_case", async)]
pub fn biometric_disable(app: tauri::AppHandle) -> IpcResult<()> {
    let path = vault_path(&app)?;
    let bind = Container::open(&path)
        .ok()
        .map(|c| biometric::bind_of(&SealTarget::from_header(c.header())));
    // `remove` 幂等（文件不存在也算成功），因此关闭按钮永远可用。
    SealStore::new(&path).remove()?;
    if let Some(bind) = bind {
        let sealer = crate::platform::platform_sealer(&app);
        crate::platform::purge_platform_key(&sealer, &bind);
    }
    Ok(())
}

/// 用快速解锁打开保险库。
#[tauri::command(rename_all = "snake_case", async)]
pub fn biometric_unlock(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> IpcResult<StatusDto> {
    let Some(_busy) = state.try_begin_biometric() else {
        return Err(busy_error());
    };
    // 已解锁时再点一次解锁是前端状态错乱，不是攻击；明确拒绝而不是悄悄重建会话
    // （重建会丢弃当前未落盘的 UI 状态）。
    {
        let session = state.session.lock().map_err(lock_err)?;
        if session.is_unlocked() {
            return Err(CoreError::Validation("vault_is_unlocked".to_string()).into());
        }
    }

    let path = vault_path(&app)?;
    let sealer = crate::platform::platform_sealer(&app);
    // §18.5 约束 10：整个流程只 open 一次容器——算 `bind` 用的是这份头部，
    // 建会话用的也是这份头部，中间没有「文件被替换」的窗口。
    let container = Container::open(&path)?;
    let store = SealStore::new(&path);
    let target = SealTarget::from_header(container.header());
    let vault_key = match biometric::unlock(&store, &sealer, &target) {
        Ok(vk) => vk,
        // D-119：平台密钥**永久**失效（Android 新增/删除指纹、恢复出厂清掉 Keystore 条目）
        // 时，封印文件本身是完好的——`inspect` 的三道校验（`mac_check` / `wkv_check` /
        // `bind`）全对，它只是再也解不开了。
        //
        // 墓碑由 `biometric::unlock` 自己写（那层才知道 `rec` 长什么样），这里补最后一手：
        // 把平台侧的残余钥匙删干净。**不删 `biometric.json`**——删了状态就退化成
        // 「从未开启」，用户看不到「为什么失效」，也找不到「去系统设置看看」的入口。
        //
        // 判定用 [`biometric::is_platform_revoked`]：只有平台**明确**说密钥永久失效才动，
        // `Tampered` 与改密/换 VK 都不在此列（理由见那个函数的注释）。
        Err(err) => {
            if biometric::is_platform_revoked(&err) {
                let bind = biometric::bind_of(&target);
                crate::platform::purge_platform_key(&sealer, &bind);
                append_log(
                    &app,
                    "[biometric] platform key invalidated by enrollment change; tombstoned",
                );
            }
            // D-127：**每次**失败都留痕。上次「装完新版重开就报被篡改」查不出原因，
            // 就是因为这条路径完全静默——最后只能靠推理。日志里带 subtype 与指纹，
            // 下次一眼能看出是「钥匙换了」还是「密文真坏了」。
            append_log(
                &app,
                &format!(
                    "[biometric] unlock failed: {:?} (subtype={:?}, key_fp={:?})",
                    err,
                    store.inspect(&target).ok().and_then(|s| match s {
                        passwordms_core::biometric::SealState::Ready(r) => r.key_fp,
                        _ => None,
                    }),
                    sealer.key_fingerprint()
                ),
            );
            return Err(err.into());
        }
    };
    let session = Session::unlock_with_opened_container(&path, container, vault_key)?;
    for warning in session.maintenance_warnings() {
        append_log(&app, &format!("[maintenance] {warning}"));
    }
    *state.lock_session().map_err(lock_err)? = session;
    // 与主密码解锁同一套后续动作：启动持久重同步监听（D-11）。
    let _ = ensure_resync_listener(&app)?;
    Ok(StatusDto {
        has_vault: true,
        locked: false,
    })
}

/// 并发保护：已有一次生物识别操作在进行中。
///
/// 用 `ERR_VALIDATION` 而不是生物识别专属错误码：前端对这条的处置是**静默忽略**
/// （用户正在等第一次的对话框），不该弹任何提示。
fn busy_error() -> crate::error::IpcError {
    CoreError::Validation("biometric_busy".to_string()).into()
}

/// 组装 DTO。
///
/// 六个构造点全部走这里——加字段时只改一处，不会出现「某个分支忘了填」导致的
/// 前端读不到 `platform` 而退回通用文案。
fn status_dto(
    availability: Availability,
    state: &str,
    reason: Option<&str>,
    enabled_at: Option<i64>,
) -> BiometricStatusDto {
    BiometricStatusDto {
        availability: availability_name(availability).to_string(),
        state: state.to_string(),
        reason: reason.map(str::to_string),
        enabled_at,
        platform: platform_name().to_string(),
    }
}

/// 当前平台（§23：前端据此选择「Windows Hello」还是「指纹 / 人脸」这套名词）。
///
/// 由后端说出自己是谁，比前端嗅探 UA 可靠——UA 字符串随 WebView 版本和厂商 ROM 漂移。
fn platform_name() -> &'static str {
    #[cfg(windows)]
    {
        "windows"
    }
    #[cfg(target_os = "android")]
    {
        "android"
    }
    #[cfg(not(any(windows, target_os = "android")))]
    {
        "other"
    }
}

/// 可用性的 wire 取值（前端 i18n 键后缀，`settings.bio_avail_*`）。
fn availability_name(a: Availability) -> &'static str {
    match a {
        Availability::Available => "available",
        Availability::NotConfigured => "not_configured",
        // D-125：只录了便捷级（Class 2）生物识别。前端有专门的文案解释「为什么用不了」，
        // 绝不能并进 `not_configured`——那句话会说「本机尚未设置指纹 / 人脸」。
        Availability::WeakOnly => "weak_only",
        Availability::NotPresent => "not_present",
        Availability::DisabledByPolicy => "disabled_by_policy",
        Availability::Busy => "busy",
        Availability::Unsupported => "unsupported",
        // D-116：本平台没有适配器（P3 未完成）。前端据此**整块不渲染**快速解锁卡片——
        // 与 `unsupported`（有适配器但平台做不了，必须可见）严格区分。
        Availability::NotImplemented => "not_implemented",
    }
}

/// 失效原因的 wire 取值（前端 i18n 键后缀，`settings.bio_stale_*`）。
fn reason_name(r: InvalidatedReason) -> &'static str {
    match r {
        InvalidatedReason::MasterPasswordChanged => "master_password_changed",
        InvalidatedReason::VaultKeyChanged => "vault_key_changed",
        InvalidatedReason::BiometricEnrollmentChanged => "enrollment_changed",
        // D-127：平台保护密钥被重置（TPM 清理 / 系统更新 / 重装）。与 `tampered` 分开，
        // 因为引导完全不同：这个重新开启一次就好，且不该让人以为被攻击。
        InvalidatedReason::PlatformKeyReset => "platform_key_reset",
        InvalidatedReason::Tampered => "tampered",
    }
}

/// 打开系统的生物识别设置页（D-114；Android 侧见 §23）。
///
/// 存在理由：`UserConsentVerifier` 在「Windows Hello 从未配置过」的机器上回报的是
/// `DeviceNotPresent`（不是 `NotConfiguredForUser`）。前端照着 `not_present` 显示
/// 「没有可用的硬件」会把用户带进死胡同——他真正的下一步只是去开通。所以卡片的红字
/// 改成引导语之后，还要有这条能**一键跳过去**的通道。
///
/// 无参数、无 URL：目标页面在 Rust / Kotlin 里是编译期常量，前端拿不到参数位，构造不出
/// 别的协议或 Intent（`open_url` 的白名单只放行 http/https，不能为了它放宽）。
#[tauri::command(rename_all = "snake_case", async)]
pub fn open_biometric_settings(app: tauri::AppHandle) -> IpcResult<()> {
    if crate::platform::open_biometric_settings(&app) {
        Ok(())
    } else {
        Err(crate::error::IpcError::internal("open_biometric_settings"))
    }
}
