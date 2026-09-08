#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod commands;
mod config;
mod history;
mod hotkey;
mod inject;
mod state;
mod textproc;
mod whisper_engine;

use state::AppState;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};

fn main() {
    let cfg = config::load_config();
    let db = history::open().expect("не удалось открыть базу истории диктовок");
    let app_state = AppState::new(cfg, db);

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::engine_ping,
            commands::get_config,
            commands::set_config,
            commands::capture_next_hotkey,
            commands::start_dictation,
            commands::stop_dictation,
            commands::search_history,
            commands::clear_history,
        ])
        .setup(|app| {
            setup_tray(app)?;
            setup_global_hotkey(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                let minimize = state.config.lock().unwrap().minimize_to_tray;
                if minimize {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("ошибка запуска DictaFlow");
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Открыть DictaFlow", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("DictaFlow — голос в текст")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Регистрирует низкоуровневый listener клавиатуры (rdev), реагирующий
/// на press&hold настроенной в конфиге клавиши из ЛЮБОГО приложения Windows,
/// не только когда окно DictaFlow в фокусе.
fn setup_global_hotkey(app: tauri::AppHandle) {
    let app_for_get_key = app.clone();
    let app_for_press = app.clone();
    let app_for_release = app.clone();

    hotkey::spawn_listener(
        move || {
            let state = app_for_get_key.state::<AppState>();
            let name = state.config.lock().unwrap().hotkey.clone();
            hotkey::key_from_name(&name)
        },
        move || {
            let app = app_for_press.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                let _ = commands::start_dictation(app.clone(), state).await;
            });
        },
        move || {
            let app = app_for_release.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                let _ = commands::stop_dictation(app.clone(), state).await;
            });
        },
    );
}
