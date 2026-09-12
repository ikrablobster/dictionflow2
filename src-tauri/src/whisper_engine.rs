use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[cfg(feature = "local-whisper")]
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct TranscriptionResult {
    pub text: String,
    pub language: String,
}

/// Short clips retain all audio plus padding. Longer recordings use standard chunking.
pub fn short_audio_context(samples: usize) -> i32 {
    if samples > 20 * 16_000 { return 0; }
    let frames = samples.div_ceil(320);
    (frames + 64).div_ceil(64).saturating_mul(64).clamp(256, 1500) as i32
}

pub struct WhisperEngine {
    #[cfg(feature = "local-whisper")]
    ctx: Mutex<Option<whisper_rs::WhisperState>>,
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
pub async fn ensure_model_downloaded(size: &str, progress: impl Fn(String)) -> anyhow::Result<PathBuf> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    anyhow::ensure!(matches!(size, "tiny" | "base" | "small" | "medium" | "large-v3"), "Неизвестная модель");
    let path = model_path(size);
    if let Ok(mut file) = tokio::fs::File::open(&path).await {
        let mut magic = [0; 4];
        if file.read_exact(&mut magic).await.is_ok() && &magic == b"lmgg"
            && file.metadata().await?.len() > 1_000_000 {
            return Ok(path);
        }
    }
    let url = model_url(size);
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .read_timeout(std::time::Duration::from_secs(60))
        .timeout(std::time::Duration::from_secs(1800)).build()?;
    let mut resp = client.get(&url).send().await?.error_for_status()?;
    let total = resp.content_length();
    let temporary = path.with_extension("bin.part");
    let result = async {
        let mut file = tokio::fs::File::create(&temporary).await?;
        let mut received = 0u64;
        let mut last_update = std::time::Instant::now();
        while let Some(chunk) = resp.chunk().await? {
            file.write_all(&chunk).await?;
            received += chunk.len() as u64;
            if last_update.elapsed().as_millis() >= 500 {
                let message = match total {
                    Some(total) => format!("Загрузка {size}: {} / {} МБ", received / 1_000_000, total / 1_000_000),
                    None => format!("Загрузка {size}: {} МБ", received / 1_000_000),
                };
                progress(message);
                last_update = std::time::Instant::now();
            }
        }
        anyhow::ensure!(total.map_or(true, |total| received == total), "Модель скачана не полностью");
        anyhow::ensure!(received > 1_000_000, "Сервер вернул некорректную модель");
        file.sync_all().await?;
        drop(file);
        let mut file = tokio::fs::File::open(&temporary).await?;
        let mut magic = [0; 4];
        file.read_exact(&mut magic).await?;
        anyhow::ensure!(&magic == b"lmgg", "Файл не является моделью Whisper GGML");
        drop(file);
        tokio::fs::rename(&temporary, &path).await?;
        Ok::<(), anyhow::Error>(())
    }.await;
    if result.is_err() { let _ = tokio::fs::remove_file(&temporary).await; }
    result?;
    progress("Загрузка модели в память…".into());
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

        *self.ctx.lock().unwrap() = Some(ctx.create_state()?);
        *guard = path.to_str().map(|s| s.to_string());
        Ok(())
    }

    #[cfg(feature = "local-whisper")]
    pub fn transcribe(&self, samples: &[f32], language_mode: &str) -> anyhow::Result<TranscriptionResult> {
        self.transcribe_cancellable(samples, language_mode, None, 0)
    }

    #[cfg(feature = "local-whisper")]
    pub fn transcribe_cancellable(&self, samples: &[f32], language_mode: &str, cancel: Option<Arc<AtomicBool>>, audio_context: i32) -> anyhow::Result<TranscriptionResult> {
        anyhow::ensure!((0..=1500).contains(&audio_context), "Некорректное окно распознавания");
        let mut guard = self.ctx.lock().unwrap();
        let state = guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Модель распознавания не загружена"))?;

        if cancel.as_ref().is_some_and(|flag| flag.load(Ordering::Relaxed)) { anyhow::bail!("Предпросмотр отменён"); }
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        // auto -> whisper сам определит язык (RU/UK/EN хорошо детектируются moделью small+)
        if language_mode != "auto" {
            params.set_language(Some(language_mode));
        } else {
            params.set_language(None);
        }
        params.set_translate(false);
        params.set_audio_ctx(audio_context);
        params.set_no_context(true);
        params.set_no_timestamps(true);
        // Avoid repeated temperature fallback decoding of a short/noisy utterance.
        params.set_temperature_inc(0.0);
        if let Some(flag) = cancel.as_ref() {
            // The Arc stays alive until synchronous state.full returns. The C
            // callback only reads its AtomicBool, never the Whisper state.
            unsafe {
                params.set_abort_callback(Some(should_abort));
                params.set_abort_callback_user_data(Arc::as_ptr(flag) as *mut std::ffi::c_void);
            }
        }
        params.set_n_threads(std::thread::available_parallelism().map(|n| n.get().saturating_sub(1).clamp(1, 8) as i32).unwrap_or(2));
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

    #[cfg(not(feature = "local-whisper"))]
    pub fn transcribe_cancellable(&self, samples: &[f32], language_mode: &str, _cancel: Option<Arc<AtomicBool>>, _audio_context: i32) -> anyhow::Result<TranscriptionResult> {
        self.transcribe(samples, language_mode)
    }
}

#[cfg(all(test, feature = "local-whisper"))]
mod tests {
    use super::*;

    #[test]
    fn short_context_covers_the_entire_clip() {
        for seconds in [1, 3, 5, 12, 20] {
            assert!(short_audio_context(seconds * 16_000) >= (seconds * 50 + 64) as i32);
        }
        assert_eq!(short_audio_context(3 * 16_000), 256);
        assert_eq!(short_audio_context(21 * 16_000), 0);
    }

    /// Exercises the real HTTP download, GGML validation, model loader and
    /// recognizer without depending on a microphone or desktop audio routing.
    #[tokio::test]
    #[ignore = "Downloads the tiny model and the official whisper.cpp sample"]
    async fn transcribes_official_whisper_sample() {
        let path = ensure_model_downloaded("tiny", |_| {}).await.unwrap();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60)).build().unwrap();
        let bytes = client.get("https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/samples/jfk.wav")
            .send().await.unwrap().error_for_status().unwrap().bytes().await.unwrap();
        let mut wav = hound::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(wav.spec().sample_rate, 16_000);
        assert_eq!(wav.spec().channels, 1);
        let samples: Vec<f32> = wav.samples::<i16>().map(|v| v.unwrap() as f32 / 32768.0).collect();
        let engine = WhisperEngine::new();
        engine.load_model(&path).unwrap();
        let cancelled = Arc::new(AtomicBool::new(true));
        assert!(engine.transcribe_cancellable(&samples, "en", Some(cancelled), 0).is_err());
        for context in [0, short_audio_context(samples.len()), 0] {
            let result = engine.transcribe_cancellable(&samples, "en", None, context).unwrap();
            assert!(result.text.to_lowercase().contains("country"), "Unexpected transcript: {}", result.text);
            assert_eq!(result.language, "en");
        }
    }

}

#[cfg(feature = "local-whisper")]
unsafe extern "C" fn should_abort(data: *mut std::ffi::c_void) -> bool {
    !data.is_null() && (*(data as *const AtomicBool)).load(Ordering::Relaxed)
}
