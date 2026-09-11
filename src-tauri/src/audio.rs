use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

/// Captures the selected microphone in its native sample rate and channel count.
/// Downmixing happens in the callback; resampling is intentionally delayed until
/// snapshot/stop so callback boundaries do not reset the resampler phase.
struct CaptureWorker {
    buffer: Arc<Mutex<Vec<f32>>>,
    stream: Option<cpal::Stream>,
    sample_rate: u32,
    level_bits: Arc<AtomicU32>,
}

impl CaptureWorker {
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

    pub fn start(&mut self, selected_device: &str, record: bool) -> anyhow::Result<()> {
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
                move |data: &[f32], _| push_f32(data, channels, &buffer, &level, record, sample_rate as usize * 600),
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.clone().into(),
                move |data: &[i16], _| {
                    push_converted(data, channels, &buffer, &level, record, sample_rate as usize * 600, |s| {
                        s as f32 / i16::MAX as f32
                    })
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::U16 => device.build_input_stream(
                &config.clone().into(),
                move |data: &[u16], _| {
                    push_converted(data, channels, &buffer, &level, record, sample_rate as usize * 600, |s| {
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
        self.stream = Some(stream);
        Ok(())
    }

    /// Current microphone activity, normalized to 0..1 for the UI meter.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level_bits.load(Ordering::Relaxed)).clamp(0.0, 1.0)
    }

    /// Returns the current recording as 16 kHz mono without stopping capture.
    /// Used for live Whisper preview in the floating transcription window.
    pub fn snapshot(&self) -> Vec<f32> {
        let buffer = self.buffer.lock().unwrap();
        let start = buffer.len().saturating_sub(self.sample_rate as usize * 12);
        let raw = buffer[start..].to_vec();
        drop(buffer);
        resample_linear(&raw, self.sample_rate, 16_000)
    }

    /// Stops capture and returns 16 kHz mono samples expected by whisper.cpp.
    pub fn stop(&mut self) -> Vec<f32> {
        self.stream = None;
        self.set_level(0.0);
        let raw = std::mem::take(&mut *self.buffer.lock().unwrap());
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
    record: bool,
    max_samples: usize,
) {
    let mono = downmix_to_mono(data, channels);
    update_level(&mono, level);
    if record {
        let mut buffer = buffer.lock().unwrap();
        let remaining = max_samples.saturating_sub(buffer.len());
        buffer.extend_from_slice(&mono[..mono.len().min(remaining)]);
    }
}

fn push_converted<T: Copy>(
    data: &[T],
    channels: usize,
    buffer: &Arc<Mutex<Vec<f32>>>,
    level: &Arc<AtomicU32>,
    record: bool,
    max_samples: usize,
    convert: impl Fn(T) -> f32,
) {
    let converted: Vec<f32> = data.iter().copied().map(convert).collect();
    push_f32(&converted, channels, buffer, level, record, max_samples);
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
use std::sync::mpsc;

enum AudioRequest {
    Start(String, bool, mpsc::Sender<Result<(), String>>),
    Stop(mpsc::Sender<Vec<f32>>),
    Snapshot(mpsc::Sender<Vec<f32>>),
}

/// CPAL streams are created, operated and destroyed on their owning thread.
/// No unsafe Send/Sync promise is needed for platform-specific stream handles.
pub struct AudioCapture {
    sender: mpsc::Sender<AudioRequest>,
    level_bits: Arc<AtomicU32>,
}

impl AudioCapture {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        let level_bits = Arc::new(AtomicU32::new(0.0f32.to_bits()));
        let worker_level = level_bits.clone();
        std::thread::spawn(move || {
            let mut capture = CaptureWorker::new();
            capture.level_bits = worker_level;
            while let Ok(request) = receiver.recv() {
                match request {
                    AudioRequest::Start(device, record, reply) => {
                        let _ = reply.send(capture.start(&device, record).map_err(|e| e.to_string()));
                    }
                    AudioRequest::Stop(reply) => { let _ = reply.send(capture.stop()); }
                    AudioRequest::Snapshot(reply) => { let _ = reply.send(capture.snapshot()); }
                }
            }
        });
        Self { sender, level_bits }
    }
    pub fn list_input_devices() -> anyhow::Result<Vec<AudioDeviceInfo>> { CaptureWorker::list_input_devices() }
    pub fn start(&mut self, device: &str) -> anyhow::Result<()> { self.begin(device, true) }
    pub fn start_test(&mut self, device: &str) -> anyhow::Result<()> { self.begin(device, false) }
    fn begin(&self, device: &str, record: bool) -> anyhow::Result<()> {
        let (tx, rx) = mpsc::channel();
        self.sender.send(AudioRequest::Start(device.to_owned(), record, tx))?;
        rx.recv()?.map_err(anyhow::Error::msg)
    }
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level_bits.load(Ordering::Relaxed)).clamp(0.0, 1.0)
    }
    pub fn snapshot(&self) -> Vec<f32> {
        let (tx, rx) = mpsc::channel();
        if self.sender.send(AudioRequest::Snapshot(tx)).is_err() { return Vec::new(); }
        rx.recv().unwrap_or_default()
    }
    pub fn stop(&mut self) -> Vec<f32> {
        let (tx, rx) = mpsc::channel();
        if self.sender.send(AudioRequest::Stop(tx)).is_err() { return Vec::new(); }
        rx.recv().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stereo_downmix_and_resampling_preserve_duration_and_signal() {
        let stereo = vec![0.25; 48_000 * 2];
        let mono = downmix_to_mono(&stereo, 2);
        let output = resample_linear(&mono, 48_000, 16_000);
        assert_eq!(output.len(), 16_000);
        assert!(output.iter().all(|v| (*v - 0.25).abs() < 0.00001));
        assert_eq!(resample_linear(&vec![0.5; 44_100], 44_100, 16_000).len(), 16_000);
    }
    #[test]
    fn microphone_test_measures_signal_without_storing_audio() {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let level = Arc::new(AtomicU32::new(0));
        push_f32(&[0.2; 100], 1, &buffer, &level, false, 10);
        assert!(buffer.lock().unwrap().is_empty());
        assert!(f32::from_bits(level.load(Ordering::Relaxed)) > 0.0);
        push_f32(&[0.2; 100], 1, &buffer, &level, true, 10);
        push_f32(&[0.2; 100], 1, &buffer, &level, true, 10);
        assert_eq!(buffer.lock().unwrap().len(), 10);
    }
    #[test]
    fn preview_is_bounded_but_final_recording_is_complete() {
        let mut capture = CaptureWorker::new();
        capture.sample_rate = 16_000;
        *capture.buffer.lock().unwrap() = vec![0.2; 16_000 * 20];
        assert_eq!(capture.snapshot().len(), 16_000 * 12);
        assert_eq!(capture.stop().len(), 16_000 * 20);
        assert!(capture.snapshot().is_empty());
    }
}
