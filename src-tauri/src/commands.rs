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
    let _ = app.emit(
        "dictation://status",
        EngineStatus {
            state: state.to_string(),
            message,
        },
    );
}

#[tauri::command]
pub fn engine_ping() -> Result<bool, String> {
    Ok(true)
}

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
pub fn capture_next_hotkey() -> Result<String, String> {
    hotkey::capture_single_press(10_000).ok_or_else(|| "Тайм-аут ожидания клавиши".to_string())
}

/// Начинает запись с микрофона. Вызывается либо кнопкой в UI, либо
/// обработчиком press у глобального хоткея (main.rs).
#[tauri::command]
pub async fn start_dictation(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut listening = state.is_listening.lock().unwrap();
        if *listening {
            return Ok(());
        }
        *listening = true;
    }

    state
        .audio
        .lock()
        .unwrap()
        .start()
        .map_err(|e| e.to_string())?;

    emit_status(&app, "listening", None);
    Ok(())
}

/// Останавливает запись, гоняет буфер через Whisper, постобработку и
/// вставляет итоговый текст в активное окно.
#[tauri::command]
pub async fn stop_dictation(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut listening = state.is_listening.lock().unwrap();
        if !*listening {
            return Ok(());
        }
        *listening = false;
    }

    let samples = state.audio.lock().unwrap().stop();
    if samples.len() < 1600 {
        // Меньше ~0.1с — считаем случайным нажатием, не гоняем модель впустую.
        emit_status(&app, "idle", None);
        return Ok(());
    }

    emit_status(&app, "processing", None);

    let cfg = state.config.lock().unwrap().clone();

    // Гарантируем, что нужная модель скачана и загружена (не блокируем основной поток).
    let model_size = cfg.model_size.clone();
    let model_path = whisper_engine::ensure_model_downloaded(&model_size)
        .await
        .map_err(|e| e.to_string())?;

    state
        .engine
        .load_model(&model_path)
        .map_err(|e| e.to_string())?;

    let language_mode = cfg.language_mode.clone();
    let transcription = {
        // whisper-rs синхронный и тяжёлый — уводим в blocking пул, чтобы не морозить UI/хоткеи.
        // Получаем состояние заново через AppHandle (Send+Clone), а не через `state`,
        // чтобы не тащить в замыкание нессылочные данные с непонятным временем жизни.
        let app_for_blocking = app.clone();
        tokio::task::spawn_blocking(move || {
            let state = app_for_blocking.state::<AppState>();
            state.engine.transcribe(&samples, &language_mode)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
    };

    let mut text = transcription.text;

    if textproc::is_delete_last_sentence_command(&text) {
        let _ = app.emit("dictation://delete-last-sentence", ());
        emit_status(&app, "idle", None);
        return Ok(());
    }

    text = textproc::apply_voice_commands(&text, cfg.voice_commands);
    text = textproc::apply_custom_dictionary(&text, &cfg.custom_dictionary);
    text = textproc::auto_punctuate(&text, cfg.auto_punctuation);

    if cfg.cloud_enabled && cfg.grammar_correction && !cfg.cloud_api_key.is_empty() {
        if let Ok(corrected) =
            textproc::cloud_grammar_correct(&text, &cfg.cloud_api_key, &transcription.language).await
        {
            text = corrected;
        }
    }

    inject::insert_text(&text, &cfg.insertion_mode).map_err(|e| e.to_string())?;

    {
        let db = state.db.lock().unwrap();
        let _ = history::insert(&db, &text, &transcription.language, None);
    }

    let _ = app.emit("dictation://text", text);
    emit_status(&app, "idle", None);
    Ok(())
}

#[tauri::command]
pub fn search_history(state: State<AppState>, query: String) -> Result<Vec<history::HistoryEntry>, String> {
    let db = state.db.lock().unwrap();
    history::search(&db, &query).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_history(state: State<AppState>) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    history::clear(&db).map_err(|e| e.to_string())
}
