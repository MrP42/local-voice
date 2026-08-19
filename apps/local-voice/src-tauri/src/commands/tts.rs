//! Tauri-Commands des Vorlesen-Bereichs (TP1).

use crate::managers::tts::{ReadingInfo, TtsManager, TtsStatus};
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[tauri::command]
#[specta::specta]
pub async fn tts_speak_text(app: AppHandle, text: String) -> Result<(), String> {
    let tts = app.state::<Arc<TtsManager>>().inner().clone();
    // speak_text sichert selbst: Cache-Offline-Pfad oder Serverstart.
    tts.speak_text(&text).await.map(|_| ())
}

#[tauri::command]
#[specta::specta]
pub async fn tts_speak_clipboard(app: AppHandle) -> Result<(), String> {
    let text = app
        .clipboard()
        .read_text()
        .map_err(|e| format!("clipboard read failed: {e}"))?;
    tts_speak_text(app, text).await
}

#[tauri::command]
#[specta::specta]
pub fn tts_cancel(app: AppHandle) -> Result<(), String> {
    app.state::<Arc<TtsManager>>().cancel();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn tts_server_start(app: AppHandle) -> Result<(), String> {
    let tts = app.state::<Arc<TtsManager>>().inner().clone();
    tts.refresh_from_settings();
    tts.ensure_server().await
}

#[tauri::command]
#[specta::specta]
pub fn tts_server_stop(app: AppHandle) -> Result<(), String> {
    app.state::<Arc<TtsManager>>().stop_server();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn tts_server_status(app: AppHandle) -> Result<TtsStatus, String> {
    Ok(app.state::<Arc<TtsManager>>().status())
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct ImportedVoice {
    pub id: String,
    pub transcript: String,
}

#[tauri::command]
#[specta::specta]
pub fn tts_list_voices(app: AppHandle) -> Result<Vec<String>, String> {
    Ok(app.state::<Arc<TtsManager>>().list_voice_ids())
}

#[tauri::command]
#[specta::specta]
pub fn tts_record_reference_start(app: AppHandle) -> Result<(), String> {
    app.state::<Arc<TtsManager>>().record_reference_start()
}

#[tauri::command]
#[specta::specta]
pub fn tts_record_reference_stop(app: AppHandle) -> Result<String, String> {
    app.state::<Arc<TtsManager>>().record_reference_stop()
}

#[tauri::command]
#[specta::specta]
pub fn tts_save_voice(app: AppHandle, name: String, transcript: String) -> Result<String, String> {
    app.state::<Arc<TtsManager>>()
        .save_pending_voice(&name, &transcript)
}

#[tauri::command]
#[specta::specta]
pub fn tts_import_voice(
    app: AppHandle,
    name: String,
    wav_path: String,
    transcript: Option<String>,
) -> Result<ImportedVoice, String> {
    let (id, transcript) = app
        .state::<Arc<TtsManager>>()
        .import_voice_file(&name, &wav_path, transcript)?;
    Ok(ImportedVoice { id, transcript })
}

#[tauri::command]
#[specta::specta]
pub fn tts_delete_voice(app: AppHandle, id: String) -> Result<(), String> {
    app.state::<Arc<TtsManager>>().delete_voice_id(&id)
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct TranslateOutcome {
    pub transcript: String,
    pub translation: String,
}

#[tauri::command]
#[specta::specta]
pub async fn tts_translate_speak(
    app: AppHandle,
    text: String,
    target_lang: String,
) -> Result<String, String> {
    let tts = app.state::<Arc<TtsManager>>().inner().clone();
    tts.translate_and_speak(&text, &target_lang).await
}

#[tauri::command]
#[specta::specta]
pub fn tts_record_translate_start(app: AppHandle) -> Result<(), String> {
    app.state::<Arc<TtsManager>>().record_translate_start()
}

#[tauri::command]
#[specta::specta]
pub async fn tts_record_translate_stop(
    app: AppHandle,
    target_lang: String,
) -> Result<TranslateOutcome, String> {
    let tts = app.state::<Arc<TtsManager>>().inner().clone();
    let (transcript, translation) = tts.record_translate_stop(&target_lang).await?;
    Ok(TranslateOutcome {
        transcript,
        translation,
    })
}

#[tauri::command]
#[specta::specta]
pub fn tts_reading_open(app: AppHandle, path: String) -> Result<ReadingInfo, String> {
    app.state::<Arc<TtsManager>>().reading_open(&path)
}

#[tauri::command]
#[specta::specta]
pub fn tts_reading_play(app: AppHandle) -> Result<(), String> {
    app.state::<Arc<TtsManager>>()
        .inner()
        .clone()
        .reading_play()
}

#[tauri::command]
#[specta::specta]
pub fn tts_reading_pause(app: AppHandle) -> Result<(), String> {
    app.state::<Arc<TtsManager>>().reading_pause();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn tts_reading_list(app: AppHandle) -> Result<Vec<ReadingInfo>, String> {
    Ok(app.state::<Arc<TtsManager>>().reading_list())
}

#[tauri::command]
#[specta::specta]
pub fn tts_reading_seek(app: AppHandle, delta: i32) -> Result<ReadingInfo, String> {
    app.state::<Arc<TtsManager>>()
        .inner()
        .clone()
        .reading_seek(delta)
}

#[tauri::command]
#[specta::specta]
pub async fn tts_speak_resume(app: AppHandle) -> Result<(), String> {
    let tts = app.state::<Arc<TtsManager>>().inner().clone();
    tts.speak_resume().await.map(|_| ())
}

#[tauri::command]
#[specta::specta]
pub fn tts_reading_reset(app: AppHandle, key: String) -> Result<(), String> {
    app.state::<Arc<TtsManager>>().reading_reset(&key)
}

#[tauri::command]
#[specta::specta]
pub fn tts_reading_remove(app: AppHandle, key: String) -> Result<(), String> {
    app.state::<Arc<TtsManager>>().reading_remove(&key)
}

#[tauri::command]
#[specta::specta]
pub fn tts_export_format(app: AppHandle) -> Result<String, String> {
    Ok(app.state::<Arc<TtsManager>>().export_format())
}

#[tauri::command]
#[specta::specta]
pub async fn tts_summarize_text(
    app: AppHandle,
    text: String,
    options: crate::summarizer::SummaryOptions,
) -> Result<String, String> {
    let settings = crate::settings::get_settings(&app);
    crate::summarizer::summarize(&settings, &text, &options).await
}

#[tauri::command]
#[specta::specta]
pub fn tts_extract_document(path: String) -> Result<String, String> {
    crate::media::extract_document_text(std::path::Path::new(&path))
}

#[tauri::command]
#[specta::specta]
pub async fn tts_extract_url(url: String) -> Result<String, String> {
    crate::media::extract_url_text(&url).await
}

#[tauri::command]
#[specta::specta]
pub fn tts_voicechange_record_start(app: AppHandle) -> Result<(), String> {
    app.state::<Arc<TtsManager>>().record_voicechange_start()
}

#[tauri::command]
#[specta::specta]
pub async fn tts_voicechange_record_stop(app: AppHandle) -> Result<String, String> {
    let tts = app.state::<Arc<TtsManager>>().inner().clone();
    tts.record_voicechange_stop().await
}

#[tauri::command]
#[specta::specta]
pub async fn tts_voicechange_file(app: AppHandle, wav_path: String) -> Result<String, String> {
    let tts = app.state::<Arc<TtsManager>>().inner().clone();
    tts.respeak_file(&wav_path).await
}

#[tauri::command]
#[specta::specta]
pub async fn tts_synthesize_to_file(
    app: AppHandle,
    text: String,
    out_path: String,
) -> Result<(), String> {
    let tts = app.state::<Arc<TtsManager>>().inner().clone();
    tts.synthesize_to_file(&text, &out_path).await.map(|_| ())
}
