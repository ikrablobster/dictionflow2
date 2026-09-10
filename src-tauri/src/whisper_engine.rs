use std::path::PathBuf;
use std::sync::Mutex;

#[cfg(feature = "local-whisper")]
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct TranscriptionResult {
    pub text: String,
    pub language: String,
}

pub struct WhisperEngine {
    #[cfg(feature = "local-whisper")]
    ctx: Mutex<Option<WhisperContext>>,
    #[cfg(not(feature = "local-whisper"))]
    _placeholder: Mutex<()>,
    loaded_model: Mutex<Option<String>>,
}

/// Возвращает путь к папке с моделями Whisper внутри директории данных приложения.
pub fn models_dir() -> PathBuf {
    let dir = dirs_next::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("DictaFlow")
        .join("models");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// URL модели на Hugging Face (ggml-формат для whisper.cpp).
pub fn model_url(size: &str) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{}.bin",
        size
    )
}

pub fn model_path(size: &str) -> PathBuf {
    models_dir().join(format!("ggml-{}.bin", size))
}

/// Скачивает модель, если она ещё не загружена локально.
pub async fn ensure_model_downloaded(size: &str) -> anyhow::Result<PathBuf> {
    let path = model_path(size);
    if path.exists() {
        return Ok(path);
    }
    let url = model_url(size);
    let resp = reqwest::get(&url).await?;
    let bytes = resp.bytes().await?;
    std::fs::write(&path, &bytes)?;
    Ok(path)
}

impl WhisperEngine {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "local-whisper")]
            ctx: Mutex::new(None),
            #[cfg(not(feature = "local-whisper"))]
            _placeholder: Mutex::new(()),
            loaded_model: Mutex::new(None),
        }
    }

    #[cfg(feature = "local-whisper")]
    pub fn load_model(&self, path: &PathBuf) -> anyhow::Result<()> {
        let mut guard = self.loaded_model.lock().unwrap();
        if guard.as_deref() == path.to_str() {
            return Ok(()); // уже загружена
        }
        let ctx = WhisperContext::new_with_params(
            path.to_str().ok_or_else(|| anyhow::anyhow!("Некорректный путь к модели"))?,
            WhisperContextParameters::default(),
        )
        .map_err(|e| anyhow::anyhow!("Не удалось загрузить модель Whisper: {e:?}"))?;

        *self.ctx.lock().unwrap() = Some(ctx);
        *guard = path.to_str().map(|s| s.to_string());
        Ok(())
    }

    #[cfg(feature = "local-whisper")]
    pub fn transcribe(&self, samples: &[f32], language_mode: &str) -> anyhow::Result<TranscriptionResult> {
        let guard = self.ctx.lock().unwrap();
        let ctx = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Модель распознавания не загружена"))?;

        let mut state = ctx.create_state()?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        // auto -> whisper сам определит язык (RU/UK/EN хорошо детектируются moделью small+)
        if language_mode != "auto" {
            params.set_language(Some(language_mode));
        } else {
            params.set_language(None);
        }
        params.set_translate(false);
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_suppress_blank(true);
        params.set_token_timestamps(false);

        state.full(params, samples)?;

        let num_segments = state.full_n_segments()?;
        let mut text = String::new();
        for i in 0..num_segments {
            text.push_str(&state.full_get_segment_text(i)?);
        }

        // ВАЖНО: в whisper-rs 0.13.2 метод называется `full_lang_id_from_state`,
        // а не `full_lang_id` (которого у WhisperState вообще нет — E0599,
        // компилятор сам подсказал верное имя через "help: there is a method...").
        let detected_lang = state
            .full_lang_id_from_state()
            .ok()
            .map(|id| whisper_rs::get_lang_str(id).unwrap_or("auto").to_string())
            .unwrap_or_else(|| "auto".to_string());

        Ok(TranscriptionResult {
            text: text.trim().to_string(),
            language: detected_lang,
        })
    }

    #[cfg(not(feature = "local-whisper"))]
    pub fn load_model(&self, _path: &PathBuf) -> anyhow::Result<()> {
        Err(anyhow::anyhow!(
            "Собрано без фичи local-whisper — офлайн-распознавание недоступно в этой сборке"
        ))
    }

    #[cfg(not(feature = "local-whisper"))]
    pub fn transcribe(&self, _samples: &[f32], _language_mode: &str) -> anyhow::Result<TranscriptionResult> {
        Err(anyhow::anyhow!("Офлайн-движок отключён в этой сборке"))
    }
}
