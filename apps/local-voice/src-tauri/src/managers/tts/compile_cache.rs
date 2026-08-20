//! Reparatur des PyTorch-Compile-Caches nach einem Systemabsturz.
//!
//! Fish Speech startet mit `--compile`; TorchInductor legt dabei kompilierte
//! Kernel und Autotune-Ergebnisse in einem Cache-Verzeichnis ab. Bricht der
//! Rechner mitten in einem Schreibvorgang ab, hinterlässt NTFS eine Datei mit
//! korrekter Länge, deren Daten die Platte nie erreicht haben — sie liest sich
//! als lauter Nullbytes. Beim nächsten Start liest der Inductor sie, `json`
//! scheitert an Position 0, und der Server stirbt beim Aufwärmen.
//!
//! Beobachtet am 21.08.2026: drei `.best_config`-Dateien, 208 bzw. 225 Byte,
//! zu 100 % Nullbytes, Zeitstempel 31 Sekunden vor einem Bluescreen. Der
//! Server meldete nur „exit code: 3"; derselbe Fehler kam beim bereits
//! laufenden Server als HTTP 500 heraus.
//!
//! Verhindern lässt sich das nicht — ein Stromverlust ist keine Frage der
//! Anwendung. Erkennen und beheben lässt es sich: der Cache ist per
//! Definition wiederherstellbar, und eine nachweislich zerstörte Datei zu
//! löschen kostet nur die Zeit, sie neu zu erzeugen.
//!
//! **Gelöscht wird ausschließlich, was nachweislich kaputt ist** — nie der
//! Cache als Ganzes. Ein Rundumschlag kostete hier gemessene 228 MB Kompilat
//! und Minuten Startzeit, ohne irgendetwas zu verbessern.

use std::path::{Path, PathBuf};

/// Verzeichnisse, in denen überhaupt gelöscht werden darf. Eine Schranke
/// gegen den schlimmstmöglichen Fehlgriff: zeigte die Pfadermittlung je
/// woandershin, fasst diese Datei nichts an.
const REQUIRED_DIR_PREFIX: &str = "torchinductor_";

/// Deutet dieses Startprotokoll auf einen zerstörten Compile-Cache?
///
/// Verlangt werden zwei unabhängige Hinweise: ein Compile-Backend, das
/// gescheitert ist, UND ein Lesefehler beim Entschlüsseln. Ein einzelner
/// `JSONDecodeError` irgendwo im Protokoll reicht nicht — er könnte aus einer
/// ganz anderen Ecke kommen.
pub fn looks_like_broken_compile_cache(log: &str) -> bool {
    let compile_failed = log.contains("BackendCompilerFailed")
        || log.contains("_inductor")
        || log.contains("dynamo");
    let decode_failed = log.contains("JSONDecodeError")
        || log.contains("UnicodeDecodeError")
        || log.contains("codec can't decode");
    compile_failed && decode_failed
}

/// Ist der Inhalt dieser Datei nachweislich zerstört?
///
/// Zwei Kriterien, beide ohne Ermessensspielraum:
///
/// 1. Die Datei ist nicht leer und besteht **ausschließlich** aus Nullbytes.
///    Kein gültiges Kompilat sieht so aus; das ist die Handschrift des
///    abgebrochenen Schreibvorgangs.
/// 2. Eine `.best_config` enthält kein gültiges JSON. Genau das liest der
///    Autotune-Cache, und genau daran ist er gescheitert.
///
/// Alles andere gilt als heil. Im Zweifel nicht löschen: ein überflüssig
/// gelöschter Eintrag kostet Rechenzeit, ein fälschlich behaltener kostet
/// einen weiteren Fehlstart — aber ein fälschlich gelöschter *gültiger*
/// Eintrag wäre ein selbst verursachter Schaden.
pub fn is_corrupt(path: &Path, data: &[u8]) -> bool {
    if !data.is_empty() && data.iter().all(|b| *b == 0) {
        return true;
    }
    let is_best_config = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("best_config"));
    if is_best_config {
        return serde_json::from_slice::<serde_json::Value>(data).is_err();
    }
    false
}

/// Das Cache-Verzeichnis von TorchInductor.
///
/// `TORCHINDUCTOR_CACHE_DIR` hat Vorrang (so lässt es sich umlenken), sonst
/// gilt die Voreinstellung `<temp>/torchinductor_<benutzer>`.
pub fn cache_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("TORCHINDUCTOR_CACHE_DIR") {
        if !dir.trim().is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    default_temp_cache_dir()
}

/// Der Ort, an den PyTorch von sich aus schreibt: `%TEMP%/torchinductor_<user>`.
///
/// Die App legt ihren Cache bewusst woanders ab (siehe
/// `TtsManager::inductor_cache_dir`), weil `%TEMP%` von der
/// Datenträgerbereinigung geleert wird. Dieser Pfad wird nur noch gebraucht,
/// um einen dort liegenden Bestand einmalig abzuholen.
pub fn default_temp_cache_dir() -> Option<PathBuf> {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok()?;
    Some(std::env::temp_dir().join(format!("{REQUIRED_DIR_PREFIX}{user}")))
}

/// Zerstörte Einträge aus dem Cache entfernen. Rückgabe: die gelöschten Pfade.
///
/// Kein Fehler, wenn das Verzeichnis fehlt — dann gibt es nichts zu heilen.
pub fn repair(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !name.starts_with(REQUIRED_DIR_PREFIX) {
        return Err(format!(
            "Verweigert: {} ist kein TorchInductor-Cache",
            dir.display()
        ));
    }
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut removed = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(data) = std::fs::read(&path) else {
                continue;
            };
            if is_corrupt(&path, &data) && std::fs::remove_file(&path).is_ok() {
                removed.push(path);
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Das echte Protokoll vom 21.08.2026, gekürzt auf die tragenden Zeilen.
    const REAL_LOG: &str = concat!(
        "torch._dynamo.exc.BackendCompilerFailed: backend='inductor' raised:\n",
        "JSONDecodeError: Expecting value: line 1 column 1 (char 0)\n",
        "ERROR:    Application startup failed. Exiting.\n"
    );

    #[test]
    fn das_echte_protokoll_wird_als_cache_schaden_erkannt() {
        assert!(looks_like_broken_compile_cache(REAL_LOG));
    }

    /// Auch die zweite beobachtete Spielart: Nullbytes brechen den Decoder
    /// mit einem Codec-Fehler statt eines JSON-Fehlers ab.
    #[test]
    fn auch_der_codec_fehler_zaehlt() {
        let log = "torch._inductor raised: 'utf-32-be' codec can't decode byte 0x00";
        assert!(looks_like_broken_compile_cache(log));
    }

    /// Andere Startfehler duerfen KEINE Reparatur ausloesen — sonst loescht
    /// die App Dateien wegen eines Problems, das woanders liegt.
    #[test]
    fn fremde_startfehler_loesen_keine_reparatur_aus() {
        assert!(!looks_like_broken_compile_cache(
            "CUDA out of memory. Tried to allocate 2.00 GiB"
        ));
        assert!(!looks_like_broken_compile_cache(
            "ModuleNotFoundError: No module named 'torch'"
        ));
        // JSON-Fehler allein reicht nicht: der Bezug zum Compiler fehlt.
        assert!(!looks_like_broken_compile_cache(
            "JSONDecodeError: Expecting value"
        ));
        assert!(!looks_like_broken_compile_cache(""));
    }

    #[test]
    fn eine_datei_aus_lauter_nullbytes_gilt_als_zerstoert() {
        assert!(is_corrupt(Path::new("a.py"), &[0u8; 208]));
        assert!(is_corrupt(Path::new("a.best_config"), &[0u8; 225]));
    }

    #[test]
    fn heile_dateien_bleiben_unangetastet() {
        assert!(!is_corrupt(
            Path::new("a.best_config"),
            br#"{"XBLOCK": 1024, "num_warps": 8}"#
        ));
        assert!(!is_corrupt(Path::new("kernel.py"), b"import torch\n"));
        // Eine leere Datei ist kein Beweis fuer Zerstoerung.
        assert!(!is_corrupt(Path::new("leer.py"), b""));
        // Binaerdaten mit Nullbytes, aber nicht nur solchen.
        assert!(!is_corrupt(Path::new("k.cubin"), &[0, 0, 1, 0, 0]));
    }

    /// Eine .best_config ohne gueltiges JSON ist zerstoert — genau die Datei
    /// liest der Autotune-Cache, und genau daran ist er gescheitert.
    #[test]
    fn eine_unlesbare_best_config_gilt_als_zerstoert() {
        assert!(is_corrupt(
            Path::new("a.best_config"),
            b"\x01\x02 kein json"
        ));
        // Dieselben Bytes unter anderem Namen sind nicht unser Kriterium.
        assert!(!is_corrupt(Path::new("a.py"), b"\x01\x02 kein json"));
    }

    fn temp_cache(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("6h")).unwrap();
        dir
    }

    #[test]
    fn reparatur_entfernt_nur_das_zerstoerte() {
        let dir = temp_cache("torchinductor_test");
        let kaputt = dir.join("6h").join("a.best_config");
        let heil = dir.join("6h").join("b.best_config");
        let kernel = dir.join("kernel.py");
        std::fs::write(&kaputt, [0u8; 208]).unwrap();
        std::fs::write(&heil, br#"{"XBLOCK": 1024}"#).unwrap();
        std::fs::write(&kernel, b"import torch\n").unwrap();

        let removed = repair(&dir).unwrap();
        assert_eq!(removed.len(), 1, "entfernt: {removed:?}");
        assert!(!kaputt.exists(), "die zerstoerte Datei liegt noch da");
        assert!(heil.exists(), "eine heile Datei wurde geloescht");
        assert!(kernel.exists(), "ein Kernel wurde geloescht");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Die Schranke: ausserhalb eines TorchInductor-Caches wird nichts
    /// angefasst, auch wenn dort etwas nach Schaden aussieht.
    #[test]
    fn ausserhalb_des_cache_verzeichnisses_wird_nichts_geloescht() {
        let dir = temp_cache("meine-dokumente");
        let opfer = dir.join("6h").join("wichtig.best_config");
        std::fs::write(&opfer, [0u8; 10]).unwrap();
        assert!(repair(&dir).is_err());
        assert!(opfer.exists(), "ausserhalb des Caches wurde geloescht");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ein_fehlendes_verzeichnis_ist_kein_fehler() {
        let dir = std::env::temp_dir().join("torchinductor_gibtsnicht-xyz");
        assert_eq!(repair(&dir).unwrap().len(), 0);
    }
}
