//! Dokumente und Medien als Eingabe (statt nur Texteingabe/WAV).
//!
//! - Text aus Dokumenten: TXT/MD direkt, PDF über pdf-extract, DOCX über
//!   zip + Tag-Stripping von word/document.xml.
//! - Beliebige Audio-/Videoformate (mp3, m4a, mp4, mov, mkv, …) werden über
//!   das lokale ffmpeg zu Mono-WAV dekodiert; ohne ffmpeg gibt es eine
//!   sprechende Fehlermeldung statt eines stillen Scheiterns.

use std::path::{Path, PathBuf};

pub const DOCUMENT_EXTENSIONS: [&str; 4] = ["txt", "md", "pdf", "docx"];
pub const MEDIA_EXTENSIONS: [&str; 13] = [
    "wav", "mp3", "m4a", "aac", "flac", "ogg", "opus", "wma", "mp4", "mov", "mkv", "webm", "avi",
];

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

/// XML-Textinhalt aus DOCX-Body-XML: Absätze werden Zeilenumbrüche, Tags
/// fallen weg, die fünf XML-Entities werden aufgelöst.
pub fn docx_xml_to_text(xml: &str) -> String {
    let with_breaks = xml
        .replace("</w:p>", "\n")
        .replace("<w:tab/>", "\t")
        .replace("<w:br/>", "\n");
    let re = regex::Regex::new("<[^>]+>").expect("static regex");
    let stripped = re.replace_all(&with_breaks, "");
    stripped
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .trim()
        .to_string()
}

fn extract_docx(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("not a DOCX (zip) file: {e}"))?;
    let mut doc = archive
        .by_name("word/document.xml")
        .map_err(|_| "not a DOCX file (word/document.xml missing)".to_string())?;
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut doc, &mut xml).map_err(|e| e.to_string())?;
    Ok(docx_xml_to_text(&xml))
}

/// Text aus einem Dokument extrahieren. Der Rückgabetext ist unverändert —
/// Kürzung auf `tts_max_chars` passiert erst beim Sprechen.
pub fn extract_document_text(path: &Path) -> Result<String, String> {
    let text = match extension_of(path).as_str() {
        "txt" | "md" => {
            let bytes =
                std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes))
                .to_string()
        }
        "pdf" => pdf_extract::extract_text(path)
            .map_err(|e| format!("PDF text extraction failed: {e}"))?,
        "docx" => extract_docx(path)?,
        other => {
            return Err(format!(
                "Nicht unterstütztes Dokumentformat '.{other}' — unterstützt: {}",
                DOCUMENT_EXTENSIONS.join(", ")
            ))
        }
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("Das Dokument enthält keinen extrahierbaren Text".into());
    }
    Ok(trimmed.to_string())
}

/// Beliebiges Audio/Video über ffmpeg zu Mono-WAV mit gegebener Samplerate.
pub fn decode_media_to_wav(input: &Path, out_wav: &Path, sample_rate: u32) -> Result<(), String> {
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.args([
        "-y",
        "-i",
        &input.to_string_lossy(),
        "-vn",
        "-ac",
        "1",
        "-ar",
        &sample_rate.to_string(),
        &out_wav.to_string_lossy(),
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = cmd.output().map_err(|e| {
        format!("ffmpeg nicht gefunden ({e}) — Installation z. B.: winget install ffmpeg")
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
        return Err(format!("ffmpeg konnte {} nicht dekodieren: {tail}", input.display()));
    }
    if !out_wav.exists() {
        return Err("ffmpeg lieferte keine Ausgabedatei".into());
    }
    Ok(())
}

/// Eingabedatei als WAV bereitstellen: WAV geht direkt durch, alles andere
/// wird in eine Tempdatei dekodiert. Rückgabe: (Pfad, Option<Tempdatei zum
/// Aufräumen durch den Aufrufer über NamedTempFile-Drop>).
pub fn ensure_wav(
    input: &Path,
    sample_rate: u32,
) -> Result<(PathBuf, Option<tempfile::TempPath>), String> {
    if extension_of(input) == "wav" {
        return Ok((input.to_path_buf(), None));
    }
    let tmp = tempfile::Builder::new()
        .prefix("lva-media-")
        .suffix(".wav")
        .tempfile()
        .map_err(|e| e.to_string())?
        .into_temp_path();
    decode_media_to_wav(input, &tmp, sample_rate)?;
    Ok((tmp.to_path_buf(), Some(tmp)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn plain_text_and_markdown_read_directly_with_bom_stripped() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("notiz.txt");
        std::fs::write(&p, b"\xEF\xBB\xBFHallo Dokument.\nZweite Zeile.").unwrap();
        assert_eq!(
            extract_document_text(&p).unwrap(),
            "Hallo Dokument.\nZweite Zeile."
        );
    }

    #[test]
    fn docx_body_xml_becomes_readable_paragraphs() {
        let xml = r#"<w:document><w:body><w:p><w:r><w:t>Erster Absatz mit &amp; Zeichen.</w:t></w:r></w:p><w:p><w:r><w:t>Zweiter</w:t></w:r><w:r><w:t> Absatz.</w:t></w:r></w:p></w:body></w:document>"#;
        assert_eq!(
            docx_xml_to_text(xml),
            "Erster Absatz mit & Zeichen.\nZweiter Absatz."
        );
    }

    #[test]
    fn a_real_docx_zip_is_extracted() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("brief.docx");
        let file = std::fs::File::create(&p).unwrap();
        let mut z = zip::ZipWriter::new(file);
        let options: zip::write::SimpleFileOptions = Default::default();
        z.start_file("word/document.xml", options).unwrap();
        z.write_all("<w:document><w:body><w:p><w:r><w:t>Inhalt aus Word.</w:t></w:r></w:p></w:body></w:document>".as_bytes()).unwrap();
        z.finish().unwrap();
        assert_eq!(extract_document_text(&p).unwrap(), "Inhalt aus Word.");
    }

    #[test]
    fn unsupported_and_empty_documents_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("x.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        assert!(extract_document_text(&exe).unwrap_err().contains("txt, md, pdf, docx"));
        let empty = dir.path().join("leer.txt");
        std::fs::write(&empty, b"   \n").unwrap();
        assert!(extract_document_text(&empty).is_err());
    }

    /// Braucht ein installiertes ffmpeg; ohne wird übersprungen (lokal ist es
    /// vorhanden, der Test deckt den echten Dekodierpfad ab).
    #[test]
    fn media_files_decode_to_mono_16k_wav_via_ffmpeg() {
        if std::process::Command::new("ffmpeg").arg("-version").output().is_err() {
            eprintln!("ffmpeg fehlt — Test übersprungen");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        // 1 s 440-Hz-Ton als WAV schreiben, per ffmpeg nach mp3, dann zurück.
        let src = dir.path().join("ton.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&src, spec).unwrap();
        for i in 0..44_100u32 {
            w.write_sample(((i as f32 * 0.0627).sin() * 12000.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        let mp3 = dir.path().join("ton.mp3");
        decode_media_to_wav(&src, &dir.path().join("umweg.wav"), 44_100).unwrap();
        assert!(std::process::Command::new("ffmpeg")
            .args(["-y", "-i", &src.to_string_lossy(), &mp3.to_string_lossy()])
            .output()
            .unwrap()
            .status
            .success());

        let (wav_path, _guard) = ensure_wav(&mp3, 16_000).unwrap();
        let reader = hound::WavReader::open(&wav_path).unwrap();
        assert_eq!(reader.spec().sample_rate, 16_000);
        assert_eq!(reader.spec().channels, 1);
        let secs = reader.duration() as f32 / 16_000.0;
        assert!((secs - 1.0).abs() < 0.15, "Dauer blieb ~1 s, war {secs}");
    }
}
