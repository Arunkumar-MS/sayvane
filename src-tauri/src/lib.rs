pub mod audio;
pub mod brand;
pub mod commands;
pub mod core;
pub mod credentials;
pub mod db;
pub mod export;
pub mod gesture;
pub mod insertion;
pub mod ipc_types;
pub mod jobs;
pub mod paths;
pub mod platform;
pub mod providers;
pub mod session;
pub mod settings;
pub mod shell;
pub mod speech;
pub mod stats;

use tauri::Manager;

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let builder = tauri::Builder::default()
        // One instance per user: a second launch opens the existing app.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            shell::show_main(app, ipc_types::Page::Home);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| shell::handle_shortcut(app, shortcut, event.state()))
                .build(),
        );
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .setup(|app| {
            shell::setup(app.handle())?;
            Ok(())
        })
        .on_menu_event(|app, event| shell::handle_menu_event(app, event.id().as_ref()))
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::start_dictation,
            commands::finish_dictation,
            commands::cancel_session,
            commands::list_history,
            commands::get_stats,
            commands::recover_entry,
            commands::delete_entry,
            commands::copy_entry,
            commands::copy_last,
            commands::paste_last,
            commands::import_audio,
            commands::cancel_import,
            commands::export_entry,
            commands::delete_all_data,
            commands::list_models,
            commands::install_model,
            commands::cancel_model_install,
            commands::remove_model,
            commands::import_model_file,
            commands::list_inputs,
            commands::update_settings,
            commands::set_polish_enabled,
            commands::set_shortcut,
            commands::suspend_shortcuts,
            commands::request_permission,
            commands::open_permission_settings,
            commands::refresh_permissions,
            commands::reset_permissions,
            commands::list_profiles,
            commands::save_profile,
            commands::delete_profile,
            commands::set_active_profile,
            commands::credential_status,
            commands::save_credential,
            commands::delete_credential,
            commands::refresh_provider_models,
            commands::test_connection,
            commands::open_main,
            commands::reset_bar_position,
            commands::show_bar_menu,
            commands::set_bar_hit_region,
            commands::reveal_data_dir,
            commands::open_url,
        ])
        .build(tauri::generate_context!())
        .expect("error while building application")
        .run(|app, event| match event {
            // Closing the last window keeps the tray app running.
            tauri::RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
            tauri::RunEvent::Exit => {
                if let Some(core) = app.try_state::<core::CoreRef>() {
                    core.cancel_session();
                    core.speech.shutdown();
                }
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => shell::show_main(app, ipc_types::Page::Home),
            _ => {}
        });
}
