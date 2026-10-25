//! Tauri 外壳：注册插件、命令与托管状态（架构文档 §4.5、§5.2）。
//!
//! 业务逻辑全部下沉到 `passwordms-core`；本层只做「参数解包 → 调核心 → 错误映射」。

pub mod commands;
mod content_uri;
pub mod dto;
pub mod error;
pub mod state;

use crate::commands::system_cmd;
use crate::state::AppState;
use tauri::Manager as _;

/// 启动应用。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::new())
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
            commands::sync_cmd::sync_accept_pair,
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
            app.run(|app_handle, event| {
                if let tauri::RunEvent::Exit = event {
                    if let Some(state) = app_handle.try_state::<AppState>() {
                        // 刻意走 `system_cmd::` 短路径而非 `commands::system_cmd::` 全路径：
                        // ipc-contract 契约测试以 `commands::<module>::<fn>` 提取
                        // generate_handler! 注册面，本处是普通函数调用、不是命令注册，
                        // 撞上模式会造成「注册面多出一个幽灵命令」的假红。
                        let _ = system_cmd::clear_owned_clipboard(app_handle, state.inner());
                    }
                }
            });
        }
        // 不使用 unwrap/expect：启动失败时打印并退出。
        Err(err) => {
            eprintln!("PasswordMS fatal startup error: {err}");
            std::process::exit(1);
        }
    }
}
