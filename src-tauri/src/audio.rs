use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// Tauri managed state must be Send + Sync. The stream itself is only ever
/// replaced/dropped while AudioCapture is protected by AppState.audio Mutex;
/// audio callbacks run on CPAL's own thread and only touch Arc-backed data.
struct StreamHandle(cpal::Stream);
unsafe impl Send for StreamHandle {}
unsafe impl Sync for StreamHandle {}

#[derive(Debug, Clone, Serialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

/// Captures the selected microphone in its native sample rate and channel count.
/// Downmixing happens in the callback; resampling is intentionally delayed until
/// snapshot/stop so callback boundaries do not reset the resampler phase.
pub struct AudioCapture {
    buffer: Arc<Mutex<Vec<f32>>>,
    stream: Option<StreamHandle>,
    sample_rate: u32,
    level_bits: Arc<AtomicU32>,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            sample_rate: 16_000,
            level_bits: Arc::new(AtomicU32::new(0.0f32.to_bits())),
        }
    }

    pub fn list_input_devices() -> anyhow::Result<Vec<AudioDeviceInfo>> {
        let host = cpal::default_host();
        let default_name = host.default_input_device().and_then(|d| d.name().ok());
        let mut devices = Vec::new();

        for device in host.input_devices()? {
            if let Ok(name) = device.name() {
                let is_default = default_name.as_deref() == Some(name.as_str());
                devices.push(AudioDeviceInfo { name, is_default });
            }
        }

        devices.sort_by(|a, b| {
            b.is_default
                .cmp(&a.is_default)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        devices.dedup_by(|a, b| a.name == b.name);
        Ok(devices)
    }

    pub fn start(&mut self, selected_device: &str) -> anyhow::Result<()> {
        // If microphone test is active, restarting capture cleanly switches it
        // into a real dictation session without leaving a second audio stream.
        self.stream = None;
        self.buffer.lock().unwrap().clear();
        self.set_level(0.0);

        let host = cpal::default_host();
        let device = if selected_device.trim().is_empty() {
            host.default_input_device()
                .ok_or_else(|| anyhow::anyhow!("Микрофон по умолчанию не найден"))?
        } else {
            host.input_devices()?
                .find(|d| d.name().map(|n| n == selected_device).unwrap_or(false))
                .or_else(|| host.default_input_device())
                .ok_or_else(|| anyhow::anyhow!("Микрофон не найден: {selected_device}"))?
        };

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        self.sample_rate = sample_rate;

        let buffer = self.buffer.clone();
        let level = self.level_bits.clone();
        let err_fn = |err| eprintln!("[audio] ошибка потока: {err}");

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.clone().into(),
                move |data: &[f32], _| push_f32(data, channels, &buffer, &level),
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.clone().into(),
                move |data: &[i16], _| {
                    push_converted(data, channels, &buffer, &level, |s| {
                        s as f32 / i16::MAX as f32
                    })
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::U16 => device.build_input_stream(
                &config.clone().into(),
                move |data: &[u16], _| {
                    push_converted(data, channels, &buffer, &level, |s| {
                        (s as f32 - 32768.0) / 32768.0
                    })
                },
                err_fn,
                None,
            )?,
            other => {
                return Err(anyhow::anyhow!(
                    "Неподдерживаемый формат микрофона: {other:?}"
                ))
            }
        };

        stream.play()?;
        self.stream = Some(StreamHandle(stream));
        Ok(())
    }

    /// Current microphone activity, normalized to 0..1 for the UI meter.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level_bits.load(Ordering::Relaxed)).clamp(0.0, 1.0)
    }

    /// Returns the current recording as 16 kHz mono without stopping capture.
    /// Used for live Whisper preview in the floating transcription window.
    pub fn snapshot(&self) -> Vec<f32> {
        let raw = self.buffer.lock().unwrap().clone();
        resample_linear(&raw, self.sample_rate, 16_000)
    }

    /// Stops capture and returns 16 kHz mono samples expected by whisper.cpp.
    pub fn stop(&mut self) -> Vec<f32> {
        self.stream = None;
        self.set_level(0.0);
        let raw: Vec<f32> = self.buffer.lock().unwrap().drain(..).collect();
        resample_linear(&raw, self.sample_rate, 16_000)
    }

    fn set_level(&self, value: f32) {
        self.level_bits.store(value.to_bits(), Ordering::Relaxed);
    }
}

fn push_f32(
    data: &[f32],
    channels: usize,
    buffer: &Arc<Mutex<Vec<f32>>>,
    level: &Arc<AtomicU32>,
) {
    let mono = downmix_to_mono(data, channels);
    update_level(&mono, level);
    buffer.lock().unwrap().extend_from_slice(&mono);
}

fn push_converted<T: Copy>(
    data: &[T],
    channels: usize,
    buffer: &Arc<Mutex<Vec<f32>>>,
    level: &Arc<AtomicU32>,
    convert: impl Fn(T) -> f32,
) {
    let converted: Vec<f32> = data.iter().copied().map(convert).collect();
    push_f32(&converted, channels, buffer, level);
}

fn update_level(samples: &[f32], level: &Arc<AtomicU32>) {
    if samples.is_empty() {
        level.store(0.0f32.to_bits(), Ordering::Relaxed);
        return;
    }
    let rms = (samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt();
    // Speech RMS is usually quite low; a gentle gain makes the meter useful
    // without affecting samples sent to Whisper.
    let normalized = (rms * 8.0).clamp(0.0, 1.0);
    level.store(normalized.to_bits(), Ordering::Relaxed);
}

fn downmix_to_mono(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    data.chunks(channels)
        .filter(|frame| frame.len() == channels)
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
