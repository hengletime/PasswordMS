//! 局域网访问权限（D-149）。
//!
//! ## 为什么这不是「一条普通诊断信息」
//!
//! Android 17（API 37）起，`targetSdk >= 37` 的应用访问局域网必须持有
//! `ACCESS_LOCAL_NETWORK`（NEARBY_DEVICES 运行时权限组），**默认关闭**。缺它时系统对
//! 本 UID 的 AppOps 恒为 `ignore`，包被**静默丢弃**：手机侧 `ss -tne` 卡在 SYN-SENT、
//! 收不到 RST，于是配对和同步都表现为「超时」。用户能想到的排查方向（WiFi、路由器、
//! 防火墙、息屏省电）**全部无效**，因为问题出在权限上。
//!
//! `MainActivity.ensureLocalNetworkPermission()` 自 D-141 起就一直在申请，但结果只写进
//! logcat。于是「一台手机明明连着 WiFi，只有本应用同步不上」这件事在界面上没有任何
//! 落点。本模块把那一个布尔值送到前端，让同步页能直说「是权限没给」。
//!
//! ## 两条命令的分工
//!
//! | 命令 | 何时调 | 平台副作用 |
//! |---|---|---|
//! | [`lan_permission_status`] | 同步页加载 / 从系统设置切回前台 | 无（只读） |
//! | [`lan_permission_request`] | 用户点「授予权限」 | 弹一次系统对话框（可能） |
//!
//! ## 桌面为什么也返回一份而不是报错
//!
//! `supported = false, granted = true` = 「这个问题在本平台不存在」。前端只按
//! `supported && !granted` 渲染警示，所以桌面与旧版 Android 都自然不显示——不需要前端
//! 再做平台判断（前端不该知道 `__TAURI_INTERNALS__` 里有没有 android 字段）。

use crate::error::IpcResult;

/// 局域网访问权限状态。
#[derive(Debug, Clone, serde::Serialize)]
pub struct LanPermissionDto {
    /// 当前系统**是否存在**这个权限。Android < 17 与全部桌面平台恒 `false`。
    ///
    /// 前端据此决定整块提示渲不渲染：「未授予」在不存在此权限的平台上是个伪问题，
    /// 报出来只会把用户赶去系统设置里找一个永远找不到的开关。
    pub supported: bool,
    /// 是否已授予。`supported == false` 时恒 `true`（语义：不构成阻塞）。
    pub granted: bool,
}

/// Kotlin `status` / `request` 的回包形状。
///
/// `request` 只回 `granted`，故两个字段都给 `#[serde(default)]`——同一个 struct 复用，
/// 缺的那个按 `false` 落。**别**为省一个 struct 把它改成 `Option`：前端拿到 `null`
/// 还得再判一次，而这里「缺字段」的唯一来源就是「这个命令本来就不回它」。
#[cfg(target_os = "android")]
#[derive(serde::Deserialize)]
struct KotlinResp {
    #[serde(default)]
    supported: bool,
    #[serde(default)]
    granted: bool,
}

/// 读取当前局域网访问权限状态（只读，同步页每次加载都问一次）。
///
/// `(async)` 不是可选项：Android 侧的 `run_mobile_plugin` 会阻塞等 Kotlin 回复，
/// 而 Tauri 的**同步**命令直接在 IPC 线程上跑函数体。万一那条线程就是主线程，
/// 会和「`run_command` 把 JNI 调用投递到主线程」撞成死锁（同 `biometric_cmd` 的教训）。
#[tauri::command(rename_all = "snake_case", async)]
pub fn lan_permission_status(app: tauri::AppHandle) -> IpcResult<LanPermissionDto> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager as _;
        let handle = app.state::<crate::LanPermissionHandle>().inner().0.clone();
        // 传 `{}` 而不是 `()`：Kotlin 侧 `invoke.getArgs()` 会 `JSObject(argsJson)`，
        // 拿到 `null` 直接抛 JSONException。
        return match handle.run_mobile_plugin::<KotlinResp>("status", serde_json::json!({})) {
            Ok(r) => Ok(LanPermissionDto {
                supported: r.supported,
                granted: r.granted,
            }),
            Err(e) => {
                // 插件调不通 ⇒ 报「不适用」而**不是**「未授予」。后者会顶一条永远消不掉的
                // 警示，把用户赶去找一个可能根本不存在的开关；而真正的原因（插件没注册上）
                // 用户无能为力，只能由日志说话。
                crate::state::append_log(&app, &format!("[lanperm] status 调用失败：{e}"));
                Ok(LanPermissionDto {
                    supported: false,
                    granted: true,
                })
            }
        };
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = &app;
        Ok(LanPermissionDto {
            supported: false,
            granted: true,
        })
    }
}

/// 主动申请一次局域网访问权限（用户点「授予权限」时调）。
///
/// 返回**申请之后**的实际状态，而不是「弹窗有没有弹出来」——后者对用户没有意义，
/// 他要知道的是「现在能不能同步」。用户按返回键取消时 `granted` 为 `false`，前端
/// 保留警示并补一句「可去系统设置手动打开」。
///
/// 失败（宿主未就绪 / 插件没注册）时返回 `Err`，前端按普通错误 toast 并**保留警示**：
/// 绝不能因为一次调用出错就假装权限已经在。
#[tauri::command(rename_all = "snake_case", async)]
pub fn lan_permission_request(app: tauri::AppHandle) -> IpcResult<LanPermissionDto> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager as _;
        let handle = app.state::<crate::LanPermissionHandle>().inner().0.clone();
        let r = handle
            .run_mobile_plugin::<KotlinResp>("request", serde_json::json!({}))
            .map_err(|e| {
                crate::state::append_log(&app, &format!("[lanperm] request 调用失败：{e}"));
                // 不新造错误码：`ErrorCode` 与 `shell_codes` 是逐字契约（契约测试守着），
                // 为一个低频失败加码要同时改前端类型、四语言文案与测试。按 biometric_cmd
                // 的既有做法——码用合法的 `ERR_INTERNAL`，`message_key` 精确化到本场景。
                crate::error::IpcError {
                    code: passwordms_core::CoreError::Internal(String::new())
                        .code()
                        .to_string(),
                    message_key: "sync.lan_perm_failed".to_string(),
                    record_count: None,
                }
            })?;
        // `supported` 由「是否真的存在这个权限」决定，不该由回包推断——request 成功返回
        // 本身就说明系统 >= 17（低版本走不到这里，Kotlin 直接回 true）。
        return Ok(LanPermissionDto {
            supported: true,
            granted: r.granted,
        });
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = &app;
        Ok(LanPermissionDto {
            supported: false,
            granted: true,
        })
    }
}
