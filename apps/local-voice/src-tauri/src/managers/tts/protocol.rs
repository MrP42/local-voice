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
    tts_request_body_in_format(text, seed, reference_id, "wav")
}

/// Wie `tts_request_body`, aber mit wählbarem Ausgabeformat — der Fish-Server
/// encodiert wav/mp3/opus direkt (Datei-Export).
pub fn tts_request_body_in_format(
    text: &str,
    seed: i64,
    reference_id: Option<&str>,
    format: &str,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "text": text,
        "format": format,
        "seed": seed,
        "streaming": false,
    });
    if let Some(voice) = reference_id {
        body["reference_id"] = serde_json::json!(voice);
        body["use_memory_cache"] = serde_json::json!("on");
    }
    body
}

/// Formatbewusste Plausibilitätsprüfung der Serverantwort: Magic-Bytes plus
/// nennenswerte Nutzlast, damit HTML-Fehlerseiten nie als Audio durchgehen.
pub fn looks_like_audio(bytes: &[u8], format: &str) -> bool {
    if bytes.len() <= 1024 {
        return false;
    }
    match format {
        "wav" => bytes.starts_with(b"RIFF"),
        "mp3" => bytes.starts_with(b"ID3") || bytes.starts_with(&[0xFF]),
        "opus" => bytes.starts_with(b"OggS"),
        _ => false,
    }
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
/// Ein Stück Vorlesetext mit der Stimme, die es sprechen soll.
/// `voice == None` heißt „die eingestellte Stimme" — so klingt ein Text ohne
/// jede Sprechermarkierung genau wie vorher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceSegment {
    pub voice: Option<String>,
    pub text: String,
}

/// Zerlegt Vorlesetext in Abschnitte je Sprecher.
///
/// Eine Zeile, die mit dem Namen einer **bekannten** Stimme und einem
/// Doppelpunkt beginnt (`olga: Guten Morgen.`), schaltet auf diese Stimme um;
/// sie gilt bis zur nächsten solchen Zeile. Text vor der ersten Markierung
/// gehört der eingestellten Stimme.
///
/// Der Abgleich gegen die *vorhandenen* Stimmen ist der Kern: „Achtung: nicht
/// vergessen" fängt genauso an, ist aber keine Sprecherzeile. Ohne diese
/// Prüfung würde jeder Doppelpunkt am Zeilenanfang Text verschlucken.
/// Groß-/Kleinschreibung ist egal, damit `Olga:` und `olga:` dasselbe tun.
pub fn split_voice_segments(text: &str, known_voices: &[String]) -> Vec<VoiceSegment> {
    let mut segments: Vec<VoiceSegment> = Vec::new();
    let mut current: Option<String> = None;
    let mut buffer = String::new();

    let flush = |segments: &mut Vec<VoiceSegment>, voice: &Option<String>, buffer: &mut String| {
        if !buffer.trim().is_empty() {
            segments.push(VoiceSegment {
                voice: voice.clone(),
                text: buffer.trim().to_string(),
            });
        }
        buffer.clear();
    };

    for line in text.lines() {
        match speaker_marker(line, known_voices) {
            Some((voice, rest)) => {
                flush(&mut segments, &current, &mut buffer);
                current = Some(voice);
                if !rest.trim().is_empty() {
                    buffer.push_str(rest.trim());
                    buffer.push('\n');
                }
            }
            None => {
                buffer.push_str(line);
                buffer.push('\n');
            }
        }
    }
    flush(&mut segments, &current, &mut buffer);
    segments
}

/// `("olga", " Guten Morgen.")` für `olga: Guten Morgen.`, sonst `None`.
fn speaker_marker(line: &str, known_voices: &[String]) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    let (name, rest) = trimmed.split_once(':')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let matched = known_voices
        .iter()
        .find(|voice| voice.eq_ignore_ascii_case(name))?;
    Some((matched.clone(), rest.to_string()))
}

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
        assert_eq!(
            split_sentences("Nur ein Satz ohne Ende"),
            vec!["Nur ein Satz ohne Ende"]
        );
        assert!(split_sentences("   ").is_empty());
    }

    #[test]
    fn sprecherzeilen_schalten_die_stimme_um() {
        let voices = vec!["olga".to_string(), "patrick".to_string()];
        let text = "Vorspann ohne Sprecher.
olga: Guten Morgen.
Wie geht es dir?
patrick: Danke, gut.";
        let segments = split_voice_segments(text, &voices);
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].voice, None, "Text vor der ersten Markierung");
        assert_eq!(segments[0].text, "Vorspann ohne Sprecher.");
        assert_eq!(segments[1].voice.as_deref(), Some("olga"));
        assert_eq!(
            segments[1].text,
            "Guten Morgen.
Wie geht es dir?",
            "die Folgezeile gehoert noch olga"
        );
        assert_eq!(segments[2].voice.as_deref(), Some("patrick"));
        assert_eq!(segments[2].text, "Danke, gut.");
    }

    #[test]
    fn ein_gewoehnlicher_doppelpunkt_ist_keine_sprecherzeile() {
        let voices = vec!["olga".to_string()];
        let segments = split_voice_segments("Achtung: nicht vergessen.", &voices);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].voice, None);
        assert_eq!(
            segments[0].text, "Achtung: nicht vergessen.",
            "der Text darf nicht angeknabbert werden"
        );
    }

    #[test]
    fn sprechernamen_sind_gross_klein_egal_und_duerfen_leer_ausgehen() {
        let voices = vec!["Olga".to_string()];
        let segments = split_voice_segments(
            "OLGA:
Erste Zeile.",
            &voices,
        );
        assert_eq!(segments.len(), 1);
        assert_eq!(
            segments[0].voice.as_deref(),
            Some("Olga"),
            "gemeldet wird die Stimme, wie sie wirklich heisst"
        );
        assert_eq!(segments[0].text, "Erste Zeile.");
    }

    #[test]
    fn ohne_bekannte_stimmen_bleibt_alles_ein_stueck() {
        let segments = split_voice_segments(
            "olga: Hallo.
patrick: Hi.",
            &[],
        );
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].voice, None);
    }

    #[test]
    fn export_formats_reach_the_server_and_are_validated_by_magic() {
        let b = tts_request_body_in_format("Hallo", 42, None, "mp3");
        assert_eq!(b["format"], "mp3");
        let mut mp3 = b"ID3".to_vec();
        mp3.extend_from_slice(&[0u8; 2000]);
        assert!(looks_like_audio(&mp3, "mp3"));
        let mut ogg = b"OggS".to_vec();
        ogg.extend_from_slice(&[0u8; 2000]);
        assert!(looks_like_audio(&ogg, "opus"));
        assert!(
            !looks_like_audio(&ogg, "wav"),
            "falsches Magic je Format zählt nicht"
        );
        assert!(
            !looks_like_audio(b"OggS", "opus"),
            "Mini-Antworten sind Fehlerseiten"
        );
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
