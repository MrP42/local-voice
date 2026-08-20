//! Protokolle als Datei: Markdown, reiner Text oder Word (.docx).
//!
//! Warum im Backend und nicht per `writeTextFile` im Frontend: das
//! fs-Plugin prüft jeden Schreibzugriff gegen die Capability-Liste, und die
//! ist bewusst auf `$APPDATA` begrenzt. Ein Speicherziel in „Dokumente" —
//! also genau das, was der Speichern-Dialog anbietet — scheiterte deshalb
//! mit „not allowed by ACL". Den Pfad hat der Nutzer im Systemdialog selbst
//! gewählt; ihn danach noch gegen eine Positivliste zu prüfen, schützt
//! niemanden. Der Umweg über Rust erlaubt außerdem Binärformate: eine
//! .docx-Datei ist ein ZIP-Archiv und kein Text.

use std::io::Write;
use std::path::Path;

/// Zielformate des Protokoll-Exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// Markdown, unverändert wie erzeugt.
    Markdown,
    /// Reiner Text: Auszeichnungen aufgelöst, Struktur über Leerzeilen.
    PlainText,
    /// Word-Dokument mit echten Überschriften und Fettungen.
    Docx,
}

impl ExportFormat {
    /// Format aus der Dateiendung. Der Nutzer wählt im Speichern-Dialog eine
    /// Endung — die ist die Absichtserklärung, nicht ein zweites Auswahlfeld.
    /// Unbekanntes wird Markdown: Rohtext ist nie falsch, nur unschöner.
    pub fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "txt" => Self::PlainText,
            "docx" | "doc" => Self::Docx,
            _ => Self::Markdown,
        }
    }
}

/// Protokoll schreiben. Format ergibt sich aus der Endung von `path`.
pub fn write_document(path: &Path, markdown: &str) -> Result<(), String> {
    match ExportFormat::from_path(path) {
        ExportFormat::Markdown => std::fs::write(path, markdown.as_bytes())
            .map_err(|e| format!("could not write {}: {e}", path.display())),
        ExportFormat::PlainText => std::fs::write(path, markdown_to_text(markdown).as_bytes())
            .map_err(|e| format!("could not write {}: {e}", path.display())),
        ExportFormat::Docx => write_docx(path, markdown),
    }
}

// ---------------------------------------------------------------- Markdown --

/// Eine Zeile des Protokolls, so weit ausgewertet, wie beide Zielformate es
/// brauchen. Mehr Markdown als das erzeugt `minutes.rs` nicht.
#[derive(Debug, PartialEq)]
enum Block {
    Heading(u8, String),
    Bullet(String),
    Paragraph(String),
    Blank,
}

fn parse_blocks(markdown: &str) -> Vec<Block> {
    markdown
        .lines()
        .map(|raw| {
            let line = raw.trim_end();
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                return Block::Blank;
            }
            if let Some(rest) = trimmed.strip_prefix("### ") {
                return Block::Heading(3, rest.trim().to_string());
            }
            if let Some(rest) = trimmed.strip_prefix("## ") {
                return Block::Heading(2, rest.trim().to_string());
            }
            if let Some(rest) = trimmed.strip_prefix("# ") {
                return Block::Heading(1, rest.trim().to_string());
            }
            if let Some(rest) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                return Block::Bullet(rest.trim().to_string());
            }
            Block::Paragraph(trimmed.to_string())
        })
        .collect()
}

/// Ein Textabschnitt mit seiner Auszeichnung.
#[derive(Debug, PartialEq, Clone)]
struct Span {
    text: String,
    bold: bool,
    italic: bool,
}

/// `**fett**` und `*kursiv*` / `_kursiv_` in Abschnitte zerlegen.
///
/// Bewusst ein einfacher Zustandsautomat statt einer Markdown-Bibliothek:
/// die Quelle ist unser eigener Protokoll-Generator, nicht beliebiges
/// Markdown aus dem Netz. Unpaarige Zeichen bleiben stehen, statt den Rest
/// der Zeile zu verschlucken.
fn parse_spans(line: &str) -> Vec<Span> {
    let chars: Vec<char> = line.chars().collect();
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut bold = false;
    let mut italic = false;
    let mut i = 0;

    let flush = |current: &mut String, bold: bool, italic: bool, spans: &mut Vec<Span>| {
        if !current.is_empty() {
            spans.push(Span {
                text: std::mem::take(current),
                bold,
                italic,
            });
        }
    };

    while i < chars.len() {
        let two = i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*';
        if two {
            // Öffnen nur, wenn es weiter hinten auch wieder zugeht — sonst
            // wäre ein einzelnes "**" mitten im Text eine unsichtbare
            // Fettung bis zum Zeilenende.
            let closes = bold;
            let opens = !bold
                && chars
                    .get(i + 2..)
                    .unwrap_or(&[])
                    .windows(2)
                    .any(|w| w == ['*', '*']);
            if closes || opens {
                flush(&mut current, bold, italic, &mut spans);
                bold = !bold;
                i += 2;
                continue;
            }
        }
        let one = chars[i] == '*' || chars[i] == '_';
        if one {
            flush(&mut current, bold, italic, &mut spans);
            italic = !italic;
            i += 1;
            continue;
        }
        current.push(chars[i]);
        i += 1;
    }
    flush(&mut current, bold, italic, &mut spans);
    spans
}

/// Auszeichnungen entfernen — die Textfassung soll gelesen, nicht geparst
/// werden.
fn strip_marks(line: &str) -> String {
    parse_spans(line)
        .into_iter()
        .map(|s| s.text)
        .collect::<String>()
}

/// Protokoll als reiner Text.
///
/// Überschriften bleiben als eigene Zeile mit Leerzeile davor stehen und
/// Aufzählungen behalten ihr Zeichen — ohne diese Struktur wäre die
/// Textfassung eine Wand aus Sätzen.
pub fn markdown_to_text(markdown: &str) -> String {
    let mut out = String::new();
    for block in parse_blocks(markdown) {
        match block {
            Block::Heading(_, text) => {
                if !out.is_empty() && !out.ends_with("\n\n") {
                    out.push('\n');
                }
                out.push_str(&strip_marks(&text));
                out.push('\n');
            }
            Block::Bullet(text) => {
                out.push_str("  \u{2022} ");
                out.push_str(&strip_marks(&text));
                out.push('\n');
            }
            Block::Paragraph(text) => {
                out.push_str(&strip_marks(&text));
                out.push('\n');
            }
            Block::Blank => out.push('\n'),
        }
    }
    // Windows-Zeilenenden: die Textfassung landet regelmäßig im Editor.
    out.trim_end().replace('\n', "\r\n") + "\r\n"
}

// -------------------------------------------------------------------- DOCX --

/// XML-Sonderzeichen maskieren. Ein Protokoll enthält Namen und Zitate —
/// ein `&` oder `<` darin darf die Datei nicht unlesbar machen.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn runs_xml(line: &str) -> String {
    parse_spans(line)
        .into_iter()
        .map(|span| {
            let mut props = String::new();
            if span.bold {
                props.push_str("<w:b/>");
            }
            if span.italic {
                props.push_str("<w:i/>");
            }
            let props = if props.is_empty() {
                String::new()
            } else {
                format!("<w:rPr>{props}</w:rPr>")
            };
            format!(
                "<w:r>{props}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
                xml_escape(&span.text)
            )
        })
        .collect()
}

fn document_xml(markdown: &str) -> String {
    let mut body = String::new();
    for block in parse_blocks(markdown) {
        match block {
            Block::Heading(level, text) => {
                body.push_str(&format!(
                    "<w:p><w:pPr><w:pStyle w:val=\"Heading{level}\"/></w:pPr>{}</w:p>",
                    runs_xml(&text)
                ));
            }
            Block::Bullet(text) => {
                // Aufzählungszeichen als Text statt über numbering.xml: das
                // spart einen weiteren Archivteil samt Nummerierungs-
                // definition, sieht in Word identisch aus und kann nicht
                // dadurch kaputtgehen, dass eine Listen-Id nicht aufgelöst wird.
                body.push_str(&format!(
                    "<w:p><w:pPr><w:ind w:left=\"360\" w:hanging=\"180\"/></w:pPr>\
                     <w:r><w:t xml:space=\"preserve\">\u{2022} </w:t></w:r>{}</w:p>",
                    runs_xml(&text)
                ));
            }
            Block::Paragraph(text) => {
                body.push_str(&format!("<w:p>{}</w:p>", runs_xml(&text)));
            }
            // Leerzeilen im Markdown trennen Absätze, die in Word ohnehin
            // Abstand haben — ein leerer Absatz je Leerzeile ergäbe Lücken.
            Block::Blank => {}
        }
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1417" w:right="1417" w:bottom="1134" w:left="1417"/></w:sectPr></w:body></w:document>"#
    )
}

/// Formatvorlagen für Normal und Überschrift 1–3.
///
/// Ohne diesen Teil würde Word `pStyle w:val="Heading1"` ins Leere zeigen
/// lassen und alles gleich groß setzen. Die Größen sind in halben Punkt
/// angegeben (`w:sz`), so will es das Format.
const STYLES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:pPr><w:spacing w:after="120"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="0"/><w:spacing w:before="240" w:after="120"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="1"/><w:spacing w:before="200" w:after="100"/></w:pPr><w:rPr><w:b/><w:sz w:val="28"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="2"/><w:spacing w:before="160" w:after="80"/></w:pPr><w:rPr><w:b/><w:sz w:val="24"/></w:rPr></w:style>
</w:styles>"#;

const CONTENT_TYPES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;

const ROOT_RELS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCUMENT_RELS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

/// Protokoll als .docx schreiben — ein ZIP mit vier XML-Teilen, das ist das
/// ganze Format.
pub fn write_docx(path: &Path, markdown: &str) -> Result<(), String> {
    let file = std::fs::File::create(path)
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let parts: [(&str, String); 4] = [
        ("[Content_Types].xml", CONTENT_TYPES_XML.to_string()),
        ("_rels/.rels", ROOT_RELS_XML.to_string()),
        (
            "word/_rels/document.xml.rels",
            DOCUMENT_RELS_XML.to_string(),
        ),
        ("word/styles.xml", STYLES_XML.to_string()),
    ];
    for (name, content) in parts {
        zip.start_file(name, options)
            .map_err(|e| format!("docx part {name} failed: {e}"))?;
        zip.write_all(content.as_bytes())
            .map_err(|e| format!("docx part {name} failed: {e}"))?;
    }
    zip.start_file("word/document.xml", options)
        .map_err(|e| format!("docx body failed: {e}"))?;
    zip.write_all(document_xml(markdown).as_bytes())
        .map_err(|e| format!("docx body failed: {e}"))?;
    zip.finish()
        .map_err(|e| format!("could not finish {}: {e}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Protokoll: Test\n\n**Datum:** 2026-08-20\n\n## Aufgaben\n\n- Rückmeldung geben (*Wer: Herr Wolf*)\n- Vertrag & Frist prüfen\n";

    #[test]
    fn die_endung_bestimmt_das_format() {
        assert_eq!(
            ExportFormat::from_path(Path::new("a.docx")),
            ExportFormat::Docx
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("a.DOCX")),
            ExportFormat::Docx
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("a.txt")),
            ExportFormat::PlainText
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("a.md")),
            ExportFormat::Markdown
        );
        // Ohne Endung lieber Rohtext als ein kaputtes Word-Dokument.
        assert_eq!(
            ExportFormat::from_path(Path::new("protokoll")),
            ExportFormat::Markdown
        );
    }

    #[test]
    fn die_textfassung_traegt_keine_auszeichnungszeichen_mehr() {
        let text = markdown_to_text(SAMPLE);
        assert!(!text.contains('#'), "Rauten übrig:\n{text}");
        assert!(!text.contains('*'), "Sterne übrig:\n{text}");
        assert!(text.contains("Protokoll: Test"));
        assert!(text.contains("Datum: 2026-08-20"));
        assert!(text.contains("\u{2022} Rückmeldung geben (Wer: Herr Wolf)"));
        assert!(text.contains("\r\n"), "Windows-Zeilenenden fehlen");
    }

    #[test]
    fn fett_und_kursiv_werden_zu_eigenen_abschnitten() {
        let spans = parse_spans("**Datum:** 20.08. (*Wer: X*)");
        assert_eq!(spans[0].text, "Datum:");
        assert!(spans[0].bold);
        assert!(!spans[1].bold);
        let kursiv = spans.iter().find(|s| s.italic).expect("kursiver Abschnitt");
        assert_eq!(kursiv.text, "Wer: X");
    }

    #[test]
    fn ueberschriften_und_aufzaehlungen_werden_erkannt() {
        let blocks = parse_blocks(SAMPLE);
        assert!(blocks.contains(&Block::Heading(1, "Protokoll: Test".into())));
        assert!(blocks.contains(&Block::Heading(2, "Aufgaben".into())));
        assert!(blocks.contains(&Block::Bullet("Vertrag & Frist prüfen".into())));
    }

    #[test]
    fn xml_sonderzeichen_werden_maskiert() {
        let xml = document_xml("Vertrag & Frist <wichtig>");
        assert!(xml.contains("Vertrag &amp; Frist &lt;wichtig&gt;"), "{xml}");
    }

    /// Ein .docx ist erst dann eines, wenn die vier Pflichtteile im Archiv
    /// liegen — Word öffnet sonst gar nicht erst.
    #[test]
    fn das_word_dokument_enthaelt_alle_pflichtteile() {
        let dir = std::env::temp_dir().join(format!("lv-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("protokoll.docx");
        write_document(&path, SAMPLE).unwrap();

        let file = std::fs::File::open(&path).unwrap();
        let mut zip = zip::ZipArchive::new(file).expect("gültiges ZIP");
        let names: Vec<String> = zip.file_names().map(|n| n.to_string()).collect();
        for required in [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/_rels/document.xml.rels",
            "word/styles.xml",
            "word/document.xml",
        ] {
            assert!(names.contains(&required.to_string()), "{required} fehlt");
        }

        let mut body = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("word/document.xml").unwrap(), &mut body)
            .unwrap();
        assert!(body.contains("<w:pStyle w:val=\"Heading1\"/>"));
        assert!(body.contains("<w:b/>"), "Fettung fehlt");
        assert!(body.contains("Rückmeldung geben"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn markdown_wird_unveraendert_geschrieben() {
        let dir = std::env::temp_dir().join(format!("lv-export-md-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("protokoll.md");
        write_document(&path, SAMPLE).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SAMPLE);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
