//! Playback hinter einem Trait, damit der Manager ohne Soundkarte testbar ist.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub trait Player: Send + Sync {
    /// Spielt einen kompletten WAV-Blob ab; kehrt erst nach Ende oder Abbruch
    /// zurück. `cancelled` wird von cancel()/neuen Aufträgen gesetzt.
    fn play(
        &self,
        wav: Vec<u8>,
        device: Option<String>,
        volume: f32,
        speed: f32,
        cancelled: Arc<AtomicBool>,
    ) -> Result<(), String>;
}

pub struct RodioPlayer;

impl Player for RodioPlayer {
    fn play(
        &self,
        wav: Vec<u8>,
        device: Option<String>,
        volume: f32,
        speed: f32,
        cancelled: Arc<AtomicBool>,
    ) -> Result<(), String> {
        use rodio::OutputStreamBuilder;
        // Geräteauswahl wie audio_feedback::play_audio_file (Default-Fallback).
        let stream_builder = match device {
            Some(name) if name != "Default" => {
                use cpal::traits::{DeviceTrait, HostTrait};
                let host = crate::audio_toolkit::get_cpal_host();
                let found = host
                    .output_devices()
                    .map_err(|e| e.to_string())?
                    .find(|d| d.name().map(|n| n == name).unwrap_or(false));
                match found {
                    Some(d) => OutputStreamBuilder::from_device(d).map_err(|e| e.to_string())?,
                    None => {
                        log::warn!("TTS output device '{name}' not found, using default");
                        OutputStreamBuilder::from_default_device().map_err(|e| e.to_string())?
                    }
                }
            }
            _ => OutputStreamBuilder::from_default_device().map_err(|e| e.to_string())?,
        };
        let stream = stream_builder.open_stream().map_err(|e| e.to_string())?;
        let sink =
            rodio::play(stream.mixer(), std::io::Cursor::new(wav)).map_err(|e| e.to_string())?;
        sink.set_volume(volume);
        // Tempo per Resampling; verändert die Tonhöhe mit — der zulässige
        // Bereich (0,5–2,0) ist deshalb bewusst eng gehalten.
        sink.set_speed(speed.clamp(0.5, 2.0));
        // Abbrechbar warten statt sleep_until_end: cancel() wirkt in <=50 ms.
        while !sink.empty() {
            if cancelled.load(Ordering::Acquire) {
                sink.stop();
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Ok(())
    }
}

/// Testdouble: registriert nur die Byte-Zahl des letzten Auftrags.
#[cfg(test)]
pub struct CountingPlayer(pub std::sync::Mutex<usize>);

#[cfg(test)]
impl Player for CountingPlayer {
    fn play(
        &self,
        wav: Vec<u8>,
        _device: Option<String>,
        _volume: f32,
        _speed: f32,
        _cancelled: Arc<AtomicBool>,
    ) -> Result<(), String> {
        *self.0.lock().unwrap() = wav.len();
        Ok(())
    }
}
