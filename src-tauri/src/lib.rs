//! Tauri 外壳：注册插件、命令与托管状态（架构文档 §4.5、§5.2）。
//!
//! 业务逻辑全部下沉到 `passwordms-core`；本层只做「参数解包 → 调核心 → 错误映射」。

pub mod app_events;
pub mod commands;
mod content_uri;
pub mod dto;
pub mod error;
// 系统集成是桌面专属：移动端没有托盘、没有全局热键插件、也没有自启（见 Cargo.toml 的
// target 条件依赖与 `docs/06` §6.33）。Android 上设置卡按 DTO 的 `supported` 不渲染。
#[cfg(desktop)]
pub mod hotkey;
pub mod platform;
pub mod prefs;
// 快速搜索小窗同样是桌面专属：移动端只有一个窗口，也没有"任意程序里按热键唤出"这回事。
#[cfg(desktop)]
pub mod quick_search;
pub mod state;
#[cfg(desktop)]
pub mod tray;

use crate::commands::system_cmd;
use crate::state::AppState;
use tauri::Manager as _;

/// Android Kotlin 插件句柄（`gen/android/.../BiometricPlugin.kt`）。
///
/// 只能在 `register_android_plugin` 之后拿到，所以由插件 `setup` 存进 Tauri 的托管状态；
/// `platform::platform_sealer()` / `open_biometric_settings()` 再从 state 取回来。
/// Windows 不需要——DPAPI 没有需要登记的插件。
#[cfg(target_os = "android")]
pub struct BiometricHandle(pub tauri::plugin::PluginHandle<tauri::Wry>);

/// 「宿主把我叫回前台了」的事件名（D-123 引入，D-124 修正触发源）。
///
/// 只在移动端有意义：桌面窗口真的丢焦点又拿回来时，前端自己的 `focus` 事件就够了；
/// 而安卓从系统设置切回来时 WebView 里的 `document` 从未失焦，`focus` 不会触发，
/// 只有宿主生命周期知道这件事。
///
/// **与 `src/lib/app-resume.ts` 的 `APP_RESUMED_EVENT` 是跨语言字面值**：两边写成
/// 不一样也不会有任何报错，只会静默失效（D-118 就是同一类）。守卫测试逐字比对。
#[cfg(any(target_os = "android", target_os = "ios"))]
pub const APP_RESUMED_EVENT: &str = "app://resumed";

/// 快速解锁的 Kotlin 插件（Android 专用；其它平台是个空壳，`setup` 里什么都不注册）。
///
/// 为什么必须走 Tauri 插件机制：Rust 要调 Kotlin 只有两条路——直连 JNI（得自己管
/// Activity/线程/Attach）或经 `PluginHandle::run_mobile_plugin`（Tauri 已把这些都封好）。
/// 后者要求插件先在 `PluginManager` 里登记，也就是这里的 `setup`。
///
/// 注意 `Builder::new("pmsbiometric")` 的名字就是 `PluginManager.load(...)` 用的键，
/// 而 `register_android_plugin` 的第二参是**类名**，两者含义不同，别写反。
fn biometric_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("pmsbiometric")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    api.register_android_plugin("com.passwordms.app", "BiometricPlugin")?;
                app.manage(BiometricHandle(handle));
            }
            #[cfg(not(target_os = "android"))]
            {
                let _ = (app, api);
            }
            Ok(())
        })
        .build()
}

/// 启动时把应用级配置从磁盘读进托管状态（v1.4.10）。
///
/// 读不到 / 读坏了都回落默认值并留痕——**配置问题绝不能让程序起不来**，
/// 万一 prefs.json 被写坏，用户至少还能打开软件去修它。
fn load_prefs(app: &tauri::AppHandle) {
    let Ok(path) = crate::state::prefs_path(app) else {
        crate::state::append_log(app, "[prefs] 数据目录不可用，使用默认配置");
        return;
    };
    let (prefs, warn) = crate::prefs::load_or_default(&path);
    if let Some(w) = warn {
        crate::state::append_log(app, &w);
    }
    match app.try_state::<AppState>() {
        Some(state) => match state.prefs.lock() {
            Ok(mut guard) => *guard = prefs,
            Err(_) => crate::state::append_log(app, "[prefs] 状态锁中毒，使用默认配置"),
        },
        None => crate::state::append_log(app, "[prefs] 托管状态未就绪，使用默认配置"),
    }
}

/// 启动应用。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    // D-128 单实例守卫：**必须是第一个注册的插件**。
    //
    // 官方要求它先于其它插件初始化，否则第二个实例可能还没走到检查就已经建好了自己的
    // 窗口/托盘，出现两个图标闪一下再退的观感。
    //
    // 为什么非做不可（不只是"体验问题"）：
    // - v1.4.10 起关闭窗口只是**隐藏到托盘**，进程常驻。用户以为关了、再点一次图标，
    //   就成了第二个实例；
    // - 两个实例共用同一个 `passwordms.vault` 与 `biometric.json`，还会同时抢 TPM 上
    //   那把持久密钥——`unseal` 的 transient 重试一旦耗尽就会被归类成"密文被篡改"，
    //   也就是主人看到的「快速解锁配置被篡改或损坏」（D-127 的直接诱因）。
    //
    // 第二个实例不做任何业务：把已有窗口唤到前台，然后由插件自己退出。
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        // 小窗若正开着，它盖在主窗口上面；先让它让位（失焦也会自动收起，这里主动一点）。
        crate::quick_search::hide(app);
        crate::tray::show_main(app);
        crate::state::append_log(app, "[single] 已有实例在运行，已唤出其主窗口，本实例退出");
    }));

    let builder = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init());

    // v1.4.10 系统集成：全局热键 + 开机自启 + 关窗最小化。
    //
    // 三者都是**桌面专属**，必须整段 cfg(desktop)：移动端没有 `tauri::menu` / `tauri::tray`，
    // 这两个插件也没有移动端实现。曾经把它们放进公共依赖，结果
    // `cargo check --target aarch64-linux-android` 直接炸 17 个错误（2026-10-29 实测）。
    #[cfg(desktop)]
    let builder = builder
        // 两个插件都**只在后端使用**，前端拿不到对应权限（见 `capabilities/default.json`）。
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        // 关窗改为隐藏到托盘（托盘缺失时自动退回"关窗即退出"，见 `tray::on_window_event`）。
        .on_window_event(|window, event| match window.label() {
            // 只劫持主窗口：小窗的关闭是"收起"，绝不能沿用到托盘那套"首次提示"逻辑。
            "main" => crate::tray::on_window_event(window.app_handle(), event),
            // 小窗是"用完就走"的浮层：点别处（失焦）立即收起。
            //
            // 这条是复制密码后能不能顺手粘贴的关键——用户复制完要切到目标程序去，
            // 小窗若还浮在屏幕上，就正好挡住刚填的那一行。
            crate::quick_search::LABEL => {
                if matches!(event, tauri::WindowEvent::Focused(false)) {
                    let _ = window.hide();
                }
            }
            _ => {}
        });

    let result = builder
        .plugin(biometric_plugin())
        .manage(AppState::new())
        // D-108（3.4-6）：auto-lock 的 Rust 兜底线程——WebView 后台节流时前端
        // 定时器停摆，闲置锁定由后端按 last_activity_ms 仲裁。
        .setup(|app| {
            crate::state::spawn_auto_lock_worker(app.handle().clone());
            // §34：Hello 对话框要挂到主窗口上，先把 HWND 记下来。
            // 取不到不是错误（窗口可能稍后才创建）——闸门会自动回落到 UWP 版 API。
            #[cfg(windows)]
            crate::platform::note_main_window(app.handle());
            load_prefs(app.handle());
            // 托盘失败不阻断启动：只是没有托盘图标（关窗会退化成"必须用托盘退出"的
            // 反面——所以同时把 hotkey 也跳过，避免"热键唤不出窗口又没法从托盘恢复"）。
            #[cfg(desktop)]
            match crate::tray::install(app.handle()) {
                Ok(()) => {
                    crate::hotkey::install_from_prefs(app.handle());
                    // 预建隐藏小窗：首次按热键零等待（理由见 `quick_search` 模块注释）。
                    // 建不出来不影响主程序——热键届时会退回"唤主窗口"路径。
                    match crate::quick_search::ensure(app.handle()) {
                        Ok(_) => {
                            crate::state::append_log(app.handle(), "[quick] 小窗已预建（隐藏）")
                        }
                        Err(e) => crate::state::append_log(
                            app.handle(),
                            &format!("[quick] 预建小窗失败：{e}"),
                        ),
                    }
                }
                Err(e) => crate::state::append_log(app.handle(), &e),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // system
            commands::system_cmd::get_status,
            commands::system_cmd::copy_secret,
            commands::system_cmd::clear_secret_if_own,
            commands::system_cmd::open_url,
            commands::system_cmd::vault_file_info,
            commands::system_cmd::vault_compaction_stats,
            commands::system_cmd::compact_vault,
            // vault
            commands::vault_cmd::create_vault,
            commands::vault_cmd::unlock,
            commands::vault_cmd::lock,
            commands::vault_cmd::change_master_password,
            commands::vault_cmd::export_vault,
            commands::vault_cmd::restore_backup,
            commands::vault_cmd::get_auto_lock,
            commands::vault_cmd::set_auto_lock,
            // biometric（D-113 / docs/08 P2）
            commands::biometric_cmd::biometric_status,
            commands::biometric_cmd::biometric_enable,
            commands::biometric_cmd::biometric_disable,
            commands::biometric_cmd::biometric_unlock,
            commands::biometric_cmd::open_biometric_settings,
            // item
            commands::item_cmd::list_items,
            commands::item_cmd::get_item,
            commands::item_cmd::create_item,
            commands::item_cmd::update_item,
            commands::item_cmd::delete_item,
            commands::item_cmd::toggle_favorite,
            // ordering（#531：钉住 / 频度 / 归组）
            commands::ordering_cmd::record_item_usage,
            commands::ordering_cmd::set_item_pin,
            commands::ordering_cmd::reorder_pinned_items,
            commands::ordering_cmd::set_folder_pin,
            commands::ordering_cmd::reorder_pinned_folders,
            commands::ordering_cmd::move_item_to_folder,
            // folder
            commands::folder_cmd::list_folders,
            commands::folder_cmd::create_folder,
            commands::folder_cmd::update_folder,
            commands::folder_cmd::delete_folder,
            commands::folder_cmd::reorder_folders,
            // keydoc
            commands::keydoc_cmd::add_attachment,
            commands::keydoc_cmd::remove_attachment,
            commands::keydoc_cmd::export_attachment,
            commands::keydoc_cmd::attachment_blob_ready,
            // generator
            commands::generator_cmd::generate_password,
            commands::generator_cmd::totp_now,
            // import
            commands::import_cmd::import_preview,
            commands::import_cmd::import_apply,
            commands::import_cmd::save_import_template,
            // trash
            commands::trash_cmd::list_trash,
            commands::trash_cmd::restore_item,
            commands::trash_cmd::purge_item,
            commands::trash_cmd::empty_trash,
            // health
            commands::health_cmd::health_check,
            // sync
            commands::sync_cmd::sync_start_pairing,
            commands::sync_cmd::sync_cancel_pairing,
            commands::sync_cmd::sync_join,
            commands::sync_cmd::list_paired_devices,
            commands::sync_cmd::rename_paired_device,
            commands::sync_cmd::unpair,
            commands::sync_cmd::sync_now,
            commands::sync_cmd::sync_status,
            // D-134：本机同步网络信息（只读诊断，手动同步失败时定性用）
            commands::sync_cmd::sync_net_info,
            commands::sync_cmd::list_conflicts,
            commands::sync_cmd::resolve_conflict,
            // settings
            commands::settings_cmd::get_settings,
            commands::settings_cmd::update_settings,
            // prefs（v1.4.10 系统集成：热键 / 开机自启 / 托盘文案）
            commands::prefs_cmd::prefs_get,
            commands::prefs_cmd::prefs_set_hotkey,
            commands::prefs_cmd::prefs_set_autostart,
            commands::prefs_cmd::tray_set_labels,
        ])
        .build(tauri::generate_context!());

    // 【D-96】退出兜底：进程退出前把「本应用写入且仍未被消费」的剪贴板内容清掉。
    // 前端 30s 倒计时与 copy_secret 的兜底线程都只在应用存活时有效；直接退出
    // （关闭窗口 / 任务栏退出）时密码可能刚好还挂在剪贴板上——`RunEvent::Exit`
    // 在事件循环结束时同步触发，是最后一个清理窗口。
    match result {
        Ok(app) => {
            app.run(|app_handle, event| match event {
                tauri::RunEvent::Exit => {
                    if let Some(state) = app_handle.try_state::<AppState>() {
                        // 刻意走 `system_cmd::` 短路径而非 `commands::system_cmd::` 全路径：
                        // ipc-contract 契约测试以 `commands::<module>::<fn>` 提取
                        // generate_handler! 注册面，本处是普通函数调用、不是命令注册，
                        // 撞上模式会造成「注册面多出一个幽灵命令」的假红。
                        let _ = system_cmd::clear_owned_clipboard(app_handle, state.inner());
                    }
                }
                // 【D-124】回前台通知（修正 D-123 接错的那一层）。
                //
                // D-123 接的是 `tauri::RunEvent::Resumed`，而那条**在安卓上从不触发**：
                // `tauri-runtime-wry` 只在 `Event::NewEvents(StartCause::Poll)` 里发
                // `RunEvent::Resumed`，那是桌面事件循环的路径；移动端的生命周期由 tao 的
                // ndk-glue 从 `onResume` 送上来，落地形式是**窗口事件**
                // `Event::WindowEvent { event: WindowEvent::Resumed }`，也就是下面这条。
                //
                // 上一版因此是一段永不执行的分支——真机表现「去系统设置录了指纹，切回来
                // 卡片一点不变」（主人 2026-10-28 第二次实测）。上一版编译通过、单测全绿，
                // 因为字面量本身是对的，错的是**挂在哪一层**：这类错误只有源码级守门能挡。
                //
                // 变体只在移动端存在（`#[cfg(mobile)]`），桌面目标必须 cfg 掉，
                // 否则编译报「没有这个变体」。
                #[cfg(any(target_os = "android", target_os = "ios"))]
                tauri::RunEvent::WindowEvent {
                    event: tauri::WindowEvent::Resumed,
                    ..
                } => {
                    use tauri::Emitter;
                    let _ = app_handle.emit(APP_RESUMED_EVENT, ());
                }
                _ => {}
            });
        }
        // 不使用 unwrap/expect：启动失败时打印并退出。
        Err(err) => {
            eprintln!("PasswordMS fatal startup error: {err}");
            std::process::exit(1);
        }
    }
}
