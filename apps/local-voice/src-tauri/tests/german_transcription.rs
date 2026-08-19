//! End-to-end check that local German speech recognition actually works on this
//! machine — no network, no API key, no account.
//!
//! This is deliberately an integration test against the real model rather than a
//! mock: the whole point of the product is that transcription runs locally, and a
//! mocked STT step would prove nothing about that claim.
//!
//! The test is skipped (not failed) when the model is absent, so a fresh checkout
//! without the ~456 MB download still has a green suite. Run it with:
//!
//!   cargo test --test german_transcription -- --nocapture

use std::path::PathBuf;

use transcribe_rs::onnx::{
    parakeet::{ParakeetModel, ParakeetParams, TimestampGranularity},
    Quantization,
};

/// Where the app keeps its models on Windows.
fn model_dir() -> Option<PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    let dir = PathBuf::from(base)
        .join("de.wolffappliedai.localvoiceai")
        .join("models")
        .join("parakeet-tdt-0.6b-v3-int8");
    dir.is_dir().then_some(dir)
}

/// Read a WAV file and return 16 kHz mono f32 samples, which is what the ASR
/// front-end expects. Windows TTS emits 16-bit PCM, often at 22.05 kHz.
fn load_wav_16k_mono(path: &PathBuf) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).expect("open wav");
    let spec = reader.spec();

    let mono: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            let raw: Vec<f32> = reader
                .samples::<i32>()
                .map(|s| s.expect("sample") as f32 * scale)
                .collect();
            downmix(raw, spec.channels as usize)
        }
        hound::SampleFormat::Float => {
            let raw: Vec<f32> = reader
                .samples::<f32>()
                .map(|s| s.expect("sample"))
                .collect();
            downmix(raw, spec.channels as usize)
        }
    };

    resample_linear(&mono, spec.sample_rate as f32, 16_000.0)
}

fn downmix(samples: Vec<f32>, channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples;
    }
    samples
        .chunks(channels)
        .map(|c| c.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Linear interpolation is plenty for a speech-band test fixture; the production
/// path uses rubato.
fn resample_linear(input: &[f32], from_hz: f32, to_hz: f32) -> Vec<f32> {
    if (from_hz - to_hz).abs() < f32::EPSILON || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from_hz / to_hz;
    let out_len = (input.len() as f32 / ratio).floor() as usize;
    (0..out_len)
        .map(|i| {
            let src = i as f32 * ratio;
            let j = src.floor() as usize;
            let frac = src - j as f32;
            let a = input[j];
            let b = *input.get(j + 1).unwrap_or(&a);
            a + (b - a) * frac
        })
        .collect()
}

/// Normalize for comparison: lowercase, strip punctuation, collapse whitespace.
fn norm(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn transcribes_german_speech_locally() {
    let Some(dir) = model_dir() else {
        eprintln!("SKIP: Parakeet V3 model not installed; run the app's model download first.");
        return;
    };

    let wav = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/de_test_01.wav");
    if !wav.is_file() {
        eprintln!("SKIP: fixture {} missing", wav.display());
        return;
    }

    let audio = load_wav_16k_mono(&wav);
    assert!(
        audio.len() > 16_000,
        "fixture should be longer than one second, got {} samples",
        audio.len()
    );

    let started = std::time::Instant::now();
    let mut model = ParakeetModel::load(&dir, &Quantization::Int8).expect("load Parakeet V3 int8");
    let load_ms = started.elapsed().as_millis();

    let started = std::time::Instant::now();
    let result = model
        .transcribe_with(
            &audio,
            &ParakeetParams {
                timestamp_granularity: Some(TimestampGranularity::Segment),
                ..Default::default()
            },
        )
        .expect("transcribe");
    let infer_ms = started.elapsed().as_millis();

    let audio_secs = audio.len() as f32 / 16_000.0;
    eprintln!("--- local German transcription ---");
    eprintln!("audio      : {audio_secs:.2} s");
    eprintln!("model load : {load_ms} ms");
    eprintln!("inference  : {infer_ms} ms");
    eprintln!("text       : {}", result.text);

    let got = norm(&result.text);
    assert!(!got.is_empty(), "transcript was empty");

    // Content words from the spoken sentence. We assert on individual tokens
    // rather than the whole string because exact ASR output is not stable, and a
    // brittle full-string match would fail for reasons that have nothing to do
    // with whether local German recognition works.
    for expected in ["test", "spracherkennung", "termin", "februar"] {
        assert!(
            got.contains(expected),
            "expected {expected:?} in transcript, got: {got:?}"
        );
    }
}
