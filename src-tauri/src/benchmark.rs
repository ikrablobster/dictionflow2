//! File-only diagnostic: no microphone, history, keyboard hook or application UI.
use crate::whisper_engine::WhisperEngine;
use std::{path::PathBuf, time::Instant};

pub fn run_if_requested() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("--benchmark") { return false; }
    // dictaflow --benchmark input.wav model.bin language output.json [seconds]
    let result = run(&args);
    let report = match result {
        Ok(report) => report,
        Err(error) => serde_json::json!({ "error": error.to_string() }),
    };
    if let Some(output) = args.get(5) {
        if std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).is_err() {
            std::process::exit(2);
        }
    } else {
        eprintln!("Usage: dictaflow --benchmark input.wav model.bin language output.json [seconds]");
    }
    if report.get("error").is_some() { std::process::exit(1); }
    true
}

fn run(args: &[String]) -> anyhow::Result<serde_json::Value> {
    anyhow::ensure!(args.len() >= 6, "Expected WAV, model path, language and output JSON");
    anyhow::ensure!(matches!(args[4].as_str(), "auto" | "ru" | "uk" | "en"), "Unsupported language");
    let mut wav = hound::WavReader::open(&args[2])?;
    let spec = wav.spec();
    anyhow::ensure!(spec.channels == 1 && spec.sample_rate == 16_000 && spec.bits_per_sample == 16 && spec.sample_format == hound::SampleFormat::Int, "Expected mono 16 kHz PCM16 WAV");
    let mut samples = wav.samples::<i16>().map(|v| v.map(|v| v as f32 / 32768.0)).collect::<Result<Vec<_>, _>>()?;
    if let Some(seconds) = args.get(6) {
        let seconds = seconds.parse::<f64>()?;
        anyhow::ensure!(seconds.is_finite() && seconds > 0.0 && seconds <= 600.0, "Invalid duration");
        samples.truncate((seconds * 16_000.0) as usize);
    }
    anyhow::ensure!(samples.len() >= 1600 && samples.len() <= 9_600_000, "Audio must be 0.1–600 seconds");
    let engine = WhisperEngine::new();
    let start = Instant::now();
    engine.load_model(&PathBuf::from(&args[3]))?;
    let load_seconds = start.elapsed().as_secs_f64();
    let mut runs = Vec::new();
    let context = match args.get(7).map(String::as_str) {
        Some("fast") => crate::whisper_engine::short_audio_context(samples.len()),
        Some(value) => value.parse::<i32>()?,
        None => 0,
    };
    for _ in 0..2 {
        let start = Instant::now();
        let result = engine.transcribe_cancellable(&samples, &args[4], None, context)?;
        runs.push(serde_json::json!({ "seconds": start.elapsed().as_secs_f64(), "text": result.text, "language": result.language }));
    }
    Ok(serde_json::json!({ "model": args[3], "audio_context": context, "audio_seconds": samples.len() as f64 / 16_000.0, "load_seconds": load_seconds, "runs": runs }))
}
