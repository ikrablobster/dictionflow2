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
}
