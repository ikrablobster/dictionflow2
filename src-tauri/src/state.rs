use crate::audio::AudioCapture;
use crate::config::AppConfig;
use crate::whisper_engine::WhisperEngine;
use rusqlite::Connection;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub audio: Mutex<AudioCapture>,
    pub engine: WhisperEngine,
    pub db: Mutex<Connection>,
    pub is_listening: Mutex<bool>,
    pub operation: tokio::sync::Mutex<()>,
    pub session: AtomicU64,
    pub status: Mutex<crate::commands::EngineStatus>,
    pub insert_result: Mutex<bool>,
    pub insert_target: Mutex<Option<isize>>,
}

impl AppState {
    pub fn new(config: AppConfig, db: Connection) -> Self {
        Self {
            config: Mutex::new(config),
            audio: Mutex::new(AudioCapture::new()),
            engine: WhisperEngine::new(),
            db: Mutex::new(db),
            is_listening: Mutex::new(false),
            operation: tokio::sync::Mutex::new(()),
            session: AtomicU64::new(0),
            status: Mutex::new(crate::commands::EngineStatus { state: "idle".into(), message: None }),
            insert_result: Mutex::new(false),
            insert_target: Mutex::new(None),
        }
    }
}
