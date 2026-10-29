mod commands;
mod container;
mod crypto;
mod docx_engine;
mod domain;
mod error;
mod image_engine;
mod legal;
mod pdf_engine;
mod services;
mod storage;

use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Debe ir primero: si Firmar ya está abierto, «Firmar con Firmar» le
        // pasa el archivo a esa ventana en vez de abrir otra.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(path) = commands::file_argument(args) {
                let _ = app.emit("open-file", path.display().to_string());
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = commands::init_state(app)?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::identity_info,
            commands::vault_list,
            commands::vault_save,
            commands::vault_delete,
            commands::document_read,
            commands::document_inspect,
            commands::document_original,
            commands::docx_layout,
            commands::document_sign,
            commands::document_resign,
            commands::document_sign_batch,
            commands::history_list,
            commands::history_forget,
            commands::take_launch_file,
            commands::legal_status,
            commands::legal_accept,
            commands::data_wipe,
            commands::identity::identity_get,
            commands::identity::identity_set_name,
            commands::identity::identity_export_card,
            commands::identity::contacts_list,
            commands::identity::contact_import,
            commands::identity::contact_rename,
            commands::identity::contact_remove,
            commands::identity::lock_status,
            commands::identity::lock_set_pin,
            commands::identity::lock_disable,
            commands::identity::lock_set_options,
            commands::identity::lock_unlock_pin,
            commands::identity::lock_unlock_hello,
            commands::identity::lock_now,
            commands::backup_export,
            commands::backup_inspect,
            commands::backup_restore,
            commands::reveal_path,
            commands::open_path,
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Firmar");
}
