//! Tauri-Commands des Vorlesen-Bereichs (TP1).

use crate::managers::tts::{TtsManager, TtsStatus};
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[tauri::command]
#[specta::specta]
pub async fn tts_speak_text(app: AppHandle, text: String) -> Result<(), String> {
    let tts = app.state::<Arc<TtsManager>>().inner().clone();
    tts.refresh_from_settings();
    tts.ensure_server().await?;
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
