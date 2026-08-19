//! M8 meetings: file import. Two paths that share only the meeting shell:
//!
//! - VTT/SRT: parsed directly into segments (`subtitle::parse_subtitles`),
//!   no transcription, meeting goes straight to `ready`.
//! - Everything else (`media::MEDIA_EXTENSIONS`): decoded to WAV via
//!   `media::ensure_wav`, then run through the *same* chunk -> transcribe ->
//!   append_delta pipeline the live recorder uses (`recorder.rs`), except as
//!   one mixed channel instead of separate mic/system channels — there is
//!   only one audio track to import.
//!
//! Errors on the audio path always land the meeting on `failed` with an
//! `Error` event; nothing is silently dropped (recording an import that
//! looked like it worked but has no segments would be worse than an obvious
//! failure).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use log::{error, info};
use tauri_specta::Event;

use super::chunker::ChannelChunker;
use super::recorder::MeetingEvent;
use super::store::{MeetingSource, MeetingStatus, MeetingStore, StoredSegment, TranscriptDelta};
use super::subtitle::parse_subtitles;
use crate::managers::transcription::TranscriptionManager;
use crate::media;

/// `StoredSegment::channel` for a single imported track — there is no
/// separate mic/system split for an imported file (mirrors the doc comment
/// on `StoredSegment::channel`: 2 = MixedCapture).
const CHANNEL_MIXED: u8 = 2;
/// Import runs off the UI thread already (`spawn_blocking`), so nobody is
/// waiting on any one chunk the way the live pipeline's user is; a coarser
/// block keeps the segment/model-call overhead down. `Segments` events after
/// each chunk are still frequent enough to serve as progress feedback.
const IMPORT_CHUNK_TARGET_MS: u64 = 60_000;
const SUBTITLE_EXTENSIONS: [&str; 2] = ["vtt", "srt"];

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Import")
        .to_string()
}

/// Imports `path` as a new meeting — VTT/SRT directly, audio/video through
/// the chunked transcription pipeline. Returns the new meeting's id.
pub async fn import_media_file(
    app: &tauri::AppHandle,
    store: Arc<MeetingStore>,
    tm: Arc<TranscriptionManager>,
    path: PathBuf,
    consent_confirmed: bool,
) -> Result<String, String> {
    let consent_confirmed_at = consent_confirmed.then(|| chrono::Utc::now().timestamp());
    let title = title_from_path(&path);

    if SUBTITLE_EXTENSIONS.contains(&extension_of(&path).as_str()) {
        return import_subtitle_file(&store, &title, &path, consent_confirmed_at);
    }

    import_audio_file(app, store, tm, title, path, consent_confirmed_at).await
}

/// Subtitle path: synchronous — no transcription, so no reason to leave the
/// command task.
fn import_subtitle_file(
    store: &Arc<MeetingStore>,
    title: &str,
    path: &Path,
    consent_confirmed_at: Option<i64>,
) -> Result<String, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("Untertiteldatei nicht lesbar: {e}"))?;
    let segments = parse_subtitles(&content)?;

    let meeting = store
        .create_meeting(title, MeetingSource::Subtitle, consent_confirmed_at)
        .map_err(|e| format!("meeting_create_failed: {e}"))?;

    store
        .append_delta(
            &meeting.id,
            &TranscriptDelta {
                new_segments: segments,
            },
        )
        .map_err(|e| format!("segments_store_failed: {e}"))?;
    store
        .set_status(&meeting.id, MeetingStatus::Ready)
        .map_err(|e| format!("status_ready_failed: {e}"))?;

    info!("meetings: subtitle import ready ({})", meeting.id);
    Ok(meeting.id)
}

/// Audio/video path: create the (already `processing`) meeting row up front
/// so the caller has an id to show immediately, then do the actual decode +
/// transcribe work off the async runtime.
async fn import_audio_file(
    app: &tauri::AppHandle,
    store: Arc<MeetingStore>,
    tm: Arc<TranscriptionManager>,
    title: String,
    path: PathBuf,
    consent_confirmed_at: Option<i64>,
) -> Result<String, String> {
    let meeting = store
        .create_meeting(&title, MeetingSource::Import, consent_confirmed_at)
        .map_err(|e| format!("meeting_create_failed: {e}"))?;
    let meeting_id = meeting.id.clone();
    emit_state(app, &meeting_id, "processing");

    let app_owned = app.clone();
    let status_store = Arc::clone(&store);
    let blocking_meeting_id = meeting_id.clone();
    let join_result = tauri::async_runtime::spawn_blocking(move || {
        run_import(&app_owned, &store, &tm, &blocking_meeting_id, &path)
    })
    .await;

    match join_result {
        Ok(Ok(())) => Ok(meeting_id),
        Ok(Err(e)) => Err(e),
        Err(join_err) => {
            let _ = status_store.set_status(&meeting_id, MeetingStatus::Failed);
            emit_error(app, &meeting_id, "import_panicked");
            Err(format!("meetings_import panicked: {join_err}"))
        }
    }
}

/// The blocking body: decode, copy the WAV, transcribe in chunks, finish with
/// `ready` or `failed` — always one or the other, plus the matching event.
fn run_import(
    app: &tauri::AppHandle,
    store: &Arc<MeetingStore>,
    tm: &Arc<TranscriptionManager>,
    meeting_id: &str,
    path: &Path,
) -> Result<(), String> {
    let outcome = (|| -> Result<u64, String> {
        let (wav_path, _tmp_guard) = media::ensure_wav(path, 16_000)?;
        let samples = read_wav_i16_mono_16k(&wav_path)?;

        let dir = crate::portable::app_data_dir(app)
            .map_err(|e| format!("app_data_dir_failed: {e}"))?
            .join("meetings")
            .join(meeting_id);
        std::fs::create_dir_all(&dir).map_err(|e| format!("meeting_dir_failed: {e}"))?;
        let import_wav_path = dir.join("import.wav");
        std::fs::copy(&wav_path, &import_wav_path)
            .map_err(|e| format!("import_wav_copy_failed: {e}"))?;

        transcribe_and_store(app, store, tm, meeting_id, &samples)?;

        let duration_ms = (samples.len() as u64 * 1_000) / 16_000;
        store
            .set_audio_paths(meeting_id, import_wav_path.to_str(), None, Some(duration_ms))
            .map_err(|e| format!("audio_paths_failed: {e}"))?;
        Ok(duration_ms)
    })();

    match outcome {
        Ok(duration_ms) => {
            store
                .set_status(meeting_id, MeetingStatus::Ready)
                .map_err(|e| format!("status_ready_failed: {e}"))?;
            emit_state(app, meeting_id, "ready");
            info!("meetings: import ready ({meeting_id}, {duration_ms} ms)");
            Ok(())
        }
        Err(e) => {
            error!("meetings: import failed ({meeting_id}): {e}");
            let _ = store.set_status(meeting_id, MeetingStatus::Failed);
            emit_error(app, meeting_id, "import_failed");
            Err(e)
        }
    }
}

/// Chunk the whole file at once (`push` the full buffer, then keep draining
/// with `push(&[])` — the chunker only ever cuts once per call — followed by
/// `flush` for the tail below the target length), transcribing and storing
/// each chunk as it is cut, same as the live worker in `recorder.rs`.
fn transcribe_and_store(
    app: &tauri::AppHandle,
    store: &Arc<MeetingStore>,
    tm: &Arc<TranscriptionManager>,
    meeting_id: &str,
    samples: &[i16],
) -> Result<(), String> {
    let mut chunker = ChannelChunker::new(IMPORT_CHUNK_TARGET_MS);
    let mut chunks = Vec::new();
    if let Some(c) = chunker.push(samples) {
        chunks.push(c);
    }
    while let Some(c) = chunker.push(&[]) {
        chunks.push(c);
    }
    if let Some(c) = chunker.flush() {
        chunks.push(c);
    }

    let mut next_index: u32 = 0;
    for chunk in chunks {
        let offset_ms = chunk.offset_ms;
        let timed = tm
            .transcribe_segments(chunk.samples)
            .map_err(|e| format!("transcription_failed: {e}"))?;

        let appended: Vec<StoredSegment> = timed
            .into_iter()
            .filter(|s| !s.text.trim().is_empty())
            .map(|s| {
                let segment = StoredSegment {
                    segment_index: next_index,
                    text: s.text,
                    start_ms: offset_ms + s.start_ms,
                    end_ms: offset_ms + s.end_ms,
                    channel: CHANNEL_MIXED,
                    speaker_index: None,
                };
                next_index += 1;
                segment
            })
            .collect();
        if appended.is_empty() {
            continue;
        }

        store
            .append_delta(
                meeting_id,
                &TranscriptDelta {
                    new_segments: appended.clone(),
                },
            )
            .map_err(|e| format!("delta_store_failed: {e}"))?;
        let _ = (MeetingEvent::Segments {
            meeting_id: meeting_id.to_string(),
            appended,
        })
        .emit(app);
    }
    Ok(())
}

/// Reads a WAV file as 16 kHz mono i16 PCM, downmixing/resampling as needed.
/// `ensure_wav` already guarantees this for anything it transcodes via
/// ffmpeg, but a `.wav` input passes straight through unchanged, so this
/// stays tolerant of arbitrary channel counts and sample rates (same
/// downmix/linear-resample approach as
/// `managers::tts::voices::load_wav_mono_16k`, i16 output instead of f32).
fn read_wav_i16_mono_16k(path: &Path) -> Result<Vec<i16>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("WAV nicht lesbar: {e}"))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;

    let interleaved: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, _) => reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?,
        (hound::SampleFormat::Int, bits) => {
            let scale = (1i64 << (bits - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?
        }
    };

    let mono: Vec<f32> = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();

    let resampled = if spec.sample_rate == 16_000 || mono.is_empty() {
        mono
    } else {
        let ratio = spec.sample_rate as f64 / 16_000.0;
        let out_len = ((mono.len() as f64) / ratio).floor() as usize;
        let mut out = Vec::with_capacity(out_len);
        for i in 0..out_len {
            let pos = i as f64 * ratio;
            let idx = pos.floor() as usize;
            let frac = (pos - idx as f64) as f32;
            let a = mono[idx.min(mono.len() - 1)];
            let b = mono[(idx + 1).min(mono.len() - 1)];
            out.push(a + (b - a) * frac);
        }
        out
    };

    Ok(resampled
        .into_iter()
        .map(|v| (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect())
}

fn emit_state(app: &tauri::AppHandle, meeting_id: &str, status: &str) {
    let _ = (MeetingEvent::State {
        meeting_id: meeting_id.to_string(),
        status: status.to_string(),
        paused: false,
    })
    .emit(app);
}

fn emit_error(app: &tauri::AppHandle, meeting_id: &str, message: &str) {
    let _ = (MeetingEvent::Error {
        meeting_id: meeting_id.to_string(),
        message: message.to_string(),
    })
    .emit(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_from_path_uses_the_file_stem() {
        assert_eq!(title_from_path(Path::new("C:/rec/Jour Fixe.mp3")), "Jour Fixe");
        assert_eq!(title_from_path(Path::new("C:/rec/notes.vtt")), "notes");
    }

    #[test]
    fn title_from_path_falls_back_when_there_is_no_usable_stem() {
        assert_eq!(title_from_path(Path::new("C:/rec/   .mp3")), "Import");
    }

    #[test]
    fn subtitle_extensions_are_recognized_case_insensitively() {
        assert!(SUBTITLE_EXTENSIONS.contains(&extension_of(Path::new("a.VTT")).as_str()));
        assert!(SUBTITLE_EXTENSIONS.contains(&extension_of(Path::new("a.srt")).as_str()));
        assert!(!SUBTITLE_EXTENSIONS.contains(&extension_of(Path::new("a.mp3")).as_str()));
    }
}
