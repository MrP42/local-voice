//! Reiner Zustands- und Entscheidungsanteil des TTS-Managers.

use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TtsPhase {
    Stopped,
    Starting,
    Ready,
    Speaking,
    Error,
}

/// Idle-Stopp nur für Server, die wir selbst gestartet haben, nur im
/// Ruhezustand `Ready`, und nur wenn eine Frist konfiguriert ist (0 = nie).
pub fn should_idle_stop(
    idle_for_secs: u64,
    idle_minutes: u32,
    owns_server: bool,
    phase: TtsPhase,
) -> bool {
    if idle_minutes == 0 || !owns_server || phase != TtsPhase::Ready {
        return false;
    }
    idle_for_secs >= u64::from(idle_minutes) * 60
}

/// Ab 120 s Startdauer bekommt die UI den Hinweis, dass vermutlich VRAM
/// fehlt (andere GPU-Apps schließen); der harte Timeout liegt bei 180 s.
pub fn start_hint_after(elapsed_secs: u64) -> Option<&'static str> {
    (elapsed_secs >= 120).then_some("vram")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_stop_only_for_owned_ready_servers_past_the_deadline() {
        assert!(should_idle_stop(16 * 60, 15, true, TtsPhase::Ready));
        assert!(
            !should_idle_stop(14 * 60, 15, true, TtsPhase::Ready),
            "noch nicht fällig"
        );
        assert!(
            !should_idle_stop(16 * 60, 15, false, TtsPhase::Ready),
            "fremde Server nie stoppen"
        );
        assert!(
            !should_idle_stop(16 * 60, 15, true, TtsPhase::Speaking),
            "nicht mitten im Sprechen"
        );
        assert!(
            !should_idle_stop(16 * 60, 15, true, TtsPhase::Starting),
            "nicht während des Starts"
        );
        assert!(
            !should_idle_stop(u64::MAX, 0, true, TtsPhase::Ready),
            "0 heißt: nie stoppen"
        );
    }

    #[test]
    fn slow_starts_earn_a_vram_hint() {
        assert_eq!(start_hint_after(60), None);
        assert_eq!(start_hint_after(120), Some("vram"));
        assert_eq!(start_hint_after(179), Some("vram"));
    }
}
