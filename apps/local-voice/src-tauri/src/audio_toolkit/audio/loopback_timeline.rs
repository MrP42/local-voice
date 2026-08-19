/// Führt die Zeitachse des Loopback-Streams anhand der Device-Position (Frames
/// seit Stream-Start, aus dem WASAPI-Capture-Client), NIE anhand gezählter Buffer.
pub struct LoopbackTimeline {
    expected_next_frame: Option<u64>,
}

#[derive(Debug)]
pub enum TimelineAction {
    /// So viele Silence-Frames VOR dem Buffer einfügen (Lücke durch Stille).
    PadSilence(u64),
    /// Buffer direkt anhängen.
    Append,
    /// Buffer verwerfen (Positionssprung rückwärts — Gerätewechsel o. ä.); Aufrufer loggt.
    Drop,
}

impl LoopbackTimeline {
    pub fn new() -> Self {
        LoopbackTimeline {
            expected_next_frame: None,
        }
    }

    pub fn on_buffer(&mut self, device_position_frames: u64, buffer_frames: u64) -> TimelineAction {
        match self.expected_next_frame {
            None => {
                // First buffer: define time zero
                self.expected_next_frame = Some(device_position_frames + buffer_frames);
                TimelineAction::Append
            }
            Some(expected) => {
                if device_position_frames == expected {
                    // Contiguous: append and update expected
                    self.expected_next_frame = Some(device_position_frames + buffer_frames);
                    TimelineAction::Append
                } else if device_position_frames > expected {
                    // Gap: pad silence
                    let silence_frames = device_position_frames - expected;
                    self.expected_next_frame = Some(device_position_frames + buffer_frames);
                    TimelineAction::PadSilence(silence_frames)
                } else {
                    // Backwards jump: drop buffer, don't update expected
                    TimelineAction::Drop
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contiguous_buffers_append_without_padding() {
        let mut t = LoopbackTimeline::new();
        assert!(matches!(t.on_buffer(0, 480), TimelineAction::Append));
        assert!(matches!(t.on_buffer(480, 480), TimelineAction::Append));
    }

    #[test]
    fn a_silence_gap_is_padded_not_compressed() {
        let mut t = LoopbackTimeline::new();
        t.on_buffer(0, 480);
        // 3 Sekunden Stille bei 48 kHz: Position springt um 144_000 Frames
        match t.on_buffer(480 + 144_000, 480) {
            TimelineAction::PadSilence(n) => assert_eq!(n, 144_000),
            other => panic!("erwartet PadSilence, bekam {other:?}"),
        }
    }

    #[test]
    fn the_first_buffer_defines_time_zero_even_at_nonzero_position() {
        // Stream lief schon, bevor wir zuhören: erste Position != 0 erzeugt KEIN Padding.
        let mut t = LoopbackTimeline::new();
        assert!(matches!(t.on_buffer(96_000, 480), TimelineAction::Append));
        assert!(matches!(t.on_buffer(96_480, 480), TimelineAction::Append));
    }

    #[test]
    fn backwards_position_jumps_drop_the_buffer() {
        let mut t = LoopbackTimeline::new();
        t.on_buffer(10_000, 480);
        assert!(matches!(t.on_buffer(5_000, 480), TimelineAction::Drop));
    }
}
