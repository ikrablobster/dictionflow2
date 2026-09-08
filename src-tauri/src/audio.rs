use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

/// Захват аудио с микрофона по умолчанию, ресемплинг до 16кГц моно —
/// формат, который ожидает whisper.cpp.
pub struct AudioCapture {
    buffer: Arc<Mutex<Vec<f32>>>,
    stream: Option<cpal::Stream>,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            stream: None,
        }
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        self.buffer.lock().unwrap().clear();

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("Микрофон не найден"))?;

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let buf = self.buffer.clone();

        let err_fn = |err| eprintln!("[audio] ошибка потока: {err}");

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    let mono = downmix_to_mono(data, channels);
                    let resampled = resample_linear(&mono, sample_rate, 16_000);
                    buf.lock().unwrap().extend_from_slice(&resampled);
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _| {
                    let as_f32: Vec<f32> = data.iter().map(|s| *s as f32 / i16::MAX as f32).collect();
                    let mono = downmix_to_mono(&as_f32, channels);
                    let resampled = resample_linear(&mono, sample_rate, 16_000);
                    buf.lock().unwrap().extend_from_slice(&resampled);
                },
                err_fn,
                None,
            )?,
            _ => return Err(anyhow::anyhow!("Неподдерживаемый формат аудио устройства")),
        };

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    /// Останавливает захват и возвращает записанные 16кГц моно-samples.
    pub fn stop(&mut self) -> Vec<f32> {
        self.stream = None; // Drop останавливает поток
        self.buffer.lock().unwrap().drain(..).collect()
    }
}

fn downmix_to_mono(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    data.chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (input.len() as f64 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let src_pos = i as f64 * ratio;
            let idx = src_pos as usize;
            let frac = src_pos - idx as f64;
            let a = *input.get(idx).unwrap_or(&0.0);
            let b = *input.get(idx + 1).unwrap_or(&a);
            (a as f64 + (b as f64 - a as f64) * frac) as f32
        })
        .collect()
}
