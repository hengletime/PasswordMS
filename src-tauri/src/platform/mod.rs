//! 平台适配层：把 core 的 [`passwordms_core::biometric::VaultSealer`] 接到真实平台密钥设施
//! （`docs/08-biometric-unlock.md` §5 / §6 / §23）。
//!
//! 本层**只做平台调用**——封印结构、绑定熵、状态分类全部在
//! `passwordms_core::biometric`（平台无关，任何机器可跑、可测）。这里的代码只回答两件事：
//!
//! 1. 「现在能不能做生物识别」——[`PresenceGate::availability`] / `VaultSealer::availability`；
//! 2. 「用 `bind` 当附加认证数据，把 VK 加密 / 解密」——[`VaultSealer::seal`] / `unseal`。
//!
//! ## 为什么把「在场校验」抽成 trait
//!
//! 真机的 [`PresenceGate`] 会弹系统对话框，**无法在自动化测试里跑**。而「`bind` 必须真的
//! 参与加密」这件事只有适配器自己知道——core 侧用的是 Mock，适配器忽略 `bind` 时 core
//! 测试依然全绿（§17.5 约束 2）。抽出 trait 后，测试可以注入「永远通过」的闸门，直接对
//! **真实的 DPAPI 调用**断言「换 `bind` 必须解不开」（§18.5 约束 1）。
//!
//! ## 三个平台分支
//!
//! | 平台 | 适配器 | 形态 |
//! |---|---|---|
//! | Windows | `WinSealer<HelloGate>` | 两段式：**TPM2 优先 / DPAPI 兜底** + `UserConsentVerifier` 闸门（§6.1 / §33） |
//! | Android | `KeystoreSealer` | **一段式**：Keystore 密钥硬绑生物识别，`CryptoObject` 让闸门与加解密合一（§23） |
//! | 其它 | `UnsupportedSealer` | 恒 `NotImplemented`，前端整卡不渲染（D-116） |

use passwordms_core::biometric::{Availability, SealError};

/// 在场校验的用途（决定系统对话框文案）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceReason {
    /// 开启快速解锁（写封印）。
    Enable,
    /// 用快速解锁打开保险库（读封印）。
    Unlock,
}

/// 在场校验闸门：确认「此刻是本人在操作」。
///
/// 实现方**不得**缓存可用性（`VaultSealer::availability` 的约定：每次现查）。
pub trait PresenceGate: Send + Sync {
    /// 当前可用性（映射平台结论，不抛错——错误在 [`Self::request`] 里报）。
    fn availability(&self) -> Availability;

    /// 请求一次在场校验。用户取消 → [`SealError::Canceled`]（**不是**失效）。
    fn request(&self, reason: PresenceReason) -> Result<(), SealError>;
}

// 纯逻辑（别名派生 / 可用性映射）：无平台依赖，桌面可测。
// 非 Android 构建里这些函数没有调用方（`android_keystore` 不编译），但**测试仍要用**，
// 所以按平台放宽 dead_code——不要为了消警告把它们删掉或塞进 cfg(android)。
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
mod keystore_rules;

#[cfg(target_os = "android")]
mod android_keystore;
#[cfg(not(any(windows, target_os = "android")))]
mod unsupported;
#[cfg(windows)]
mod windows_dpapi;
#[cfg(windows)]
mod windows_sealer;
#[cfg(windows)]
mod windows_tpm;

#[cfg(target_os = "android")]
pub use android_keystore::KeystoreSealer;
#[cfg(not(any(windows, target_os = "android")))]
pub use unsupported::UnsupportedSealer;
#[cfg(windows)]
pub use windows_dpapi::{DpapiSealer, HelloGate};
#[cfg(windows)]
pub use windows_sealer::WinSealer;

/// 本机构建对应的适配器类型。
///
/// Windows 是**一个适配器两种手段**（TPM2 优先、DPAPI 兜底），而不是两个类型：
/// 封印记录的 subtype 只在 `unlock` 里被读一次，谁去解由 [`WinSealer`] 自己按密文分派。
#[cfg(windows)]
pub type PlatformSealer = WinSealer<HelloGate>;
#[cfg(target_os = "android")]
pub type PlatformSealer = KeystoreSealer<tauri::Wry>;
#[cfg(not(any(windows, target_os = "android")))]
pub type PlatformSealer = UnsupportedSealer;

/// 构造当前平台的适配器（每次现造：封印操作本身是低频的，省掉全局可变状态）。
///
/// Windows 用不到 `app`，但签名统一成吃 `AppHandle`——Android 侧要靠它拿到注册时
/// 存进 state 的 [`crate::BiometricHandle`]（Tauri 的 `PluginHandle`）。
#[cfg(windows)]
pub fn platform_sealer(_app: &tauri::AppHandle) -> PlatformSealer {
    WinSealer::new()
}

#[cfg(target_os = "android")]
pub fn platform_sealer(app: &tauri::AppHandle) -> PlatformSealer {
    use tauri::Manager;
    KeystoreSealer::new(app.state::<crate::BiometricHandle>().inner().0.clone())
}

#[cfg(not(any(windows, target_os = "android")))]
pub fn platform_sealer(_app: &tauri::AppHandle) -> PlatformSealer {
    UnsupportedSealer
}

/// 删掉平台侧的密钥材料（撤销的实质动作，`SealStore::remove()` 的搭档）。
///
/// - Windows：DPAPI **没有**可删的密钥句柄（主密钥由系统账户派生，不属于我们），
///   但 TPM2 封印器有一把持久密钥，必须删；两件事都由 [`WinSealer::purge`] 处理。
/// - Android：必须真删 Keystore 条目，否则「关闭快速解锁」只删了文件、密钥还留在系统里，
///   撤销就不彻底。
#[cfg(windows)]
pub fn purge_platform_key(sealer: &PlatformSealer, bind: &[u8]) {
    sealer.purge(bind);
}

#[cfg(target_os = "android")]
pub fn purge_platform_key(sealer: &PlatformSealer, bind: &[u8]) {
    sealer.purge_key(bind);
}

#[cfg(not(any(windows, target_os = "android")))]
pub fn purge_platform_key(_sealer: &PlatformSealer, _bind: &[u8]) {}

/// 唤起系统的生物识别 / 登录设置页（D-114 的成果在 Android 上延续）。
///
/// **无参数**：目标页面是编译期常量，前端无法借这条通道打开任意 Intent / 协议
/// （`open_url` 的 http/https 白名单是 PRD F-05 不变量，不能为它放宽）。
/// 失败恒 `false`——调用方只据此报「打不开」，界面照常可用。
///
/// 三种平台的落点：Windows → `ms-settings:signinoptions`；
/// Android → `Settings.ACTION_BIOMETRIC_ENROLL`（API 30+，低版本退到安全设置）；
/// 其它 → 恒 `false`。
#[cfg(windows)]
pub fn open_biometric_settings(_app: &tauri::AppHandle) -> bool {
    windows_dpapi::open_hello_settings()
}

#[cfg(target_os = "android")]
pub fn open_biometric_settings(_app: &tauri::AppHandle) -> bool {
    // D-122（2026-10-28 主人拍板）：Android **不再尝试跳转**。
    //
    // 曾经这条走 `run_mobile_plugin("openSettings")` → Kotlin `startActivity(ACTION_
    // BIOMETRIC_ENROLL)`。命令名对齐之后仍然跳不过去，主人真机现象是「点了完全没有反应，
    // 一点提示都没有」——注意这**不是**失败被吞：`startActivity` 若抛异常，Kotlin 会
    // `invoke.reject`，Rust 拿 Err，前端一定弹红字。全静默说明 Intent 确实发出去了，
    // 是 ROM 忽略或拉起后立刻 finish。
    //
    // 国产 ROM 把生物识别设置放在各家自己的位置，没有任何 action 能保证命中，客户端也
    // 无法判定「跳成功了没有」（`resolveActivity` 在 API 30+ 受包可见性影响会误报）。
    // 留一个点了没反应的按钮比不给按钮更糟，所以退化为纯文案引导（前端
    // `settings.bio_manage_hint_android`），把「去哪个大类里找」和「各品牌不同」说清楚。
    //
    // Windows 的 `ms-settings:signinoptions` 是文档化协议、稳定可用，那条保留。
    false
}

#[cfg(not(any(windows, target_os = "android")))]
pub fn open_biometric_settings(_app: &tauri::AppHandle) -> bool {
    false
}

// ── 主窗口句柄（仅 Windows，供 Hello 对话框指定 owner window）──────────────
//
// 为什么要存：`IUserConsentVerifierInterop::RequestVerificationForWindowAsync` 要求传
// owner HWND，而 Hello 闸门是从命令线程调进去的，那条路径上没有 `AppHandle`。
//
// 为什么用**启动时写一次的原子值**而不是 `OnceLock<HWND>`：HWND 是 `*mut c_void`，
// 不是 `Send + Sync`，塞进静态量要绕过类型系统。存 `isize` 等价且不需要 unsafe 断言。
//
// 失效处理：窗口若被销毁重建（当前版本不会发生——单窗口、不重建），值会过期，
// 由调用侧 `IsWindow` 把关后回落老 API，不抛错（§34.4）。
#[cfg(windows)]
mod main_hwnd {
    use std::sync::atomic::{AtomicIsize, Ordering};

    /// 0 = 尚未登记。
    static RAW: AtomicIsize = AtomicIsize::new(0);

    pub fn set(hwnd: isize) {
        RAW.store(hwnd, Ordering::Release);
    }

    pub fn get() -> Option<isize> {
        match RAW.load(Ordering::Acquire) {
            0 => None,
            v => Some(v),
        }
    }
}

/// 登记主窗口句柄（`lib.rs` 启动时调一次）。
///
/// 取不到窗口不算错误：Linux/macOS 没有这条路径，Windows 上极端情况下（窗口延迟创建）
/// 也只是继续用 UWP 版 API，功能不变。
#[cfg(windows)]
pub fn note_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    let win = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next());
    let Some(w) = win else {
        // 拿不到窗口不致命（闸门会回落 UWP 版），但**必须留痕**：
        // 否则「Win11 明明该挂主窗口却静默走了老路」这种回退没人能发现。
        crate::state::append_log(app, "bio: setup 阶段拿不到主窗口，Hello 将回落 UWP 版");
        return;
    };
    match w.hwnd() {
        Ok(h) => {
            let raw = h.0 as isize;
            main_hwnd::set(raw);
            crate::state::append_log(app, &format!("bio: 主窗口句柄已登记 hwnd={raw}"));
        }
        Err(e) => crate::state::append_log(
            app,
            &format!("bio: 取主窗口句柄失败（{e}），Hello 将回落 UWP 版"),
        ),
    }
}

/// 句柄有效性校验（抽出成独立函数，**为了可测**）。
///
/// 失效句柄一律 `None`：`RequestVerificationForWindowAsync` 拿到无效 owner 会
/// 直接失败，而失败发生在「用户已经点了按钮」之后——那时再回落已经晚了，
/// 用户看到的是错误而不是认证框。所以在**进入闸门之前**就把无效句柄挡掉。
///
/// 不额外判 `raw == 0`：`IsWindow(0)` 本身就返回 FALSE（已用变异验证确认——
/// 把那行判断删掉，`zero_hwnd_is_none` 依然绿），多写一层只是噪声。
#[cfg(windows)]
pub(crate) fn hwnd_if_valid(raw: isize) -> Option<isize> {
    let hwnd = windows::Win32::Foundation::HWND(raw as *mut core::ffi::c_void);
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)).as_bool() }
        .then_some(raw)
}

/// 当前登记的主窗口句柄（未登记 / 已失效 → `None`）。
///
/// 每次现查 `IsWindow`，不缓存「有效」判断：窗口销毁后这里必须立刻不给值，
/// 否则 Hello 对话框会挂到一个不存在的 owner 上。
#[cfg(windows)]
pub(crate) fn main_window_hwnd() -> Option<isize> {
    main_hwnd::get().and_then(hwnd_if_valid)
}

#[cfg(all(test, windows))]
mod hwnd_tests {
    use super::*;

    #[test]
    fn zero_hwnd_is_none() {
        assert_eq!(hwnd_if_valid(0), None, "0 不是句柄");
    }

    #[test]
    fn bogus_hwnd_is_rejected() {
        // 0x1234 不可能是本进程的窗口；IsWindow 必须否掉。
        assert_eq!(hwnd_if_valid(0x1234), None);
    }

    #[test]
    fn real_window_is_accepted() {
        // 桌面窗口恒存在，是「有效句柄」的稳定样本。
        let d = unsafe { windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow() };
        let raw = d.0 as isize;
        assert_ne!(raw, 0);
        assert_eq!(hwnd_if_valid(raw), Some(raw), "有效句柄必须放行");
    }

    #[test]
    fn unregistered_means_none() {
        // 测试进程不会调 setup，所以寄存器应为空。
        // 若这里失败，说明有测试污染了全局状态 → 必须修，不是放宽断言。
        assert_eq!(main_window_hwnd(), None);
    }
}
