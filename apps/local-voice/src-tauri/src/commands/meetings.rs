//! Thin command shell over `MeetingRecorderManager` and `MeetingStore`
//! (pattern: `commands/history.rs`). No logic beyond argument shuffling and
//! error mapping lives here.

use std::sync::Arc;

use tauri::State;

use crate::managers::meetings::recorder::MeetingRecorderManager;
use crate::managers::meetings::store::{Meeting, MeetingDocument, MeetingStore, StoredSegment};

/// Starting touches audio hardware and can block for seconds (loopback
/// start-up), hence `spawn_blocking` rather than running on the command task.
#[tauri::command]
#[specta::specta]
pub async fn meetings_start(
    recorder: State<'_, Arc<MeetingRecorderManager>>,
    title: String,
    consent_confirmed: bool,
    capture_system: bool,
) -> Result<Meeting, String> {
    let recorder = Arc::clone(&recorder);
    tauri::async_runtime::spawn_blocking(move || {
        recorder.start(title, consent_confirmed, capture_system)
    })
    .await
    .map_err(|e| format!("meetings_start panicked: {e}"))?
}

#[tauri::command]
#[specta::specta]
pub async fn meetings_pause(
    recorder: State<'_, Arc<MeetingRecorderManager>>,
) -> Result<(), String> {
    recorder.pause()
}

#[tauri::command]
#[specta::specta]
pub async fn meetings_resume(
    recorder: State<'_, Arc<MeetingRecorderManager>>,
) -> Result<(), String> {
    recorder.resume()
}

/// Stopping waits for the tail chunks to finish transcribing, so it must not
/// occupy the async runtime's worker.
#[tauri::command]
#[specta::specta]
pub async fn meetings_stop(
    recorder: State<'_, Arc<MeetingRecorderManager>>,
) -> Result<String, String> {
    let recorder = Arc::clone(&recorder);
    tauri::async_runtime::spawn_blocking(move || recorder.stop())
        .await
        .map_err(|e| format!("meetings_stop panicked: {e}"))?
}

#[tauri::command]
#[specta::specta]
pub async fn meetings_is_recording(
    recorder: State<'_, Arc<MeetingRecorderManager>>,
) -> Result<bool, String> {
    Ok(recorder.is_recording())
}

#[tauri::command]
#[specta::specta]
pub async fn meetings_list(
    store: State<'_, Arc<MeetingStore>>,
    offset: u32,
    limit: u32,
) -> Result<Vec<Meeting>, String> {
    store
        .list_meetings(offset, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn meetings_get_segments(
    store: State<'_, Arc<MeetingStore>>,
    meeting_id: String,
) -> Result<Vec<StoredSegment>, String> {
    store.get_segments(&meeting_id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn meetings_update_segment(
    store: State<'_, Arc<MeetingStore>>,
    meeting_id: String,
    segment_index: u32,
    text: String,
) -> Result<(), String> {
    store
        .update_segment_text(&meeting_id, segment_index, &text)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn meetings_get_documents(
    store: State<'_, Arc<MeetingStore>>,
    meeting_id: String,
) -> Result<Vec<MeetingDocument>, String> {
    store.get_documents(&meeting_id).map_err(|e| e.to_string())
}

/// Soft-deletes the meeting. The returned audio paths are dropped for now —
/// deleting the files from disk arrives with the retention work (Task 12).
#[tauri::command]
#[specta::specta]
pub async fn meetings_delete(
    store: State<'_, Arc<MeetingStore>>,
    meeting_id: String,
) -> Result<(), String> {
    store
        .soft_delete_meeting(&meeting_id)
        .map(|_paths| ())
        .map_err(|e| e.to_string())
}
