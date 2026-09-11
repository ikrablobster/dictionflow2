use crate::audio::{AudioCapture, AudioDeviceInfo};
use crate::config::{self, AppConfig};
use crate::history;
use crate::hotkey;
use crate::inject;
use crate::state::AppState;
use crate::textproc;
use crate::whisper_engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use std::sync::atomic::Ordering;
use tauri_plugin_autostart::ManagerExt;

#[derive(Serialize, Clone)]
pub struct EngineStatus {
    pub state: String,
    pub message: Option<String>,
}

fn emit_status(app: &AppHandle, state: &str, message: Option<String>) {
    let status = EngineStatus { state: state.into(), message };
    *app.state::<AppState>().status.lock().unwrap() = status.clone();
    let _ = app.emit("dictation://status", status);
}

fn set_overlay_visible(app: &AppHandle, visible: bool) {
    if let Some(win) = app.get_webview_window("overlay") {
        if visible { let _ = win.show(); } else { let _ = win.hide(); }
    }
}

#[tauri::command]
pub fn engine_ping() -> Result<bool, String> { Ok(true) }

#[tauri::command]
pub fn get_engine_status(state: State<AppState>) -> EngineStatus {
    state.status.lock().unwrap().clone()
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Result<AppConfig, String> {
    Ok(state.config.lock().unwrap().clone())
}

#[tauri::command]
pub fn set_config(app: AppHandle, state: State<AppState>, config: AppConfig) -> Result<(), String> {
    config::validate(&config)?;
    let _operation = state.operation.try_lock().map_err(|_| "Дождитесь завершения диктовки")?;
    if *state.is_listening.lock().unwrap() { return Err("Сначала остановите диктовку".into()); }
    let previous = state.config.lock().unwrap().clone();
    if config.start_with_windows != previous.start_with_windows {
        let autostart = app.autolaunch();
        if config.start_with_windows { autostart.enable() } else { autostart.disable() }
            .map_err(|e| e.to_string())?;
    }
    if let Err(error) = config::save_config(&config) {
        if config.start_with_windows != previous.start_with_windows {
            let autostart = app.autolaunch();
            let _ = if previous.start_with_windows { autostart.enable() } else { autostart.disable() };
        }
        return Err(error.to_string());
    }
    *state.config.lock().unwrap() = config;
    Ok(())
}

#[tauri::command]
pub async fn list_input_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    tokio::task::spawn_blocking(AudioCapture::list_input_devices)
        .await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_audio_level(state: State<AppState>) -> Result<f32, String> {
    Ok(state.audio.lock().unwrap().level())
}

#[tauri::command]
pub async fn start_microphone_test(state: State<'_, AppState>, device: String) -> Result<(), String> {
    let _operation = state.operation.try_lock().map_err(|_| "Дождитесь завершения диктовки")?;
    if *state.is_listening.lock().unwrap() { return Err("Сначала остановите диктовку".into()); }
    state.audio.lock().unwrap().start_test(&device).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_microphone_test(state: State<'_, AppState>) -> Result<(), String> {
    let Ok(_operation) = state.operation.try_lock() else { return Ok(()); };
    if !*state.is_listening.lock().unwrap() { let _ = state.audio.lock().unwrap().stop(); }
    Ok(())
}

#[tauri::command]
pub async fn capture_next_hotkey() -> Result<String, String> {
    tokio::task::spawn_blocking(|| hotkey::capture_single_press(10_000))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn start_dictation(app: AppHandle, state: State<'_, AppState>, insert: Option<bool>) -> Result<(), String> {
    let _operation = state.operation.try_lock().map_err(|_| "Дождитесь завершения текущей операции")?;
    if *state.is_listening.lock().unwrap() { return Ok(()); }
    *state.insert_target.lock().unwrap() = inject::foreground_window();
    let cfg = state.config.lock().unwrap().clone();
    config::validate(&cfg)?;
    state.audio.lock().unwrap().stop();
    emit_status(&app, "loading", Some("Подготовка модели распознавания… При первом запуске требуется интернет.".into()));
    let result = async {
        let progress_app = app.clone();
        let path = whisper_engine::ensure_model_downloaded(&cfg.model_size, move |message| {
            emit_status(&progress_app, "loading", Some(message));
        }).await.map_err(|e| e.to_string())?;
        let engine_app = app.clone();
        tokio::task::spawn_blocking(move || engine_app.state::<AppState>().engine.load_model(&path))
            .await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        state.audio.lock().unwrap().start(&cfg.input_device).map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    }.await;
    if let Err(error) = result {
        set_overlay_visible(&app, false);
        emit_status(&app, "error", Some(error.clone()));
        return Err(error);
    }
    *state.is_listening.lock().unwrap() = true;
    *state.insert_result.lock().unwrap() = insert.unwrap_or(false);
    let session = state.session.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = app.emit("dictation://partial", String::new());
    if cfg.show_transcription_overlay { set_overlay_visible(&app, true); }
    emit_status(&app, "listening", None);
    let timeout_app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(600)).await;
        let state = timeout_app.state::<AppState>();
        if state.session.load(Ordering::SeqCst) == session {
            let _ = stop_dictation(timeout_app.clone(), state).await;
        }
    });
    if cfg.live_preview { spawn_live_preview(app.clone(), cfg.language_mode, session); }
    Ok(())
}

fn spawn_live_preview(app: AppHandle, language_mode: String, session: u64) {
    tauri::async_runtime::spawn(async move {
        let mut last_preview = String::new();
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(1800)).await;
            let state = app.state::<AppState>();
            if state.session.load(Ordering::SeqCst) != session { break; }
            let samples = state.audio.lock().unwrap().snapshot();
            if samples.len() < 16_000 { continue; }
            let worker_app = app.clone();
            let lang = language_mode.clone();
            let result = tokio::task::spawn_blocking(move || {
                let state = worker_app.state::<AppState>();
                if state.session.load(Ordering::SeqCst) != session { return Ok(None); }
                state.engine.transcribe(&samples, &lang).map(Some)
            }).await;
            if state.session.load(Ordering::SeqCst) != session { break; }
            match result {
                Ok(Ok(Some(result))) => {
                    let text = result.text.trim().to_string();
                    if !text.is_empty() && text != last_preview {
                        last_preview = text.clone();
                        let _ = app.emit("dictation://partial", text);
                    }
                }
                Ok(Ok(None)) => break,
                other => {
                    let message = match other {
                        Ok(Err(e)) => e.to_string(), Err(e) => e.to_string(), _ => unreachable!(),
                    };
                    emit_status(&app, "listening", Some(format!("Предпросмотр недоступен: {message}. Остановите запись для итогового распознавания.")));
                    break;
                }
            }
        }
    });
}

#[tauri::command]
pub async fn stop_dictation(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    // Hotkey commands are queued in order, including release during model loading.
    let _operation = state.operation.lock().await;
    if !*state.is_listening.lock().unwrap() { return Ok(()); }
    *state.is_listening.lock().unwrap() = false;
    state.session.fetch_add(1, Ordering::SeqCst);
    emit_status(&app, "processing", None);
    let samples = state.audio.lock().unwrap().stop();
    let cfg = state.config.lock().unwrap().clone();
    let insert = *state.insert_result.lock().unwrap();
    let result = async {
        if samples.len() < 1600 { return Ok(()); }
        let worker_app = app.clone();
        let lang = cfg.language_mode.clone();
        let transcription = tokio::task::spawn_blocking(move || {
            worker_app.state::<AppState>().engine.transcribe(&samples, &lang)
        }).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        if transcription.text.trim().is_empty() {
            return Err("Речь не распознана. Проверьте выбранный микрофон и язык, затем повторите запись.".into());
        }
        let mut text = textproc::apply_voice_commands(&transcription.text, cfg.voice_commands);
        text = textproc::apply_custom_dictionary(&text, &cfg.custom_dictionary);
        text = textproc::auto_punctuate(&text, cfg.auto_punctuation);
        if cfg.cloud_enabled && cfg.grammar_correction && !cfg.cloud_api_key.is_empty() {
            if let Ok(corrected) = textproc::cloud_grammar_correct(&text, &cfg.cloud_api_key, &transcription.language).await {
                if !corrected.is_empty() { text = corrected; }
            }
        }
        // Publish and preserve text even if insertion into another application fails.
        let _ = app.emit("dictation://text", text.clone());
        let _ = app.emit("dictation://partial", text.clone());
        history::insert(&state.db.lock().unwrap(), &text, &transcription.language, None)
            .map_err(|e| format!("Текст распознан, но история не сохранена: {e}"))?;
        if insert {
            let mode = cfg.insertion_mode.clone();
            let target = *state.insert_target.lock().unwrap();
            tokio::task::spawn_blocking(move || {
                if target.is_some() && target != inject::foreground_window() {
                    return Err(anyhow::anyhow!("Активное окно изменилось. Скопируйте текст из истории."));
                }
                inject::insert_text(&text, &mode)
            })
                .await.map_err(|e| e.to_string())?
                .map_err(|e| format!("Текст сохранён в истории, но вставка не удалась: {e}"))?;
        }
        Ok::<(), String>(())
    }.await;
    set_overlay_visible(&app, false);
    match &result {
        Ok(()) => emit_status(&app, "idle", None),
        Err(error) => emit_status(&app, "error", Some(error.clone())),
    }
    result
}

#[tauri::command]
pub fn search_history(state: State<AppState>, query: String) -> Result<Vec<history::HistoryEntry>, String> {
    history::search(&state.db.lock().unwrap(), &query).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_history(state: State<AppState>) -> Result<(), String> {
    history::clear(&state.db.lock().unwrap()).map_err(|e| e.to_string())
}
