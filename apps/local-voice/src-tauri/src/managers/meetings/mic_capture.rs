//! Meeting-Mikrofonaufnahme: eigener cpal-Stream, KEIN VAD (die WAV muss
//! lückenlos sein), 16 kHz mono i16 an den Callback. Bewusst getrennt vom
//! Diktat-`AudioRecorder` (`audio_toolkit/audio/recorder.rs`): der ist
//! M3-stabilisiert und sammelt in RAM — beides wollen wir hier nicht
//! anfassen, dieser Capture-Pfad steht komplett für sich.
//!
//! Der cpal-Audio-Callback tut nur das Nötigste (Samples in einen Channel
//! schieben); Downmix, Resampling auf 16 kHz und die i16-Konvertierung laufen
//! auf einem separaten Konsumenten-Thread, damit kein teurer Schritt im
//! Echtzeit-Audio-Callback die Hardware-Puffer überlaufen lässt (das würde
//! genau die Lücken erzeugen, die die Meeting-WAV nicht haben darf).

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SizedSample};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::audio_toolkit::audio::{downmix_to_mono, f32_to_i16, CpalDeviceInfo, FrameResampler};
use crate::audio_toolkit::{get_cpal_host, list_input_devices};

/// Ziel-Samplerate der Meeting-Pipeline (wie die Loopback-Capture, Task 4).
pub const TARGET_SAMPLE_RATE: usize = 16_000;
/// Ausgabeblockdauer: 30 ms bei 16 kHz = 480 Samples.
const FRAME_DURATION: Duration = Duration::from_millis(30);

enum Msg {
    /// Rohe, interleaved f32-Samples direkt aus dem cpal-Callback (noch nicht
    /// downgemischt oder resampled — das passiert auf dem Konsumenten-Thread).
    Samples(Vec<f32>),
    /// Sentinel: der Stream wurde gestoppt, keine weiteren `Samples` folgen.
    End,
}

/// Eigenständige Mikrofonaufnahme für Meetings. Kein VAD, kein
/// RAM-Gesamtpuffer — jeder resamplete Block geht sofort per Callback an den
/// Aufrufer (der ihn z. B. streamend in eine WAV-Datei schreibt).
pub struct MeetingMicCapture {
    stream: Option<cpal::Stream>,
    consumer_handle: Option<JoinHandle<()>>,
    msg_tx: Option<mpsc::Sender<Msg>>,
    error_flag: Arc<AtomicBool>,
}

impl MeetingMicCapture {
    /// Startet die Aufnahme. `device_name` wird wie beim Diktat-Pfad
    /// (`managers/audio.rs:408-447`) per Namensabgleich aufgelöst; findet sich
    /// der Name nicht (oder ist keiner angegeben), fällt es auf das
    /// System-Standardgerät zurück. `on_samples` erhält 16-kHz-Mono-i16-Blöcke
    /// bis `stop()` gerufen wird (läuft auf dem Konsumenten-Thread, nicht im
    /// Audio-Callback).
    pub fn start(
        device_name: Option<String>,
        mut on_samples: impl FnMut(&[i16]) + Send + 'static,
    ) -> Result<Self> {
        let device = resolve_device(device_name.as_deref())?;
        let config = device
            .default_input_config()
            .map_err(|e| anyhow!("Failed to get default input config: {e}"))?;
        let sample_rate = config.sample_rate().0 as usize;
        let channels = config.channels() as usize;

        let error_flag = Arc::new(AtomicBool::new(false));
        let (msg_tx, msg_rx) = mpsc::channel::<Msg>();

        let stream = build_stream(&device, &config, msg_tx.clone(), Arc::clone(&error_flag))?;
        stream
            .play()
            .map_err(|e| anyhow!("Failed to start meeting mic stream: {e}"))?;

        let consumer_handle = std::thread::Builder::new()
            .name("meeting-mic-consumer".to_string())
            .spawn(move || {
                let mut resampler =
                    FrameResampler::new(sample_rate, TARGET_SAMPLE_RATE, FRAME_DURATION);
                while let Ok(msg) = msg_rx.recv() {
                    match msg {
                        Msg::Samples(interleaved) => {
                            let mono = downmix_to_mono(&interleaved, channels);
                            resampler.push(&mono, |frame| on_samples(&f32_to_i16(frame)));
                        }
                        Msg::End => break,
                    }
                }
                resampler.finish(|frame| on_samples(&f32_to_i16(frame)));
            })
            .map_err(|e| anyhow!("Failed to spawn meeting mic consumer thread: {e}"))?;

        Ok(Self {
            stream: Some(stream),
            consumer_handle: Some(consumer_handle),
            msg_tx: Some(msg_tx),
            error_flag,
        })
    }

    /// Stoppt die Aufnahme und wartet, bis der letzte resamplete Block den
    /// Callback erreicht hat.
    pub fn stop(mut self) {
        self.stop_inner();
    }

    /// Liefert `true`, wenn der cpal-Fehler-Callback seit dem Start gefeuert
    /// hat (z. B. Gerät wurde während der Aufnahme entfernt). Der Manager
    /// (Task 8) meldet das als `recording-error`-Event.
    pub fn had_error(&self) -> bool {
        self.error_flag.load(Ordering::Relaxed)
    }

    fn stop_inner(&mut self) {
        // Stream zuerst droppen: cpal stoppt den Audio-Client synchron, damit
        // danach garantiert keine weiteren `Msg::Samples` mehr eintrudeln.
        self.stream.take();
        if let Some(tx) = self.msg_tx.take() {
            let _ = tx.send(Msg::End);
        }
        if let Some(handle) = self.consumer_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for MeetingMicCapture {
    fn drop(&mut self) {
        self.stop_inner();
    }
}

/// Pure: entscheidet, welchen Gerätenamen wir öffnen sollen. `None` bedeutet
/// "System-Standardgerät verwenden" — sowohl wenn kein Name gewünscht ist als
/// auch wenn der gewünschte Name unter den verfügbaren Geräten nicht auftaucht
/// (z. B. abgestecktes USB-Mikro). Kein Cache nötig: Task 5 läuft einmal pro
/// Meeting-Aufnahme, nicht auf dem Keypress-Pfad wie der Diktat-Recorder.
fn resolve_device_name(requested: Option<&str>, available: &[String]) -> Option<String> {
    let name = requested?.trim();
    if name.is_empty() {
        return None;
    }
    available.iter().find(|n| n.as_str() == name).cloned()
}

fn resolve_device(device_name: Option<&str>) -> Result<cpal::Device> {
    let host = get_cpal_host();

    let devices: Vec<CpalDeviceInfo> = match list_input_devices() {
        Ok(devices) => devices,
        Err(e) => {
            log::warn!("meeting mic: failed to list input devices ({e}), using system default");
            Vec::new()
        }
    };
    let names: Vec<String> = devices.iter().map(|d| d.name.clone()).collect();

    let resolved_name = resolve_device_name(device_name, &names);
    let device = match resolved_name {
        Some(name) => devices
            .into_iter()
            .find(|d| d.name == name)
            .map(|d| d.device),
        None => None,
    };

    device
        .or_else(|| host.default_input_device())
        .ok_or_else(|| anyhow!("No input device available for meeting mic capture"))
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    msg_tx: mpsc::Sender<Msg>,
    error_flag: Arc<AtomicBool>,
) -> Result<cpal::Stream> {
    match config.sample_format() {
        cpal::SampleFormat::U8 => build_typed_stream::<u8>(device, config, msg_tx, error_flag),
        cpal::SampleFormat::I8 => build_typed_stream::<i8>(device, config, msg_tx, error_flag),
        cpal::SampleFormat::I16 => build_typed_stream::<i16>(device, config, msg_tx, error_flag),
        cpal::SampleFormat::I32 => build_typed_stream::<i32>(device, config, msg_tx, error_flag),
        cpal::SampleFormat::F32 => build_typed_stream::<f32>(device, config, msg_tx, error_flag),
        fmt => Err(anyhow!("Unsupported sample format: {fmt:?}")),
    }
}

fn build_typed_stream<T>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    msg_tx: mpsc::Sender<Msg>,
    error_flag: Arc<AtomicBool>,
) -> Result<cpal::Stream>
where
    T: Sample + SizedSample + Send + 'static,
    f32: cpal::FromSample<T>,
{
    let stream_cb = move |data: &[T], _: &cpal::InputCallbackInfo| {
        let interleaved: Vec<f32> = data.iter().map(|&s| s.to_sample::<f32>()).collect();
        // Empfänger kann während des Stoppens bereits weg sein — dann ist der
        // Stream ohnehin gleich gedroppt, ein verlorener Block ist unkritisch.
        let _ = msg_tx.send(Msg::Samples(interleaved));
    };
    let err_cb = move |err: cpal::StreamError| {
        log::error!("meeting mic capture stream error: {err}");
        error_flag.store(true, Ordering::Relaxed);
    };

    device
        .build_input_stream(&config.clone().into(), stream_cb, err_cb, None)
        .map_err(|e| anyhow!("Failed to build meeting mic input stream: {e}"))
}

#[cfg(test)]
mod tests {
    use super::resolve_device_name;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_requested_name_means_system_default() {
        assert_eq!(resolve_device_name(None, &names(&["Mic A", "Mic B"])), None);
    }

    #[test]
    fn empty_requested_name_means_system_default() {
        assert_eq!(resolve_device_name(Some(""), &names(&["Mic A"])), None);
    }

    #[test]
    fn whitespace_only_requested_name_means_system_default() {
        assert_eq!(resolve_device_name(Some("   "), &names(&["Mic A"])), None);
    }

    #[test]
    fn matching_name_is_resolved() {
        assert_eq!(
            resolve_device_name(Some("Mic B"), &names(&["Mic A", "Mic B"])),
            Some("Mic B".to_string())
        );
    }

    #[test]
    fn unknown_name_falls_back_to_system_default() {
        assert_eq!(
            resolve_device_name(Some("Unplugged USB Mic"), &names(&["Mic A", "Mic B"])),
            None
        );
    }

    #[test]
    fn requested_name_is_trimmed_before_matching() {
        assert_eq!(
            resolve_device_name(Some("  Mic A  "), &names(&["Mic A"])),
            Some("Mic A".to_string())
        );
    }
}
