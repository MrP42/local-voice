# TP2 „Stimmen klonen" — Zero-Shot Voice Cloning für Sprechstift

Datum: 2026-08-18 · Status: autonom entschieden (Nutzer hat Durcharbeiten ohne
Freigabe-Stopps angeordnet) · Milestone: M5 · Baut auf TP1 (M4) auf.

## Ziel

Patrick nimmt in der App eine 10–30-s-Referenz seiner Stimme auf (oder
importiert eine WAV-Datei), die App transkribiert sie automatisch mit der
vorhandenen STT, und ab dann liest „Vorlesen" (Tab + Hotkey) in der geklonten
Stimme vor — zero-shot über die `reference_id`-Mechanik des Fish-Servers,
ohne Training.

Nicht-Ziele: kein Finetuning, keine Mehrsprecher-Verwaltung pro Request,
keine Cloud, kein Streaming (TP5).

## Kernentscheidungen (mit Begründung)

1. **Speicherort = Server-Format, keine Duplikate.** Referenzen liegen direkt
   in `<tts_fish_dir>\references\<voice_id>\sample.wav` + `sample.lab` — exakt
   das Layout, das der Fish-Server pro Request lädt. Die Stimmenliste ist ein
   Verzeichnis-Scan (WAV+lab-Paar vorhanden), funktioniert also auch bei
   gestopptem Server. Der Ordner ist in `C:\AI\fish-speech\.git\info\exclude`
   bereits vom Commit ausgeschlossen (biometrische Daten bleiben lokal).
2. **Aufnahme über den vorhandenen Diktat-Aufnahmepfad** (16 kHz mono, VAD aus).
   16 kHz ist für Cloning nicht ideal (dokumentierte Einschränkung), aber der
   Pfad ist erprobt; Studioqualität kommt über den **Datei-Import** (WAV wird
   unverändert kopiert — Fish resampled selbst; nur fürs Transkript wird
   intern auf 16 kHz mono resampled).
3. **Transkript automatisch per STT**, im UI editierbar, als `.lab` (UTF-8
   ohne BOM) gespeichert. Die App hat die beste Quelle für das „exakte
   Transkript" bereits an Bord.
4. **Aktive Stimme als Setting** `tts_voice: Option<String>`; `None` = bisheriges
   Seed-Verhalten. Requests mit Stimme senden `reference_id` +
   `use_memory_cache: "on"` (Server cached das Referenz-Encoding) und weiterhin
   den Seed (deterministisches Sampling).
5. **Stimmen-IDs werden saniert** (`a-z0-9_-`, lowercase, max 40) — sie werden
   Verzeichnisnamen und JSON-Werte.
6. **Löschen** entfernt das Referenzverzeichnis; war die Stimme aktiv, fällt
   `tts_voice` auf `None` zurück.

## Architektur

- `managers/tts/voices.rs` (neu): pure + FS — `sanitize_voice_id`,
  `list_voices(fish_dir)`, `save_voice(fish_dir, id, samples_16k, transcript)`,
  `import_voice(fish_dir, id, source_wav, transcript)`, `delete_voice(fish_dir, id)`.
- `TtsManager`-Erweiterung: `pending_reference: Mutex<Option<Vec<f32>>>` hält
  die letzte Referenzaufnahme zwischen Stopp und Speichern; `record_reference_
  start/stop` nutzen `AudioRecordingManager` (`binding_id="voice_reference"`,
  VAD disabled) und `TranscriptionManager::transcribe` fürs Transkript;
  `speak` liest `tts_voice` mit.
- `protocol::tts_request_body` bekommt `reference_id: Option<&str>`.
- Commands: `tts_list_voices`, `tts_record_reference_start`,
  `tts_record_reference_stop -> String`, `tts_save_voice(id, transcript)`,
  `tts_import_voice(id, wav_path, transcript?) -> String`,
  `tts_delete_voice(id)`, `change_tts_voice_setting`.
- Frontend: im Bereich „Vorlesen" eine zweite Karte „Stimmen": Dropdown der
  aktiven Stimme (+ „Standardstimme (Seed)"), Stimmenliste mit Löschen,
  Dialog-Flow „Neue Stimme": Aufnehmen (Start/Stopp) ODER WAV wählen
  (tauri-plugin-dialog) → Transkript-Textarea (vorbefüllt) → Name → Speichern
  → optional Probesatz.

## Fehlerbehandlung

- Aufnahme < 3 s → Ablehnung mit Hinweis (zu kurz für brauchbares Cloning).
- Leeres Transkript → Speichern verweigert.
- Ungültiger/kollidierender Name → saniert bzw. Fehlermeldung bei Leerstring.
- Import: Datei nicht lesbar/kein WAV → Klartext-Fehler.
- Referenzaudio wird nie geloggt; Transkript nur als Länge.

## Tests

1. Unit: `sanitize_voice_id` (Umlaute, Leerzeichen, Länge, Leerstring),
   `list/save/delete_voice`-Roundtrip in `tempfile::tempdir`.
2. Mock-Server-Test: `speak` mit gesetzter Stimme sendet `reference_id` und
   `use_memory_cache` im Request-Body (Mock captured Body).
3. Headless: `--tts-test --tts-voice <id>` gegen den echten Server (Erweiterung
   des Selbsttests), sobald eine Referenz existiert; ohne Referenz: Fehlerpfad.
4. Manuell (Patrick): Referenz aufnehmen, Probesatz hören, Hotkey mit Stimme.

## Abnahmekriterien

1. Stimme in der App anlegbar (Aufnahme + Import), erscheint in der Liste.
2. Transkript wird automatisch erzeugt und ist editierbar.
3. Vorlesen nutzt die aktive Stimme (Request nachweislich mit `reference_id`).
4. Löschen räumt Verzeichnis und ggf. aktive Auswahl auf.
5. Referenzdaten bleiben lokal (git-excluded, keine Uploads, keine Logs).
