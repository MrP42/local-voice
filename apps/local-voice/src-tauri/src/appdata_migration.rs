//! Einmalige Übernahme der App-Daten nach dem Rebranding.
//!
//! Der Bundle-Identifier wechselte am 19.08.2026 von
//! `de.wolffappliedai.sprechstift` zu `de.wolffappliedai.localvoiceai`.
//! Tauri leitet daraus die Datenpfade ab (Settings-Store, Verlauf,
//! heruntergeladene Modelle, Logs, WebView-Cache) — ohne Umzug würde die App
//! nach der Umbenennung wie frisch installiert wirken und mehrere Gigabyte
//! Modelle erneut laden. Beim Start wird deshalb je Wurzel (Roaming + Local)
//! der alte Ordner auf den neuen Namen verschoben — nur, wenn der neue noch
//! nicht existiert: vorhandene neue Daten gewinnen immer.

use std::path::Path;

pub const OLD_IDENTIFIER: &str = "de.wolffappliedai.sprechstift";
pub const NEW_IDENTIFIER: &str = "de.wolffappliedai.localvoiceai";

/// Reine Entscheidung, getrennt vom Dateisystem-Effekt: verschoben wird nur
/// alt-vorhanden-und-neu-fehlt.
pub fn should_migrate(old_exists: bool, new_exists: bool) -> bool {
    old_exists && !new_exists
}

fn migrate_root(root: &Path) {
    let old_dir = root.join(OLD_IDENTIFIER);
    let new_dir = root.join(NEW_IDENTIFIER);
    if !should_migrate(old_dir.exists(), new_dir.exists()) {
        return;
    }
    match std::fs::rename(&old_dir, &new_dir) {
        Ok(()) => log::info!(
            "Migrated app data: {} -> {}",
            old_dir.display(),
            new_dir.display()
        ),
        Err(e) => log::warn!(
            "Could not migrate app data from {}: {e} — starting with fresh data",
            old_dir.display()
        ),
    }
}

/// Vor jeder Store-/Pfadnutzung aufrufen (und nie im portablen Modus, dort
/// liegen die Daten neben der EXE und kennen keinen Identifier).
pub fn migrate_legacy_app_data() {
    if crate::portable::is_portable() {
        return;
    }
    for var in ["APPDATA", "LOCALAPPDATA"] {
        if let Ok(root) = std::env::var(var) {
            migrate_root(Path::new(&root));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_only_fires_when_old_exists_and_new_is_absent() {
        assert!(should_migrate(true, false));
        assert!(!should_migrate(true, true), "neue Daten nie überschreiben");
        assert!(!should_migrate(false, false));
        assert!(!should_migrate(false, true));
    }

    #[test]
    fn migrate_root_moves_the_old_folder_once() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join(OLD_IDENTIFIER);
        std::fs::create_dir_all(old.join("logs")).unwrap();
        std::fs::write(old.join("settings_store.json"), b"{}").unwrap();

        migrate_root(tmp.path());
        let new = tmp.path().join(NEW_IDENTIFIER);
        assert!(new.join("settings_store.json").exists());
        assert!(!old.exists());

        // Zweiter Lauf: nichts mehr zu tun, nichts wird zerstört.
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("marker.txt"), b"alt").unwrap();
        migrate_root(tmp.path());
        assert!(
            new.join("settings_store.json").exists() && !new.join("marker.txt").exists(),
            "vorhandene neue Daten gewinnen"
        );
    }
}
