//! WASAPI-Loopback-Capture des Default-Render-Endpoints (Systemton).
//!
//! Liefert 16-kHz-Mono-i16-Blöcke an einen Callback. Die puren Hilfsfunktionen
//! (`downmix_to_mono`, `f32_to_i16`) sind plattformunabhängig und getestet; der
//! eigentliche Capture-Thread ist Windows-only (Abnahme im Harness, Task 15).

/// Ziel-Samplerate der Meeting-Pipeline.
pub const TARGET_SAMPLE_RATE: usize = 16_000;

/// Pure: f32-interleaved mit beliebiger Kanalzahl -> Mono (Mittelwert je Frame).
pub fn downmix_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Pure: f32 [-1, 1] -> i16 mit Clamping (Werte außerhalb werden begrenzt).
pub fn f32_to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| {
            let scaled = if s < 0.0 { s * 32768.0 } else { s * 32767.0 };
            scaled.clamp(-32768.0, 32767.0) as i16
        })
        .collect()
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::{downmix_to_mono, f32_to_i16, TARGET_SAMPLE_RATE};
    use crate::audio_toolkit::audio::{FrameResampler, LoopbackTimeline, TimelineAction};
    use anyhow::{anyhow, Result};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::Duration;
    use wasapi::{initialize_mta, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

    /// Ausgabeblockdauer: 30 ms bei 16 kHz = 480 Samples.
    const FRAME_DURATION: Duration = Duration::from_millis(30);
    /// Stille wird in Häppchen dieser Größe in den Resampler geschoben.
    const SILENCE_CHUNK_FRAMES: usize = 4096;
    /// Wartezeit auf das WASAPI-Event; läuft sie ab, prüfen wir das Stop-Flag.
    const EVENT_TIMEOUT_MS: u32 = 200;

    /// Startet Loopback-Capture des Default-Render-Endpoints. Liefert
    /// 16-kHz-Mono-i16-Blöcke (zeitachsen-korrekt inkl. Silence-Padding) an den
    /// Callback, bis `stop()` gerufen wird.
    pub struct LoopbackCapture {
        stop: Arc<AtomicBool>,
        handle: Option<JoinHandle<()>>,
    }

    impl LoopbackCapture {
        pub fn start(on_samples: impl FnMut(&[i16]) + Send + 'static) -> Result<Self> {
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop);
            // Der COM-Init und das Öffnen des Endpoints müssen IM Thread passieren
            // (MTA gilt pro Thread); das Ergebnis kommt über diesen Kanal zurück,
            // damit start() echte Fehler melden kann statt still zu scheitern.
            let (init_tx, init_rx) = mpsc::channel::<Result<(), String>>();

            let handle = std::thread::Builder::new()
                .name("loopback-capture".to_string())
                .spawn(move || {
                    if let Err(e) = capture_loop(thread_stop, on_samples, &init_tx) {
                        log::error!("loopback capture ended with error: {e:#}");
                        // Falls der Fehler vor der Init-Meldung auftrat, hier melden.
                        let _ = init_tx.send(Err(format!("{e:#}")));
                    }
                })?;

            match init_rx.recv() {
                Ok(Ok(())) => Ok(LoopbackCapture {
                    stop,
                    handle: Some(handle),
                }),
                Ok(Err(e)) => {
                    let _ = handle.join();
                    Err(anyhow!("loopback capture failed to start: {e}"))
                }
                Err(_) => {
                    let _ = handle.join();
                    Err(anyhow!("loopback capture thread died before startup"))
                }
            }
        }

        pub fn stop(mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    impl Drop for LoopbackCapture {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn capture_loop(
        stop: Arc<AtomicBool>,
        mut on_samples: impl FnMut(&[i16]) + Send + 'static,
        init_tx: &mpsc::Sender<Result<(), String>>,
    ) -> Result<()> {
        initialize_mta()
            .ok()
            .map_err(|e| anyhow!("CoInitializeEx (MTA) failed: {e}"))?;

        let enumerator = DeviceEnumerator::new()?;
        // Loopback = Capture-Richtung auf dem RENDER-Endpoint; die wasapi-Crate
        // setzt daraus AUDCLNT_STREAMFLAGS_LOOPBACK.
        let device = enumerator.get_default_device(&Direction::Render)?;
        let mut audio_client = device.get_iaudioclient()?;

        let mix_format = audio_client.get_mixformat()?;
        let mix_rate = mix_format.get_samplespersec() as usize;
        let channels = mix_format.get_nchannels() as usize;
        // Wir verlangen f32 bei Mix-Rate/-Kanalzahl und lassen WASAPI notfalls
        // konvertieren (autoconvert), damit der Puffer immer f32-interleaved ist.
        let desired_format = WaveFormat::new(32, 32, &SampleType::Float, mix_rate, channels, None);
        let block_align = desired_format.get_blockalign() as usize;

        let (_default_period, min_period) = audio_client.get_device_period()?;
        let mode = StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: min_period,
        };
        audio_client.initialize_client(&desired_format, &Direction::Capture, &mode)?;

        let h_event = audio_client.set_get_eventhandle()?;
        let capture_client = audio_client.get_audiocaptureclient()?;
        audio_client.start_stream()?;

        // Ab hier steht der Stream — Startmeldung an start().
        let _ = init_tx.send(Ok(()));

        let mut resampler = FrameResampler::new(mix_rate, TARGET_SAMPLE_RATE, FRAME_DURATION);
        let mut timeline = LoopbackTimeline::new();
        let mut byte_buf: Vec<u8> = Vec::new();
        let silence_chunk = vec![0.0f32; SILENCE_CHUNK_FRAMES];
        let mut dropped_buffers: u64 = 0;
        let mut padded_frames: u64 = 0;

        while !stop.load(Ordering::Relaxed) {
            // Alle bereitstehenden Pakete abholen, dann aufs nächste Event warten.
            loop {
                let next_frames = capture_client.get_next_packet_size()?.unwrap_or(0) as usize;
                if next_frames == 0 {
                    break;
                }
                let needed = next_frames * block_align;
                if byte_buf.len() < needed {
                    byte_buf.resize(needed, 0);
                }
                let (frames_read, info) =
                    capture_client.read_from_device(&mut byte_buf[..needed])?;
                if frames_read == 0 {
                    break;
                }
                let valid = frames_read as usize * block_align;
                if info.flags.silent {
                    // SILENT: Inhalt ist bedeutungslos -> Nullen. Die Position
                    // zählt trotzdem normal weiter, also NICHT überspringen.
                    byte_buf[..valid].fill(0);
                }

                // SPEC C1: Die Zeitachse kommt AUSSCHLIESSLICH aus der
                // Device-Position (`BufferInfo::index`, das pu64DevicePosition
                // von IAudioCaptureClient::GetBuffer) — niemals aus gezählten
                // Buffern oder Wall-Clock-Zeit.
                let device_position = info.index;
                match timeline.on_buffer(device_position, frames_read as u64) {
                    TimelineAction::Drop => {
                        dropped_buffers += 1;
                        if dropped_buffers % 100 == 1 {
                            log::warn!(
                                "loopback: backwards device position, dropped {dropped_buffers} buffer(s)"
                            );
                        }
                        continue;
                    }
                    TimelineAction::PadSilence(gap_frames) => {
                        padded_frames += gap_frames;
                        log::debug!(
                            "loopback: gap of {gap_frames} frames padded with silence (total {padded_frames})"
                        );
                        let mut remaining = gap_frames;
                        while remaining > 0 {
                            let take = remaining.min(SILENCE_CHUNK_FRAMES as u64) as usize;
                            resampler.push(&silence_chunk[..take], |frame| {
                                on_samples(&f32_to_i16(frame))
                            });
                            remaining -= take as u64;
                        }
                    }
                    TimelineAction::Append => {}
                }

                let interleaved: Vec<f32> = byte_buf[..valid]
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                let mono = downmix_to_mono(&interleaved, channels);
                resampler.push(&mono, |frame| on_samples(&f32_to_i16(frame)));
            }

            // Timeout ist normal (kein Ton) — dann nur das Stop-Flag prüfen.
            let _ = h_event.wait_for_event(EVENT_TIMEOUT_MS);
        }

        resampler.finish(|frame| on_samples(&f32_to_i16(frame)));
        audio_client.stop_stream()?;
        log::info!(
            "loopback capture stopped (padded {padded_frames} silence frames, dropped {dropped_buffers} buffers)"
        );
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::LoopbackCapture;

#[cfg(not(target_os = "windows"))]
mod stub_impl {
    use anyhow::{anyhow, Result};

    /// Auf Nicht-Windows-Plattformen gibt es in M8 keinen Loopback-Capture.
    pub struct LoopbackCapture {
        _private: (),
    }

    impl LoopbackCapture {
        pub fn start(_on_samples: impl FnMut(&[i16]) + Send + 'static) -> Result<Self> {
            Err(anyhow!("loopback capture is windows-only in M8"))
        }

        pub fn stop(self) {}
    }
}

#[cfg(not(target_os = "windows"))]
pub use stub_impl::LoopbackCapture;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_downmix_averages_the_channels() {
        let mono = downmix_to_mono(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2);
        assert_eq!(mono, vec![0.5, 0.5, 0.0]);
    }

    #[test]
    fn i16_conversion_clamps_out_of_range() {
        let out = f32_to_i16(&[0.0, 1.0, -1.0, 2.0, -2.0]);
        assert_eq!(out, vec![0, 32767, -32768, 32767, -32768]);
    }
}
