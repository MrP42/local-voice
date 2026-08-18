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

/// Non-Streaming-WAV-Request. Ohne Referenzstimme hält der feste Seed die
/// Zufallsstimme zwischen Aufträgen stabil; mit Stimme wählt `reference_id`
/// die geklonte Stimme und `use_memory_cache` lässt den Server das
/// Referenz-Encoding zwischen Requests wiederverwenden.
pub fn tts_request_body(text: &str, seed: i64, reference_id: Option<&str>) -> serde_json::Value {
    let mut body = serde_json::json!({
        "text": text,
        "format": "wav",
        "seed": seed,
        "streaming": false,
    });
    if let Some(voice) = reference_id {
        body["reference_id"] = serde_json::json!(voice);
        body["use_memory_cache"] = serde_json::json!("on");
    }
    body
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
        let b = tts_request_body("Hallo", 42, None);
        assert_eq!(b["text"], "Hallo");
        assert_eq!(b["format"], "wav");
        assert_eq!(b["seed"], 42);
        assert_eq!(b["streaming"], false);
        assert!(
            b.get("reference_id").is_none(),
            "ohne Stimme kein reference_id-Feld"
        );
        assert!(b.get("use_memory_cache").is_none());
    }

    #[test]
    fn request_body_carries_the_voice_and_enables_the_reference_cache() {
        let b = tts_request_body("Hallo", 42, Some("patrick"));
        assert_eq!(b["reference_id"], "patrick");
        assert_eq!(b["use_memory_cache"], "on");
        assert_eq!(b["seed"], 42, "Seed bleibt für deterministisches Sampling");
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
