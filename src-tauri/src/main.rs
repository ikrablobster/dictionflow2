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
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
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
            commands::list_input_devices,
            commands::get_audio_level,
            commands::start_microphone_test,
            commands::stop_microphone_test,
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
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    let state = window.state::<AppState>();
                    if state.config.lock().unwrap().minimize_to_tray {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("ошибка запуска DictaFlow");
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "Настройки", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "Просмотреть историю", true, None::<&str>)?;
    let paste = MenuItem::with_id(app, "paste_last", "Вставить последнюю транскрипцию", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let restart = MenuItem::with_id(app, "restart", "Перезапустить...", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Полностью выйти из DictaFlow", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &history, &sep1, &paste, &restart, &quit])?;

    TrayIconBuilder::with_id("dictaflow-main-tray")
        .menu(&menu)
        .tooltip("DictaFlow — голос в текст")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" | "history" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                    let _ = win.emit("dictaflow://navigate", event.id.as_ref());
                }
            }
            "paste_last" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    let items = {
                        let db = state.db.lock().unwrap();
                        history::search(&db, "")
                    };

                    if let Ok(items) = items {
                        if let Some(last) = items.first() {
                            let cfg = state.config.lock().unwrap().clone();
                            let _ = inject::insert_text(&last.text, &cfg.insertion_mode);
                        }
                    }
                });
            }
            "restart" => app.restart(),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                if let Some(win) = tray.app_handle().get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
        })
        .build(app)?;
    Ok(())
}

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
