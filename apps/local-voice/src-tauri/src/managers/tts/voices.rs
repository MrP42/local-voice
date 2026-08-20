//! Verwaltung der Referenzstimmen (TP2, zero-shot Voice Cloning).
//!
//! Referenzen liegen im Server-Format direkt beim Fish-Server:
//! `<fish_dir>/references/<voice_id>/sample.wav` + `sample.lab`. Die App
//! dupliziert nichts; die Stimmenliste ist ein Verzeichnis-Scan und
//! funktioniert auch bei gestopptem Server.

use std::path::{Path, PathBuf};

/// 16-kHz-Samples unterhalb dieser Dauer taugen nicht als Referenz.
pub const MIN_REFERENCE_SECS: usize = 3;
const SAMPLE_RATE: usize = 16_000;

pub fn reference_long_enough(sample_count: usize) -> bool {
    sample_count >= MIN_REFERENCE_SECS * SAMPLE_RATE
}

/// Nutzereingaben werden Verzeichnisnamen und JSON-Werte: klein, ASCII,
/// `a-z0-9_-`, deutsche Umlaute transliteriert, max 40 Zeichen.
pub fn sanitize_voice_id(raw: &str) -> Option<String> {
    let mapped: String = raw
        .to_lowercase()
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
        .replace('ß', "ss")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    // Mehrfach-Bindestriche zusammenfassen, Ränder trimmen, Länge kappen.
    let mut collapsed = String::new();
    for c in mapped.chars() {
        if c == '-' && collapsed.ends_with('-') {
            continue;
        }
        collapsed.push(c);
    }
    let trimmed: String = collapsed.trim_matches('-').chars().take(40).collect();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn references_dir(fish_dir: &Path) -> PathBuf {
    fish_dir.join("references")
}

pub fn voice_dir(fish_dir: &Path, id: &str) -> PathBuf {
    references_dir(fish_dir).join(id)
}

/// Alle Stimmen mit mindestens einem WAV samt gleichnamiger .lab-Datei —
/// dieselbe Gültigkeitsregel, die der Fish-Server beim Laden anwendet.
pub fn list_voices(fish_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(references_dir(fish_dir)) else {
        return Vec::new();
    };
    let mut voices: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            std::fs::read_dir(e.path())
                .map(|files| {
                    files.flatten().any(|f| {
                        let p = f.path();
                        p.extension().is_some_and(|ext| ext == "wav")
                            && p.with_extension("lab").exists()
                    })
                })
                .unwrap_or(false)
        })
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    voices.sort();
    voices
}

/// Die Referenzaufnahme einer Stimme samt ihrem Transkript — genau die Datei,
/// aus der Fish Speech die Stimme nachbildet. Sie ist damit auch die
/// ehrlichste Hoerprobe: kein erzeugtes Beispiel, das erst einen Serverstart
/// und Sekunden GPU-Zeit kostet, sondern die Stimme selbst.
///
/// Genommen wird das erste WAV mit gleichnamiger .lab-Datei — dieselbe
/// Gueltigkeitsregel wie in `list_voices`, damit die Liste und die Hoerprobe
/// nicht auseinanderlaufen koennen.
pub fn voice_sample(fish_dir: &Path, id: &str) -> Option<(PathBuf, String)> {
    let dir = voice_dir(fish_dir, id);
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "wav") && path.with_extension("lab").exists()
        })
        .collect();
    // Deterministic: the same voice must always preview the same take.
    candidates.sort();
    let wav = candidates.into_iter().next()?;
    let transcript = std::fs::read_to_string(wav.with_extension("lab"))
        .unwrap_or_default()
        .trim()
        .to_string();
    Some((wav, transcript))
}

/// Aufnahme (16 kHz mono f32) als Referenz speichern: sample.wav (16-bit PCM)
/// plus sample.lab (Transkript, UTF-8 ohne BOM).
pub fn save_voice(
    fish_dir: &Path,
    id: &str,
    samples: &[f32],
    transcript: &str,
) -> Result<(), String> {
    if transcript.trim().is_empty() {
        return Err("transcript must not be empty".into());
    }
    if !reference_long_enough(samples.len()) {
        return Err(format!(
            "reference too short: need at least {MIN_REFERENCE_SECS} s of audio"
        ));
    }
    let dir = voice_dir(fish_dir, id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create {}: {e}", dir.display()))?;

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let wav_path = dir.join("sample.wav");
    let mut writer = hound::WavWriter::create(&wav_path, spec)
        .map_err(|e| format!("could not write {}: {e}", wav_path.display()))?;
    for &s in samples {
        let clamped = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer
            .write_sample(clamped)
            .map_err(|e| format!("wav write failed: {e}"))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("wav finalize failed: {e}"))?;

    write_lab(&dir, transcript)
}

/// Vorhandene WAV-Datei unverändert übernehmen (der Fish-Server resampled
/// selbst — Studioqualität bleibt erhalten) plus Transkript als .lab.
pub fn import_voice(
    fish_dir: &Path,
    id: &str,
    source_wav: &Path,
    transcript: &str,
) -> Result<(), String> {
    if transcript.trim().is_empty() {
        return Err("transcript must not be empty".into());
    }
    // Frühe Validierung: muss als WAV lesbar sein, bevor irgendetwas kopiert wird.
    hound::WavReader::open(source_wav)
        .map_err(|e| format!("not a readable WAV file ({}): {e}", source_wav.display()))?;
    let dir = voice_dir(fish_dir, id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    std::fs::copy(source_wav, dir.join("sample.wav")).map_err(|e| format!("copy failed: {e}"))?;
    write_lab(&dir, transcript)
}

fn write_lab(dir: &Path, transcript: &str) -> Result<(), String> {
    let lab_path = dir.join("sample.lab");
    std::fs::write(&lab_path, transcript.trim().as_bytes())
        .map_err(|e| format!("could not write {}: {e}", lab_path.display()))
}

/// Beliebiges PCM-WAV (Rate/Kanäle/Bittiefe egal) als 16-kHz-Mono-f32 laden —
/// nur für die STT-Transkription beim Import; die Referenzdatei selbst wird
/// unverändert kopiert. Lineares Resampling reicht für Spracherkennung.
pub fn load_wav_mono_16k(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("not a readable WAV: {e}"))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;

    let interleaved: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, _) => reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?,
        (hound::SampleFormat::Int, bits) => {
            let scale = (1i64 << (bits - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?
        }
    };

    // Downmix: Kanäle mitteln.
    let mono: Vec<f32> = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();

    if spec.sample_rate == SAMPLE_RATE as u32 {
        return Ok(mono);
    }
    if mono.is_empty() {
        return Ok(mono);
    }
    let ratio = spec.sample_rate as f64 / SAMPLE_RATE as f64;
    let out_len = ((mono.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos.floor() as usize;
        let frac = (pos - idx as f64) as f32;
        let a = mono[idx.min(mono.len() - 1)];
        let b = mono[(idx + 1).min(mono.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    Ok(out)
}

/// Referenzverzeichnis entfernen. Eine nicht (mehr) existierende Stimme ist
/// kein Fehler — das Ziel „weg" ist erreicht.
pub fn delete_voice(fish_dir: &Path, id: &str) -> Result<(), String> {
    let dir = voice_dir(fish_dir, id);
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("could not delete {}: {e}", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_ids_are_sanitized_for_filesystem_and_json() {
        assert_eq!(sanitize_voice_id("Patrick"), Some("patrick".into()));
        assert_eq!(sanitize_voice_id("Müller ß"), Some("mueller-ss".into()));
        assert_eq!(
            sanitize_voice_id("  mein  Mikro!!  "),
            Some("mein-mikro".into())
        );
        assert_eq!(sanitize_voice_id("!!!"), None);
        assert_eq!(sanitize_voice_id(""), None);
        let long = "x".repeat(80);
        assert_eq!(sanitize_voice_id(&long).unwrap().len(), 40);
    }

    #[test]
    fn save_list_delete_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let fish = dir.path();
        assert!(list_voices(fish).is_empty(), "leer ohne references/");

        let samples = vec![0.1f32; 4 * 16_000]; // 4 s
        save_voice(fish, "patrick", &samples, "Hallo, das ist meine Stimme.").unwrap();
        assert_eq!(list_voices(fish), vec!["patrick".to_string()]);

        // .lab-Inhalt: getrimmt, UTF-8 ohne BOM.
        let lab = std::fs::read(fish.join("references/patrick/sample.lab")).unwrap();
        assert_eq!(lab, b"Hallo, das ist meine Stimme.");
        assert!(!lab.starts_with(&[0xEF, 0xBB, 0xBF]), "kein BOM");

        // WAV ist als 16-kHz-Mono-PCM lesbar.
        let reader = hound::WavReader::open(fish.join("references/patrick/sample.wav")).unwrap();
        assert_eq!(reader.spec().sample_rate, 16_000);
        assert_eq!(reader.spec().channels, 1);

        delete_voice(fish, "patrick").unwrap();
        assert!(list_voices(fish).is_empty());
        delete_voice(fish, "patrick").unwrap(); // idempotent
    }

    #[test]
    fn too_short_or_untranscribed_references_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let short = vec![0.1f32; 16_000]; // 1 s
        assert!(save_voice(dir.path(), "kurz", &short, "text").is_err());
        let ok_len = vec![0.1f32; 4 * 16_000];
        assert!(save_voice(dir.path(), "leer", &ok_len, "   ").is_err());
        assert!(
            list_voices(dir.path()).is_empty(),
            "nichts halb Gespeichertes"
        );
    }

    #[test]
    fn a_wav_without_matching_lab_is_not_a_voice() {
        let dir = tempfile::tempdir().unwrap();
        let vdir = dir.path().join("references/kaputt");
        std::fs::create_dir_all(&vdir).unwrap();
        std::fs::write(vdir.join("sample.wav"), b"RIFF").unwrap();
        assert!(list_voices(dir.path()).is_empty());
    }

    #[test]
    fn arbitrary_wavs_load_as_mono_16k_for_transcription() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("studio.wav");
        // 44,1 kHz stereo, 1 s Sinus links, Stille rechts.
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..44_100u32 {
            let s = ((i as f32 * 0.05).sin() * 8000.0) as i16;
            w.write_sample(s).unwrap(); // links
            w.write_sample(0i16).unwrap(); // rechts
        }
        w.finalize().unwrap();

        let mono = load_wav_mono_16k(&path).unwrap();
        assert!(
            (mono.len() as i64 - 16_000).abs() <= 2,
            "1 s bei 44,1 kHz muss ~16000 Samples ergeben, war {}",
            mono.len()
        );
        // Downmix halbiert die Amplitude (ein stummer Kanal).
        let peak = mono.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(
            peak > 0.05 && peak < 0.2,
            "Peak {peak} außerhalb des Downmix-Erwartungsbereichs"
        );
    }

    #[test]
    fn import_rejects_non_wav_sources() {
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("nicht-wav.wav");
        std::fs::write(&bogus, b"definitiv kein wav").unwrap();
        assert!(import_voice(dir.path(), "x", &bogus, "text").is_err());
        assert!(list_voices(dir.path()).is_empty());
    }
}
