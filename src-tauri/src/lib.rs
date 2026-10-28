//! Tauri 外壳：注册插件、命令与托管状态（架构文档 §4.5、§5.2）。
//!
//! 业务逻辑全部下沉到 `passwordms-core`；本层只做「参数解包 → 调核心 → 错误映射」。

pub mod commands;
mod content_uri;
pub mod dto;
pub mod error;
pub mod platform;
pub mod state;

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

/// 启动应用。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(biometric_plugin())
        .manage(AppState::new())
        // D-108（3.4-6）：auto-lock 的 Rust 兜底线程——WebView 后台节流时前端
        // 定时器停摆，闲置锁定由后端按 last_activity_ms 仲裁。
        .setup(|app| {
            crate::state::spawn_auto_lock_worker(app.handle().clone());
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
            commands::sync_cmd::list_conflicts,
            commands::sync_cmd::resolve_conflict,
            // settings
            commands::settings_cmd::get_settings,
            commands::settings_cmd::update_settings,
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
