//! Tauri 外壳：注册插件、命令与托管状态（架构文档 §4.5、§5.2）。
//!
//! 业务逻辑全部下沉到 `passwordms-core`；本层只做「参数解包 → 调核心 → 错误映射」。

pub mod commands;
pub mod dto;
pub mod error;
pub mod state;

use crate::state::AppState;

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
            // vault
            commands::vault_cmd::create_vault,
            commands::vault_cmd::unlock,
            commands::vault_cmd::lock,
            commands::vault_cmd::change_master_password,
            commands::vault_cmd::export_vault,
            commands::vault_cmd::restore_backup,
            // item
            commands::item_cmd::list_items,
            commands::item_cmd::get_item,
            commands::item_cmd::create_item,
            commands::item_cmd::update_item,
            commands::item_cmd::delete_item,
            commands::item_cmd::toggle_favorite,
            // folder
            commands::folder_cmd::list_folders,
            commands::folder_cmd::create_folder,
            commands::folder_cmd::update_folder,
            commands::folder_cmd::delete_folder,
            commands::folder_cmd::reorder_folders,
            // keydoc
            commands::keydoc_cmd::import_attachment,
            commands::keydoc_cmd::export_attachment,
            // generator
            commands::generator_cmd::generate_password,
            commands::generator_cmd::totp_now,
            // import
            commands::import_cmd::import_preview,
            commands::import_cmd::import_apply,
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
            commands::sync_cmd::unpair,
            commands::sync_cmd::sync_now,
            commands::sync_cmd::sync_status,
            commands::sync_cmd::list_conflicts,
            commands::sync_cmd::resolve_conflict,
            // settings
            commands::settings_cmd::get_settings,
            commands::settings_cmd::update_settings,
        ])
        .run(tauri::generate_context!());

    // 不使用 unwrap/expect：启动失败时打印并退出。
    if let Err(err) = result {
        eprintln!("PasswordMS fatal startup error: {err}");
        std::process::exit(1);
    }
}
