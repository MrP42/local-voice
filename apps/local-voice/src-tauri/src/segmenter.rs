//! Sentence-by-sentence transcription and injection.
//!
//! Instead of waiting for the whole dictation to finish and pasting one block, this
//! emits each spoken sentence as soon as the speaker pauses. Text appears while you
//! are still talking.
//!
//! # Why pause detection is nearly free here
//!
//! The recorder's audio callback fires *after* the Silero VAD has been applied, so
//! it only ever receives speech frames — silence is already filtered out upstream.
//! That means we do not need our own voice detector: a gap between two callbacks
//! simply *is* a pause. If no frame has arrived for `pause_ms`, the speaker stopped.
//!
//! # Why sentence-wise rather than true token streaming
//!
//! A finished sentence is stable. Token-level streaming with a batch model requires
//! re-transcribing a growing buffer and then retro-actively correcting text that has
//! already been inserted into someone's document, which is where such implementations
//! turn ugly. Here each segment is transcribed once, inserted once, and never touched
//! again.
//!
//! # The trade-off, stated plainly
//!
//! Each segment gives the model less context, and both Whisper and Parakeet use
//! context for punctuation and capitalisation. Segment mode therefore produces
//! slightly weaker punctuation than one whole-recording pass. That is why it is a
//! setting rather than a replacement.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use log::{debug, warn};
use tauri::AppHandle;

use crate::managers::transcription::TranscriptionManager;

/// 16 kHz mono is what the recorder hands us and what the engines expect.
const SAMPLE_RATE: usize = 16_000;

/// Don't emit a segment shorter than this. Guards against a stray cough or a
/// clipped syllable becoming its own "sentence".
const MIN_SEGMENT_MS: usize = 700;

/// How often the watchdog looks for a pause. Well below the pause threshold, so
/// the detection granularity is not what the user perceives.
const TICK: Duration = Duration::from_millis(100);

struct Shared {
    /// Speech frames accumulated since the last emitted segment.
    buffer: Mutex<Vec<f32>>,
    /// When the most recent speech frame arrived. `None` before the first frame.
    last_frame_at: Mutex<Option<Instant>>,
    /// Everything emitted so far this run, for the history entry.
    emitted_text: Mutex<Vec<String>>,
    /// Set while a dictation run is in progress.
    running: AtomicBool,
    /// Number of segments emitted this run; lets the caller tell whether segment
    /// mode actually produced anything.
    emitted_count: AtomicUsize,
}

/// Emits transcribed sentences as the speaker pauses.
#[derive(Clone)]
pub struct SentenceSegmenter {
    shared: Arc<Shared>,
    pause: Duration,
}

impl SentenceSegmenter {
    pub fn new(pause_ms: u64) -> Self {
        Self {
            shared: Arc::new(Shared {
                buffer: Mutex::new(Vec::new()),
                last_frame_at: Mutex::new(None),
                emitted_text: Mutex::new(Vec::new()),
                running: AtomicBool::new(false),
                emitted_count: AtomicUsize::new(0),
            }),
            pause: Duration::from_millis(pause_ms),
        }
    }

    /// True while a run is active. Cheap enough to call from the audio callback.
    #[inline]
    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Acquire)
    }

    pub fn segments_emitted(&self) -> usize {
        self.shared.emitted_count.load(Ordering::Acquire)
    }

    /// Feed one post-VAD speech frame. Called on the recorder's consumer thread, so
    /// it does nothing but append and stamp the clock.
    pub fn feed(&self, frame: &[f32]) {
        if !self.is_running() {
            return;
        }
        self.shared.buffer.lock().unwrap().extend_from_slice(frame);
        *self.shared.last_frame_at.lock().unwrap() = Some(Instant::now());
    }

    /// Begin a run and spawn the pause watchdog.
    pub fn start(&self, app: AppHandle, tm: Arc<TranscriptionManager>) {
        if self.shared.running.swap(true, Ordering::AcqRel) {
            warn!("segmenter: start called while already running");
            return;
        }
        self.shared.buffer.lock().unwrap().clear();
        self.shared.emitted_text.lock().unwrap().clear();
        self.shared.emitted_count.store(0, Ordering::Release);
        *self.shared.last_frame_at.lock().unwrap() = None;

        let shared = Arc::clone(&self.shared);
        let pause = self.pause;
        let me = self.clone();

        std::thread::spawn(move || {
            debug!("segmenter: watchdog started (pause {:?})", pause);
            while shared.running.load(Ordering::Acquire) {
                std::thread::sleep(TICK);
                if !shared.running.load(Ordering::Acquire) {
                    break;
                }
                if let Some(segment) = me.take_if_paused() {
                    me.transcribe_and_emit(segment, &app, &tm, "segment");
                }
            }
            debug!("segmenter: watchdog stopped");
        });
    }

    /// Take the buffered audio if the speaker has paused long enough and the
    /// segment is worth transcribing.
    fn take_if_paused(&self) -> Option<Vec<f32>> {
        let last = (*self.shared.last_frame_at.lock().unwrap())?;
        if last.elapsed() < self.pause {
            return None;
        }
        let mut buf = self.shared.buffer.lock().unwrap();
        if buf.len() < MIN_SEGMENT_MS * SAMPLE_RATE / 1000 {
            return None;
        }
        Some(std::mem::take(&mut *buf))
    }

    fn transcribe_and_emit(
        &self,
        segment: Vec<f32>,
        app: &AppHandle,
        tm: &Arc<TranscriptionManager>,
        kind: &str,
    ) {
        let seconds = segment.len() as f32 / SAMPLE_RATE as f32;
        let started = Instant::now();

        let text = match tm.transcribe(segment) {
            Ok(t) => t,
            Err(e) => {
                // A failed segment must not abort the dictation: the speaker is
                // very likely still talking, and the remaining audio is unaffected.
                warn!("segmenter: {kind} transcription failed: {e}");
                return;
            }
        };

        let text = text.trim().to_string();
        if text.is_empty() {
            debug!("segmenter: {kind} produced no text ({seconds:.2}s)");
            return;
        }

        debug!(
            "segmenter: {kind} {seconds:.2}s -> {:?} in {:?}",
            text,
            started.elapsed()
        );

        // Separate sentences with a space so the target field reads naturally when
        // several segments land one after another.
        let to_paste = if self.shared.emitted_count.load(Ordering::Acquire) > 0 {
            format!(" {text}")
        } else {
            text.clone()
        };

        if let Err(e) = crate::clipboard::paste(to_paste, app.clone()) {
            warn!("segmenter: pasting {kind} failed: {e}");
            // Keep the text in the run transcript even when injection failed, so the
            // history entry stays complete and nothing the user said is lost.
        }

        self.shared.emitted_text.lock().unwrap().push(text);
        self.shared.emitted_count.fetch_add(1, Ordering::AcqRel);
    }

    /// End the run, flush whatever is still buffered, and return the full text of
    /// everything emitted (for the history entry).
    pub fn finish(&self, app: &AppHandle, tm: &Arc<TranscriptionManager>) -> String {
        if !self.shared.running.swap(false, Ordering::AcqRel) {
            return String::new();
        }
        let tail = std::mem::take(&mut *self.shared.buffer.lock().unwrap());
        if tail.len() >= MIN_SEGMENT_MS * SAMPLE_RATE / 1000 {
            self.transcribe_and_emit(tail, app, tm, "tail");
        } else if !tail.is_empty() {
            debug!("segmenter: dropping {} trailing samples below minimum", tail.len());
        }
        self.shared.emitted_text.lock().unwrap().join(" ")
    }

    /// Abort the run and discard everything. Nothing is transcribed or pasted.
    pub fn cancel(&self) {
        self.shared.running.store(false, Ordering::Release);
        self.shared.buffer.lock().unwrap().clear();
        self.shared.emitted_text.lock().unwrap().clear();
        self.shared.emitted_count.store(0, Ordering::Release);
        debug!("segmenter: cancelled, buffer discarded");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ms: usize) -> Vec<f32> {
        vec![0.0; ms * SAMPLE_RATE / 1000]
    }

    #[test]
    fn ignores_frames_when_not_running() {
        let s = SentenceSegmenter::new(800);
        s.feed(&frame(1000));
        assert!(s.shared.buffer.lock().unwrap().is_empty());
    }

    #[test]
    fn does_not_emit_before_the_pause_elapses() {
        let s = SentenceSegmenter::new(800);
        s.shared.running.store(true, Ordering::Release);
        s.feed(&frame(1000));
        // The pause has not elapsed, so nothing may be taken yet.
        assert!(s.take_if_paused().is_none());
    }

    #[test]
    fn emits_after_the_pause() {
        let s = SentenceSegmenter::new(80); // short pause keeps the test fast
        s.shared.running.store(true, Ordering::Release);
        s.feed(&frame(1000));
        std::thread::sleep(Duration::from_millis(140));
        let seg = s.take_if_paused().expect("segment should be available");
        assert_eq!(seg.len(), SAMPLE_RATE); // one second of audio
        // Buffer is drained, so the same audio cannot be emitted twice.
        assert!(s.shared.buffer.lock().unwrap().is_empty());
        assert!(s.take_if_paused().is_none());
    }

    #[test]
    fn drops_segments_below_the_minimum() {
        let s = SentenceSegmenter::new(80);
        s.shared.running.store(true, Ordering::Release);
        s.feed(&frame(200)); // well under MIN_SEGMENT_MS
        std::thread::sleep(Duration::from_millis(140));
        assert!(s.take_if_paused().is_none(), "a cough must not become a sentence");
    }

    #[test]
    fn cancel_discards_everything() {
        let s = SentenceSegmenter::new(80);
        s.shared.running.store(true, Ordering::Release);
        s.feed(&frame(1000));
        s.cancel();
        assert!(!s.is_running());
        assert!(s.shared.buffer.lock().unwrap().is_empty());
        assert_eq!(s.segments_emitted(), 0);
    }
}
