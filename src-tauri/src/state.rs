use crate::audio::AudioCapture;
use crate::config::AppConfig;
use crate::whisper_engine::WhisperEngine;
use rusqlite::Connection;
use std::sync::Mutex;

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub audio: Mutex<AudioCapture>,
    pub engine: WhisperEngine,
    pub db: Mutex<Connection>,
    pub is_listening: Mutex<bool>,
}

impl AppState {
    pub fn new(config: AppConfig, db: Connection) -> Self {
        Self {
            config: Mutex::new(config),
            audio: Mutex::new(AudioCapture::new()),
            engine: WhisperEngine::new(),
            db: Mutex::new(db),
            is_listening: Mutex::new(false),
        }
    }
}
