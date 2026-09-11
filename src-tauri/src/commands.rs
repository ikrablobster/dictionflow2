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

#[derive(Serialize, Clone)]
struct EngineStatus {
    state: String,
    message: Option<String>,
}

fn emit_status(app: &AppHandle, state: &str, message: Option<String>) {
    let _ = app.emit("dictation://status", EngineStatus { state: state.into(), message });
}

fn set_overlay_visible(app: &AppHandle, visible: bool) {
    if let Some(win) = app.get_webview_window("overlay") {
        if visible { let _ = win.show(); } else { let _ = win.hide(); }
    }
}

#[tauri::command]
pub fn engine_ping() -> Result<bool, String> { Ok(true) }

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Result<AppConfig, String> {
    Ok(state.config.lock().unwrap().clone())
}

#[tauri::command]
pub fn set_config(state: State<AppState>, config: AppConfig) -> Result<(), String> {
    config::save_config(&config).map_err(|e| e.to_string())?;
    *state.config.lock().unwrap() = config;
    Ok(())
}

#[tauri::command]
pub fn list_input_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    AudioCapture::list_input_devices().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_audio_level(state: State<AppState>) -> Result<f32, String> {
    Ok(state.audio.lock().unwrap().level())
}

#[tauri::command]
pub fn start_microphone_test(state: State<AppState>, device: String) -> Result<(), String> {
    if *state.is_listening.lock().unwrap() { return Err("Сначала остановите диктовку".into()); }
    state.audio.lock().unwrap().start(&device).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn stop_microphone_test(state: State<AppState>) -> Result<(), String> {
    if !*state.is_listening.lock().unwrap() { let _ = state.audio.lock().unwrap().stop(); }
    Ok(())
}

#[tauri::command]
pub fn capture_next_hotkey() -> Result<String, String> {
    hotkey::capture_single_press(10_000).ok_or_else(|| "Тайм-аут ожидания клавиши".to_string())
}

#[tauri::command]
pub async fn start_dictation(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut listening = state.is_listening.lock().unwrap();
        if *listening { return Ok(()); }
        *listening = true;
    }
    let cfg = state.config.lock().unwrap().clone();
    if let Err(e) = state.audio.lock().unwrap().start(&cfg.input_device) {
        *state.is_listening.lock().unwrap() = false;
        emit_status(&app, "error", Some(e.to_string()));
        return Err(e.to_string());
    }
    if cfg.show_transcription_overlay {
        set_overlay_visible(&app, true);
        let _ = app.emit("dictation://partial", String::new());
    }
    emit_status(&app, "listening", None);
    if cfg.show_transcription_overlay && cfg.live_preview {
        spawn_live_preview(app.clone(), cfg.model_size, cfg.language_mode);
    }
    Ok(())
}

fn spawn_live_preview(app: AppHandle, model_size: String, language_mode: String) {
    tauri::async_runtime::spawn(async move {
        let model_path = match whisper_engine::ensure_model_downloaded(&model_size).await {
            Ok(path) => path,
            Err(_) => return,
        };
        {
            let state = app.state::<AppState>();
            if state.engine.load_model(&model_path).is_err() { return; }
        }
        let mut last_preview = String::new();
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(1400)).await;
            let samples = {
                let state = app.state::<AppState>();
                if !*state.is_listening.lock().unwrap() { break; }
                let snapshot = state.audio.lock().unwrap().snapshot();
                snapshot
            };
            if samples.len() < 16_000 { continue; }
            let app_for_blocking = app.clone();
            let lang = language_mode.clone();
            let result = tokio::task::spawn_blocking(move || {
                let state = app_for_blocking.state::<AppState>();
                state.engine.transcribe(&samples, &lang)
            }).await;
            if let Ok(Ok(result)) = result {
                let text = result.text.trim().to_string();
                if !text.is_empty() && text != last_preview {
                    last_preview = text.clone();
                    let _ = app.emit("dictation://partial", text);
                }
            }
        }
    });
}

#[tauri::command]
pub async fn stop_dictation(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut listening = state.is_listening.lock().unwrap();
        if !*listening { return Ok(()); }
        *listening = false;
    }
    let samples = state.audio.lock().unwrap().stop();
    if samples.len() < 1600 {
        set_overlay_visible(&app, false);
        emit_status(&app, "idle", None);
        return Ok(());
    }
    emit_status(&app, "processing", None);
    let cfg = state.config.lock().unwrap().clone();
    let model_path = whisper_engine::ensure_model_downloaded(&cfg.model_size).await.map_err(|e| e.to_string())?;
    state.engine.load_model(&model_path).map_err(|e| e.to_string())?;
    let language_mode = cfg.language_mode.clone();
    let transcription = {
        let app_for_blocking = app.clone();
        tokio::task::spawn_blocking(move || {
            let state = app_for_blocking.state::<AppState>();
            state.engine.transcribe(&samples, &language_mode)
        }).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?
    };
    let mut text = transcription.text;
    if textproc::is_delete_last_sentence_command(&text) {
        let _ = app.emit("dictation://delete-last-sentence", ());
        set_overlay_visible(&app, false);
        emit_status(&app, "idle", None);
        return Ok(());
    }
    text = textproc::apply_voice_commands(&text, cfg.voice_commands);
    text = textproc::apply_custom_dictionary(&text, &cfg.custom_dictionary);
    text = textproc::auto_punctuate(&text, cfg.auto_punctuation);
    if cfg.cloud_enabled && cfg.grammar_correction && !cfg.cloud_api_key.is_empty() {
        if let Ok(corrected) = textproc::cloud_grammar_correct(&text, &cfg.cloud_api_key, &transcription.language).await { text = corrected; }
    }
    inject::insert_text(&text, &cfg.insertion_mode).map_err(|e| e.to_string())?;
    {
        let db = state.db.lock().unwrap();
        let _ = history::insert(&db, &text, &transcription.language, None);
    }
    let _ = app.emit("dictation://partial", text.clone());
    let _ = app.emit("dictation://text", text);
    set_overlay_visible(&app, false);
    emit_status(&app, "idle", None);
    Ok(())
}

#[tauri::command]
pub fn search_history(state: State<AppState>, query: String) -> Result<Vec<history::HistoryEntry>, String> {
    history::search(&state.db.lock().unwrap(), &query).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_history(state: State<AppState>) -> Result<(), String> {
    history::clear(&state.db.lock().unwrap()).map_err(|e| e.to_string())
}
