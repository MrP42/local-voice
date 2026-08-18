//! Pure Bausteine des Fish-Speech-HTTP-Protokolls: URL, Request-Körper,
//! Text-Vorbereitung und WAV-Plausibilitätsprüfung. Bewusst ohne I/O,
//! damit jede Regel ohne Server testbar ist.

pub fn base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

pub struct PreparedText {
    pub text: String,
    pub truncated: bool,
}

/// Leer/Whitespace → None (kein Serverstart für nichts). Längenkappung in
/// Zeichen, damit kein UTF-8-Zeichen zerschnitten wird.
pub fn prepare_text(raw: &str, max_chars: u32) -> Option<PreparedText> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let max = max_chars as usize;
    let count = trimmed.chars().count();
    if count <= max {
        return Some(PreparedText {
            text: trimmed.to_string(),
            truncated: false,
        });
    }
    Some(PreparedText {
        text: trimmed.chars().take(max).collect(),
        truncated: true,
    })
}

/// Non-Streaming-WAV-Request; Seed fest gesetzt, damit die Stimme zwischen
/// Aufträgen stabil bleibt, bis TP2 echte Referenzstimmen bringt.
pub fn tts_request_body(text: &str, seed: i64) -> serde_json::Value {
    serde_json::json!({
        "text": text,
        "format": "wav",
        "seed": seed,
        "streaming": false,
    })
}

/// RIFF-Magic plus nennenswerte Nutzlast (>1 KiB): filtert HTML-Fehlerseiten
/// und leere Antworten, ohne einen vollen WAV-Parser zu brauchen.
pub fn looks_like_wav(bytes: &[u8]) -> bool {
    bytes.len() > 1024 && bytes.starts_with(b"RIFF")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_is_always_loopback() {
        assert_eq!(base_url(8080), "http://127.0.0.1:8080");
        assert_eq!(base_url(9000), "http://127.0.0.1:9000");
    }

    #[test]
    fn empty_or_whitespace_text_is_rejected() {
        assert!(prepare_text("", 100).is_none());
        assert!(prepare_text("   \n\t", 100).is_none());
    }

    #[test]
    fn overlong_text_is_truncated_at_a_char_boundary() {
        // 'ä' ist 2 Bytes; die Grenze zählt Zeichen, nicht Bytes.
        let p = prepare_text("ääääää", 4).unwrap();
        assert_eq!(p.text, "ääää");
        assert!(p.truncated);
        let ok = prepare_text("kurz", 100).unwrap();
        assert_eq!(ok.text, "kurz");
        assert!(!ok.truncated);
    }

    #[test]
    fn request_body_pins_wav_and_seed_and_disables_streaming() {
        let b = tts_request_body("Hallo", 42);
        assert_eq!(b["text"], "Hallo");
        assert_eq!(b["format"], "wav");
        assert_eq!(b["seed"], 42);
        assert_eq!(b["streaming"], false);
    }

    #[test]
    fn wav_check_wants_riff_and_some_payload() {
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&[0u8; 2000]);
        assert!(looks_like_wav(&wav));
        assert!(!looks_like_wav(b"RIFF")); // nur Header, kein Audio
        assert!(!looks_like_wav(b"<html>error</html>xxxxxxxxxxxxxxxx"));
    }
}
