//! Playback hinter einem Trait, damit der Manager ohne Soundkarte testbar ist.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// Tempo und Lautstärke, die eine LAUFENDE Wiedergabe mitliest.
///
/// Beide Werte wurden früher einmal beim Start eines Satzes gelesen und der
/// Wiedergabe mitgegeben. Wer während des Vorlesens am Tempo drehte, hörte
/// die Änderung deshalb erst beim nächsten Satz — bei einem langen Absatz
/// also gefühlt gar nicht. Genau dann will man aber stellen: man merkt beim
/// Hören, dass es zu langsam ist, nicht vorher.
///
/// Als Atomics statt hinter einem Mutex: der Wiedergabe-Thread liest sie
/// zwanzigmal pro Sekunde, und ein Regler darf nie auf ein Schloss warten.
/// `f32` liegt dabei als Bitmuster in einem `u32` — `AtomicF32` gibt es in
/// der Standardbibliothek nicht.
#[derive(Debug)]
pub struct PlaybackControls {
    volume_bits: AtomicU32,
    speed_bits: AtomicU32,
}

impl Default for PlaybackControls {
    fn default() -> Self {
        Self {
            volume_bits: AtomicU32::new(1.0f32.to_bits()),
            speed_bits: AtomicU32::new(1.0f32.to_bits()),
        }
    }
}

impl PlaybackControls {
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    pub fn speed(&self) -> f32 {
        f32::from_bits(self.speed_bits.load(Ordering::Relaxed))
    }

    pub fn set_volume(&self, value: f32) {
        self.volume_bits
            .store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Tempo entsteht per Resampling und zieht die Tonhöhe mit — der
    /// zulässige Bereich ist deshalb bewusst eng.
    pub fn set_speed(&self, value: f32) {
        self.speed_bits
            .store(value.clamp(0.5, 2.0).to_bits(), Ordering::Relaxed);
    }
}

pub trait Player: Send + Sync {
    /// Spielt einen kompletten WAV-Blob ab; kehrt erst nach Ende oder Abbruch
    /// zurück. `cancelled` wird von cancel()/neuen Aufträgen gesetzt.
    ///
    /// `gain` ist der Pegelausgleich DIESES Satzes (siehe `playback_gain`) und
    /// steht fest; `controls` trägt die Nutzerwerte und darf sich währenddessen
    /// ändern.
    fn play(
        &self,
        wav: Vec<u8>,
        device: Option<String>,
        gain: f32,
        controls: Arc<PlaybackControls>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<(), String>;
}

pub struct RodioPlayer;

impl Player for RodioPlayer {
    fn play(
        &self,
        wav: Vec<u8>,
        device: Option<String>,
        gain: f32,
        controls: Arc<PlaybackControls>,
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

        let mut applied_volume = controls.volume() * gain;
        let mut applied_speed = controls.speed();
        sink.set_volume(applied_volume);
        sink.set_speed(applied_speed);

        // Abbrechbar warten statt sleep_until_end: cancel() wirkt in <=50 ms.
        // Dieselbe Schleife übernimmt Regleränderungen — damit wirkt ein Dreh
        // am Tempo mitten im Satz und nicht erst beim nächsten.
        while !sink.empty() {
            if cancelled.load(Ordering::Acquire) {
                sink.stop();
                break;
            }
            let wanted_volume = controls.volume() * gain;
            if (wanted_volume - applied_volume).abs() > f32::EPSILON {
                sink.set_volume(wanted_volume);
                applied_volume = wanted_volume;
            }
            let wanted_speed = controls.speed();
            if (wanted_speed - applied_speed).abs() > f32::EPSILON {
                sink.set_speed(wanted_speed);
                applied_speed = wanted_speed;
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
        _gain: f32,
        _controls: Arc<PlaybackControls>,
        _cancelled: Arc<AtomicBool>,
    ) -> Result<(), String> {
        *self.0.lock().unwrap() = wav.len();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn die_regler_halten_sich_an_ihre_grenzen() {
        let c = PlaybackControls::default();
        c.set_speed(5.0);
        assert_eq!(c.speed(), 2.0);
        c.set_speed(0.1);
        assert_eq!(c.speed(), 0.5);
        c.set_volume(3.0);
        assert_eq!(c.volume(), 1.0);
        c.set_volume(-1.0);
        assert_eq!(c.volume(), 0.0);
    }

    /// Der Zweck der Atomics: ein anderer Thread aendert den Wert, und die
    /// Wiedergabe sieht ihn beim naechsten Blick — ohne Schloss, ohne Warten.
    #[test]
    fn eine_aenderung_ist_sofort_von_aussen_sichtbar() {
        let c = Arc::new(PlaybackControls::default());
        let writer = Arc::clone(&c);
        std::thread::spawn(move || writer.set_speed(1.75))
            .join()
            .unwrap();
        assert_eq!(c.speed(), 1.75);
    }
}
