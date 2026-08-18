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

/// Zerlegt Text an Satzenden für die Sprech-Pipeline: Satz 1 wird abgespielt,
/// während Satz 2 schon synthetisiert wird — die gefühlte Latenz ist damit die
/// Synthese des ERSTEN Satzes, nicht des ganzen Textes.
///
/// Ein Schnitt passiert nur an `.!?…` vor Whitespace UND wenn das bisherige
/// Stück mindestens 15 Zeichen hat — das lässt deutsche Abkürzungen
/// („z. B.", „Dr.") zusammen, statt sie als Mini-Sätze vorzulesen.
pub fn split_sentences(text: &str) -> Vec<String> {
    const MIN_CHUNK_CHARS: usize = 15;
    let mut sentences = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        current.push(c);
        let is_end = matches!(c, '.' | '!' | '?' | '…');
        let next_is_boundary = chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        if is_end && next_is_boundary && current.trim().chars().count() >= MIN_CHUNK_CHARS {
            sentences.push(current.trim().to_string());
            current.clear();
        }
    }
    let tail = current.trim().to_string();
    if !tail.is_empty() {
        sentences.push(tail);
    }
    sentences
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
    fn sentences_split_at_real_boundaries() {
        let text = "Hallo Patrick, schön dich zu hören. Wie geht es dir heute? Alles klar!";
        assert_eq!(
            split_sentences(text),
            vec![
                "Hallo Patrick, schön dich zu hören.".to_string(),
                "Wie geht es dir heute?".to_string(),
                "Alles klar!".to_string(),
            ]
        );
    }

    #[test]
    fn abbreviations_do_not_produce_mini_sentences() {
        let text = "Das ist z. B. ein Satz mit Abkürzung. Und hier kommt noch ein zweiter Satz.";
        let parts = split_sentences(text);
        assert_eq!(parts.len(), 2, "war: {parts:?}");
        assert!(parts[0].contains("z. B."));
    }

    #[test]
    fn single_or_empty_text_stays_whole() {
        assert_eq!(split_sentences("Nur ein Satz ohne Ende"), vec!["Nur ein Satz ohne Ende"]);
        assert!(split_sentences("   ").is_empty());
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
