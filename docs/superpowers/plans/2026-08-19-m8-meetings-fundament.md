# M8 Meetings-Fundament Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Meetings live (Mikrofon + System-Loopback, zwei Kanäle) und per Datei-Import aufnehmen, blockweise lokal transkribieren (Segmente mit Zeitstempeln), und daraus ein standardisiertes Protokoll (Structured Output) erzeugen — komplett lokal, crash-sicher, mit Consent-Gate und Retention.

**Architecture:** Neues Subsystem `managers/meetings/` mit eigener `meetings.db` (rusqlite + rusqlite_migration, ULID-Keys, Soft-Delete). Zwei getrennte Capture-Module (cpal-Mic, WASAPI-Loopback) schreiben je eine i16/16-kHz-Mono-WAV inkrementell auf Platte und füttern einen zeitbasierten Chunker; Blöcke laufen durch die neue `transcribe_segments()`-API und landen als kanal-getaggte Segmente in der DB (Live-Deltas, crash-sicher). Protokoll: `minutes.rs` (Structured-Output via `llm_client`, Map-Reduce, Validator) rendert nach Markdown (`body_format='markdown@1'`; ProseMirror kommt M10). Frontend: neuer Sidebar-Bereich nach dem etablierten 7-Schritte-Muster.

**Tech Stack:** Rust (Tauri 2, rusqlite, cpal, `wasapi`-Crate, hound nur lesend, rubato), React/TS (Zustand, Tailwind 4, i18next), tauri-specta, PowerShell-Harness.

**Spec:** `docs/superpowers/specs/2026-08-19-meetings-protokoll-design.md`

## Global Constraints

- Kein GPL/LGPL/AGPL im Cargo-Zielgraphen; nach jeder neuen Dependency: `cargo deny check licenses` (muss „ok" bleiben). Neue Crates in diesem Plan: `ulid` (MIT/Apache-2.0), `wasapi` (MIT).
- Transkript-/Protokoll-Klartext niemals in Release-Logs — Inhalte nur hinter `#[cfg(debug_assertions)]`, Release loggt Längen/Gates (DECISIONS.md D9; Muster: `segmenter.rs:174-186`).
- Jeder neue UI-String über i18next; Key zuerst in `src/i18n/locales/de/translation.json` UND `en/`, danach in allen 21 Locales (`bun run check:translations` muss grün sein; für nicht-de/en Locales ist der englische Text als Platzhalterwert zulässig — das ist Projektpraxis).
- Alle neuen Commands: `#[tauri::command] #[specta::specta]` + Eintrag in `collect_commands![]` (`src-tauri/src/lib.rs` ~Z. 836-994); typisierte Events zusätzlich in `collect_events![]` (~Z. 990). Bindings regenerieren sich beim Debug-Build (`bun run tauri dev` kurz starten oder `cargo test` + Debug-Build).
- Rust-Tests: reine Entscheidungsfunktionen bevorzugen; Mock-LLM via `TcpListener::bind("127.0.0.1:0")` (Muster `translator.rs:102`); In-Memory-SQLite (`Connection::open_in_memory()`, Muster `history.rs:657`).
- Vor jedem Commit: `cargo fmt` + betroffene Tests; Frontend: `bun run lint`. Conventional Commits (`feat:`/`fix:`/`docs:`), Fokus auf das Warum.
- Aufnahmedateien/DB liegen unter `crate::portable::app_data_dir(app)` → Unterordner `meetings/` (Portable-Modus automatisch korrekt).
- Falls Code aus anarlog (MIT) übernommen wird: Herkunft (Repo, Commit, Pfad) in `docs/THIRD-PARTY-NOTICES.md` ergänzen. Dieser Plan sieht KEINE direkte Code-Übernahme vor, nur Muster.
- Windows-only-Teile (`wasapi`) hinter `#[cfg(target_os = "windows")]`; Nicht-Windows kompiliert mit Stub, der `Err("loopback not supported on this platform")` liefert.

---

### Task 1: Meeting-Store — Schema, CRUD, Template-Seed

**Files:**
- Create: `apps/local-voice/src-tauri/src/managers/meetings/mod.rs`
- Create: `apps/local-voice/src-tauri/src/managers/meetings/store.rs`
- Modify: `apps/local-voice/src-tauri/src/managers/mod.rs` (Zeile mit `pub mod ...` ergänzen: `pub mod meetings;`)
- Modify: `apps/local-voice/src-tauri/Cargo.toml` (Dependency `ulid = "1"`)

**Interfaces:**
- Consumes: `crate::portable::app_data_dir`, rusqlite/rusqlite_migration (bereits Dependencies).
- Produces (spätere Tasks bauen exakt hierauf):
  ```rust
  pub struct MeetingStore { /* db_path: PathBuf */ }
  impl MeetingStore {
      pub fn new(app: &tauri::AppHandle) -> anyhow::Result<Self>;      // öffnet <appdata>/meetings/meetings.db, migriert
      pub fn open_at(path: &Path) -> anyhow::Result<Self>;             // für Tests
      pub fn create_meeting(&self, title: &str, source: MeetingSource, consent_confirmed_at: Option<i64>) -> anyhow::Result<Meeting>;
      pub fn set_status(&self, id: &str, status: MeetingStatus) -> anyhow::Result<()>;
      pub fn set_audio_paths(&self, id: &str, mic: Option<&str>, system: Option<&str>, duration_ms: Option<u64>) -> anyhow::Result<()>;
      pub fn get_meeting(&self, id: &str) -> anyhow::Result<Option<Meeting>>;
      pub fn list_meetings(&self, offset: u32, limit: u32) -> anyhow::Result<Vec<Meeting>>;
      pub fn soft_delete_meeting(&self, id: &str) -> anyhow::Result<Vec<String>>; // Rückgabe: zu löschende Audio-Pfade (Hard-Delete macht der Aufrufer, Task 12)
      pub fn append_delta(&self, meeting_id: &str, delta: &TranscriptDelta) -> anyhow::Result<u64>; // nächste sequence, atomar
      pub fn get_segments(&self, meeting_id: &str) -> anyhow::Result<Vec<StoredSegment>>;
      pub fn update_segment_text(&self, meeting_id: &str, segment_index: u32, text: &str) -> anyhow::Result<()>;
      pub fn upsert_document(&self, meeting_id: &str, kind: &str, body_format: &str, body: &str, generation_metadata: Option<&str>) -> anyhow::Result<String>;
      pub fn get_documents(&self, meeting_id: &str) -> anyhow::Result<Vec<MeetingDocument>>;
      pub fn list_templates(&self) -> anyhow::Result<Vec<MeetingTemplate>>;
  }
  #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
  pub struct Meeting { pub id: String, pub title: String, pub status: String, pub source: String,
      pub started_at: Option<i64>, pub ended_at: Option<i64>, pub language: Option<String>,
      pub mic_audio_path: Option<String>, pub system_audio_path: Option<String>,
      pub duration_ms: Option<u64>, pub consent_confirmed_at: Option<i64>,
      pub audio_retention_until: Option<i64>, pub created_at: i64, pub deleted_at: Option<i64> }
  #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
  pub struct StoredSegment { pub segment_index: u32, pub text: String, pub start_ms: u64, pub end_ms: u64,
      pub channel: u8, pub speaker_index: Option<u32> }   // channel: 0=DirectMic, 1=RemoteParty, 2=MixedCapture
  #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
  pub struct TranscriptDelta { pub new_segments: Vec<StoredSegment> }
  #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
  pub struct MeetingDocument { pub id: String, pub meeting_id: String, pub kind: String,
      pub body_format: String, pub body: String, pub version: u32, pub created_at: i64 }
  #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
  pub struct MeetingTemplate { pub id: String, pub title: String, pub sections_json: String, pub pinned: bool }
  pub enum MeetingSource { Live, Import, Subtitle }
  pub enum MeetingStatus { Recording, Processing, Ready, Failed }
  ```

- [ ] **Step 1: Cargo-Dependency + Lizenzcheck**

`ulid = "1"` in `[dependencies]` von `apps/local-voice/src-tauri/Cargo.toml` eintragen. Dann:
Run: `cargo deny check licenses` (im Ordner `apps/local-voice/src-tauri`)
Expected: `licenses ok`

- [ ] **Step 2: Failing Tests schreiben** (`store.rs`, `#[cfg(test)] mod tests`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> MeetingStore {
        // open_at mit Tempdir-Datei — In-Memory geht nicht, weil der Store pro Aufruf öffnet (History-Muster)
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meetings.db");
        let s = MeetingStore::open_at(&path).unwrap();
        std::mem::forget(dir); // Tempdir bis Prozessende behalten
        s
    }

    #[test]
    fn a_meeting_without_consent_timestamp_is_storable_but_marked() {
        let s = store();
        let m = s.create_meeting("Jour fixe", MeetingSource::Import, None).unwrap();
        assert!(m.consent_confirmed_at.is_none());
        assert_eq!(m.status, "processing");
    }

    #[test]
    fn live_meetings_start_in_recording_state_with_consent() {
        let s = store();
        let m = s.create_meeting("Standup", MeetingSource::Live, Some(1_755_600_000)).unwrap();
        assert_eq!(m.status, "recording");
        assert_eq!(m.consent_confirmed_at, Some(1_755_600_000));
    }

    #[test]
    fn deltas_are_sequenced_and_segments_materialize_in_order() {
        let s = store();
        let m = s.create_meeting("T", MeetingSource::Live, Some(1)).unwrap();
        let d1 = TranscriptDelta { new_segments: vec![StoredSegment {
            segment_index: 0, text: "Hallo.".into(), start_ms: 0, end_ms: 900, channel: 0, speaker_index: None }] };
        let d2 = TranscriptDelta { new_segments: vec![StoredSegment {
            segment_index: 1, text: "Guten Morgen.".into(), start_ms: 950, end_ms: 2100, channel: 1, speaker_index: None }] };
        assert_eq!(s.append_delta(&m.id, &d1).unwrap(), 1);
        assert_eq!(s.append_delta(&m.id, &d2).unwrap(), 2);
        let segs = s.get_segments(&m.id).unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[1].text, "Guten Morgen.");
        assert_eq!(segs[1].channel, 1);
    }

    #[test]
    fn segment_text_can_be_corrected() {
        let s = store();
        let m = s.create_meeting("T", MeetingSource::Live, Some(1)).unwrap();
        s.append_delta(&m.id, &TranscriptDelta { new_segments: vec![StoredSegment {
            segment_index: 0, text: "Falsch erkannt".into(), start_ms: 0, end_ms: 800, channel: 0, speaker_index: None }] }).unwrap();
        s.update_segment_text(&m.id, 0, "Richtig erkannt").unwrap();
        assert_eq!(s.get_segments(&m.id).unwrap()[0].text, "Richtig erkannt");
    }

    #[test]
    fn soft_delete_hides_the_meeting_and_returns_audio_paths() {
        let s = store();
        let m = s.create_meeting("T", MeetingSource::Live, Some(1)).unwrap();
        s.set_audio_paths(&m.id, Some("C:/x/mic.wav"), Some("C:/x/system.wav"), Some(60_000)).unwrap();
        let paths = s.soft_delete_meeting(&m.id).unwrap();
        assert_eq!(paths, vec!["C:/x/mic.wav".to_string(), "C:/x/system.wav".to_string()]);
        assert!(s.list_meetings(0, 50).unwrap().is_empty());
        assert!(s.get_meeting(&m.id).unwrap().is_none());
    }

    #[test]
    fn the_default_template_is_seeded_once() {
        let s = store();
        let t = s.list_templates().unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].title, "Standardprotokoll");
        // Alle Spec-Sektionen enthalten:
        for key in ["summary", "scope", "decisions", "tasks", "next_steps", "follow_ups", "open_questions"] {
            assert!(t[0].sections_json.contains(key), "Sektion {key} fehlt im Seed");
        }
    }

    #[test]
    fn documents_version_instead_of_overwrite() {
        let s = store();
        let m = s.create_meeting("T", MeetingSource::Live, Some(1)).unwrap();
        s.upsert_document(&m.id, "minutes", "markdown@1", "# V1", None).unwrap();
        s.upsert_document(&m.id, "minutes", "markdown@1", "# V2", None).unwrap();
        let docs = s.get_documents(&m.id).unwrap();
        assert_eq!(docs.len(), 2, "Regenerieren erzeugt neue Version statt Überschreiben (Spec M10-Vorgriff)");
        assert_eq!(docs.iter().map(|d| d.version).max(), Some(2));
    }
}
```

- [ ] **Step 3: Tests laufen lassen — müssen fehlschlagen**

Run: `cargo test --lib managers::meetings` (in `apps/local-voice/src-tauri`)
Expected: Compile-Fehler (Typen existieren nicht) — das zählt als FAIL.

- [ ] **Step 4: Implementieren**

`store.rs`: Migrationen als `static MIGRATIONS: &[M]` exakt nach dem Muster `history.rs:20-34`; **eine** Migration v1 mit allen Tabellen (Spec Abschnitt 5):

```sql
CREATE TABLE meetings (
  id TEXT PRIMARY KEY, title TEXT NOT NULL, status TEXT NOT NULL,
  source TEXT NOT NULL, started_at INTEGER, ended_at INTEGER, language TEXT,
  mic_audio_path TEXT, system_audio_path TEXT, duration_ms INTEGER,
  consent_confirmed_at INTEGER, audio_retention_until INTEGER,
  metadata_json TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);
CREATE TABLE meeting_documents (
  id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL, kind TEXT NOT NULL, template_id TEXT,
  title TEXT, body_format TEXT NOT NULL, body TEXT NOT NULL,
  generation_metadata_json TEXT, version INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);
CREATE TABLE transcripts (
  id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL UNIQUE, provider TEXT, model TEXT, language TEXT,
  granularity TEXT NOT NULL DEFAULT 'segment@1', segments_json TEXT NOT NULL DEFAULT '[]',
  speaker_hints_json TEXT, content_revision INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);
CREATE TABLE transcript_deltas (
  transcript_id TEXT NOT NULL, sequence INTEGER NOT NULL, delta_json TEXT NOT NULL,
  created_at INTEGER NOT NULL, PRIMARY KEY (transcript_id, sequence));
CREATE TABLE speakers (
  id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL, channel INTEGER NOT NULL,
  speaker_index INTEGER, human_id TEXT, display_name TEXT, consent_state TEXT,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);
CREATE TABLE humans (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, email TEXT, memo TEXT,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);
CREATE TABLE action_items (
  id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL, text TEXT NOT NULL,
  assignee_human_id TEXT, due_at INTEGER, status TEXT NOT NULL DEFAULT 'todo',
  source TEXT NOT NULL, kind TEXT NOT NULL,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);
CREATE TABLE meeting_templates (
  id TEXT PRIMARY KEY, title TEXT NOT NULL, sections_json TEXT NOT NULL,
  pinned INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);
```

Implementierungsregeln:
- `Connection::open(&self.db_path)` pro Methode (History-Muster; ein Meeting schreibt alle ~20 s einen Delta — das trägt).
- IDs: `ulid::Ulid::new().to_string()`.
- `append_delta`: **eine Transaktion**: `sequence = 1 + COALESCE(MAX(sequence),0)` für das Transkript (Transkript-Row lazy anlegen bei erstem Delta), Delta-JSON einfügen, UND die Segmente in `transcripts.segments_json` materialisieren (`content_revision += 1`). So ist `get_segments` ein einfacher JSON-Read und der Crash-Replay (Task 8) kann Deltas gegen `segments_json` abgleichen.
- `create_meeting`: Status `recording` bei `MeetingSource::Live`, sonst `processing`.
- `soft_delete_meeting`: setzt `deleted_at` auf alle Rows des Meetings, liest vorher die Audio-Pfade und gibt sie zurück; **löscht keine Dateien selbst** (Trennung: Dateisystem macht der Aufrufer, testbar ohne echte Dateien).
- Alle List-/Get-Reads filtern `deleted_at IS NULL`.
- Template-Seed in `new()`/`open_at()` nach der Migration: wenn `meeting_templates` leer →
  ```rust
  let sections = serde_json::json!(["summary","scope","speakers","speaking_shares",
      "decisions","tasks","next_steps","follow_ups","open_questions"]).to_string();
  // INSERT Standardprotokoll, pinned=1
  ```
- `mod.rs`: `pub mod store; pub use store::*;`

- [ ] **Step 5: Tests laufen lassen**

Run: `cargo test --lib managers::meetings`
Expected: alle 7 Tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/managers/meetings/ src-tauri/src/managers/mod.rs
git commit -m "feat(meetings): meetings.db store with sequenced deltas, soft-delete and template seed"
```

---

### Task 2: Streaming-WAV-Writer mit Crash-Reparatur

**Files:**
- Create: `apps/local-voice/src-tauri/src/audio_toolkit/audio/wav_writer.rs`
- Modify: `apps/local-voice/src-tauri/src/audio_toolkit/audio/mod.rs` (`pub mod wav_writer; pub use wav_writer::StreamingWavWriter;`)

**Interfaces:**
- Produces:
  ```rust
  pub struct StreamingWavWriter { /* file, frames_written */ }
  impl StreamingWavWriter {
      pub fn create(path: &Path, sample_rate: u32) -> std::io::Result<Self>;   // schreibt 44-Byte-Header (PCM i16 mono), Größenfelder 0
      pub fn append(&mut self, samples: &[i16]) -> std::io::Result<()>;         // schreibt Samples, hält frames_written
      pub fn flush_header(&mut self) -> std::io::Result<()>;                    // patcht RIFF/data-Größen per Seek, dann flush — alle ~1 s aufrufen
      pub fn finalize(mut self) -> std::io::Result<u64>;                        // Header patchen + close, Rückgabe: Dauer in ms
      pub fn frames_written(&self) -> u64;
  }
  /// Repariert eine ohne finalize() zurückgelassene Datei (Crash): Größenfelder aus der Dateilänge rekonstruieren.
  /// Rückgabe: Dauer in ms, oder None wenn die Datei kein reparierbares WAV ist.
  pub fn repair_orphan_wav(path: &Path) -> std::io::Result<Option<u64>>;
  ```

- [ ] **Step 1: Failing Tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_wav_is_readable_by_hound_after_finalize() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.wav");
        let mut w = StreamingWavWriter::create(&p, 16_000).unwrap();
        w.append(&vec![0i16; 16_000]).unwrap(); // 1 s
        let ms = w.finalize().unwrap();
        assert_eq!(ms, 1000);
        let r = hound::WavReader::open(&p).unwrap();
        assert_eq!(r.spec().sample_rate, 16_000);
        assert_eq!(r.spec().channels, 1);
        assert_eq!(r.len(), 16_000);
    }

    #[test]
    fn a_crashed_file_is_repairable_and_loses_nothing_flushed() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("crash.wav");
        {
            let mut w = StreamingWavWriter::create(&p, 16_000).unwrap();
            w.append(&vec![7i16; 32_000]).unwrap(); // 2 s
            w.flush_header().unwrap();
            // KEIN finalize — Drop simuliert den Crash
        }
        let ms = repair_orphan_wav(&p).unwrap().expect("muss reparierbar sein");
        assert_eq!(ms, 2000);
        let r = hound::WavReader::open(&p).unwrap();
        assert_eq!(r.len(), 32_000);
    }

    #[test]
    fn repair_rejects_non_wav_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("junk.wav");
        std::fs::write(&p, b"not a wav").unwrap();
        assert!(repair_orphan_wav(&p).unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run** `cargo test --lib audio_toolkit::audio::wav_writer` — Expected: Compile-FAIL.

- [ ] **Step 3: Implementieren**

Kein hound zum Schreiben (hound kann kein Header-Patchen bei offener Datei über Prozessgrenzen); von Hand:
- Header: `RIFF <size> WAVE fmt (16, PCM=1, ch=1, rate, byte_rate, block_align=2, bits=16) data <size>`.
- `append`: `samples`-Bytes little-endian anhängen (`file.write_all`), `frames_written += samples.len()`.
- `flush_header`: `seek(4)` → `36 + data_len`, `seek(40)` → `data_len`, zurück ans Ende, `file.sync_data()`.
- `repair_orphan_wav`: Datei ≥ 44 Bytes, beginnt mit `RIFF`/`WAVE`? → Größen aus `metadata().len()` errechnen, Header patchen. Sample-Rate aus dem Header lesen (Bytes 24-27) für die ms-Berechnung.

- [ ] **Step 4: Run** `cargo test --lib audio_toolkit::audio::wav_writer` — Expected: 3 PASS.

- [ ] **Step 5: Commit** — `feat(meetings): streaming wav writer with crash repair (1s flush cadence)`

---

### Task 3: Loopback-Zeitachse (pure Logik, C1)

**Files:**
- Create: `apps/local-voice/src-tauri/src/audio_toolkit/audio/loopback_timeline.rs`
- Modify: `apps/local-voice/src-tauri/src/audio_toolkit/audio/mod.rs`

**Interfaces:**
- Produces:
  ```rust
  /// Führt die Zeitachse des Loopback-Streams anhand der Device-Position (Frames
  /// seit Stream-Start, aus dem WASAPI-Capture-Client), NIE anhand gezählter Buffer.
  pub struct LoopbackTimeline { /* expected_next_frame: u64 */ }
  pub enum TimelineAction {
      /// So viele Silence-Frames VOR dem Buffer einfügen (Lücke durch Stille).
      PadSilence(u64),
      /// Buffer direkt anhängen.
      Append,
      /// Buffer verwerfen (Positionssprung rückwärts — Gerätewechsel o. ä.); Aufrufer loggt.
      Drop,
  }
  impl LoopbackTimeline {
      pub fn new() -> Self;
      pub fn on_buffer(&mut self, device_position_frames: u64, buffer_frames: u64) -> TimelineAction;
  }
  ```

- [ ] **Step 1: Failing Tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contiguous_buffers_append_without_padding() {
        let mut t = LoopbackTimeline::new();
        assert!(matches!(t.on_buffer(0, 480), TimelineAction::Append));
        assert!(matches!(t.on_buffer(480, 480), TimelineAction::Append));
    }

    #[test]
    fn a_silence_gap_is_padded_not_compressed() {
        let mut t = LoopbackTimeline::new();
        t.on_buffer(0, 480);
        // 3 Sekunden Stille bei 48 kHz: Position springt um 144_000 Frames
        match t.on_buffer(480 + 144_000, 480) {
            TimelineAction::PadSilence(n) => assert_eq!(n, 144_000),
            other => panic!("erwartet PadSilence, bekam {other:?}"),
        }
    }

    #[test]
    fn the_first_buffer_defines_time_zero_even_at_nonzero_position() {
        // Stream lief schon, bevor wir zuhören: erste Position != 0 erzeugt KEIN Padding.
        let mut t = LoopbackTimeline::new();
        assert!(matches!(t.on_buffer(96_000, 480), TimelineAction::Append));
        assert!(matches!(t.on_buffer(96_480, 480), TimelineAction::Append));
    }

    #[test]
    fn backwards_position_jumps_drop_the_buffer() {
        let mut t = LoopbackTimeline::new();
        t.on_buffer(10_000, 480);
        assert!(matches!(t.on_buffer(5_000, 480), TimelineAction::Drop));
    }
}
```

- [ ] **Step 2: Run** `cargo test --lib loopback_timeline` — Expected: FAIL (nicht vorhanden).

- [ ] **Step 3: Implementieren** — Zustand: `expected_next: Option<u64>`. Erster Buffer: `expected_next = pos + frames`, Append. Danach: `pos == expected` → Append; `pos > expected` → `PadSilence(pos - expected)`; `pos < expected` → Drop (expected unverändert). Nach Pad/Append `expected_next = pos + frames`. `#[derive(Debug)]` auf `TimelineAction`.

- [ ] **Step 4: Run** — 4 PASS. **Step 5: Commit** — `feat(meetings): loopback timeline derives padding from device position (silence != time compression)`

---

### Task 4: WASAPI-Loopback-Capture (Windows)

**Files:**
- Create: `apps/local-voice/src-tauri/src/audio_toolkit/audio/loopback.rs`
- Modify: `apps/local-voice/src-tauri/src/audio_toolkit/audio/mod.rs`
- Modify: `apps/local-voice/src-tauri/Cargo.toml` — unter `[target.'cfg(target_os = "windows")'.dependencies]`: `wasapi = "0.15"`

**Interfaces:**
- Consumes: `LoopbackTimeline` (Task 3), rubato (vorhanden).
- Produces:
  ```rust
  /// Startet Loopback-Capture des Default-Render-Endpoints. Liefert 16-kHz-Mono-i16-Blöcke
  /// (zeitachsen-korrekt inkl. Silence-Padding) an den Callback, bis stop() gerufen wird.
  pub struct LoopbackCapture { /* thread handle, stop flag */ }
  impl LoopbackCapture {
      pub fn start(on_samples: impl FnMut(&[i16]) + Send + 'static) -> anyhow::Result<Self>;
      pub fn stop(self);
  }
  /// Pure: f32-Interleaved beliebiger Kanalzahl -> Mono (Mittelwert je Frame).
  pub fn downmix_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32>;
  /// Pure: f32 [-1,1] -> i16 mit Clamping.
  pub fn f32_to_i16(samples: &[f32]) -> Vec<i16>;
  ```

- [ ] **Step 1: Failing Tests für die puren Funktionen**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_downmix_averages_the_channels() {
        let mono = downmix_to_mono(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2);
        assert_eq!(mono, vec![0.5, 0.5, 0.0]);
    }

    #[test]
    fn i16_conversion_clamps_out_of_range() {
        let out = f32_to_i16(&[0.0, 1.0, -1.0, 2.0, -2.0]);
        assert_eq!(out, vec![0, 32767, -32768, 32767, -32768]);
    }
}
```

- [ ] **Step 2: Run** `cargo test --lib audio_toolkit::audio::loopback` — FAIL. **Implementieren, Run → PASS.**

- [ ] **Step 3: Capture-Thread implementieren** (`#[cfg(target_os = "windows")]`, kein Unit-Test — Abnahme im Harness Task 15)

Ablauf im Thread (wasapi-Crate-API):
```rust
// initialize_mta() einmal im Thread; DeviceEnumerator -> default render device
// -> IAudioClient im Share-Mode mit AUDCLNT_STREAMFLAGS_LOOPBACK (wasapi: Direction::Capture auf Render-Device)
// Mix-Format abfragen (typisch 48kHz f32 stereo); Loop:
//   capture_client.read_from_device(...) -> (buffer, flags, device_position)
//   timeline.on_buffer(device_position, frames):
//     PadSilence(n) -> resampler mit n Frames Stille füttern
//     Drop         -> continue (im Release nur Zähler loggen)
//     Append       -> downmix_to_mono -> Resampler (rubato SincFixedIn, mix_rate -> 16_000) -> f32_to_i16 -> on_samples(&block)
//   AUDCLNT_BUFFERFLAGS_SILENT im Flag: Bufferinhalt durch Nullen ersetzen, Position zählt normal weiter
// stop-Flag (Arc<AtomicBool>) beendet den Loop; Rest im Resampler flushen.
```
Wichtige Regel im Code-Kommentar festhalten: **Zeitachse kommt ausschließlich aus `device_position`** (Spec C1). Nicht-Windows: `LoopbackCapture::start` gibt `Err(anyhow!("loopback capture is windows-only in M8"))`.

- [ ] **Step 4:** `cargo deny check licenses` → ok; `cargo build` (Windows) → ok.

- [ ] **Step 5: Commit** — `feat(meetings): wasapi loopback capture -> 16k mono i16 with position-based timeline`

---

### Task 5: Mic-Capture für Meetings (eigenständig, ohne Diktat-Pfad)

**Files:**
- Create: `apps/local-voice/src-tauri/src/managers/meetings/mic_capture.rs`
- Modify: `apps/local-voice/src-tauri/src/managers/meetings/mod.rs`

**Interfaces:**
- Consumes: `crate::audio_toolkit::list_input_devices` (Geräteauswahl wie `managers/audio.rs:408-447`), cpal, rubato, `f32_to_i16` (Task 4).
- Produces:
  ```rust
  /// Meeting-Mikrofonaufnahme: eigener cpal-Stream, KEIN VAD (die WAV muss lückenlos sein),
  /// 16 kHz mono i16 an den Callback. Bewusst getrennt vom Diktat-AudioRecorder:
  /// der ist M3-stabilisiert und sammelt in RAM — beides wollen wir hier nicht anfassen.
  pub struct MeetingMicCapture { /* stream, stop */ }
  impl MeetingMicCapture {
      pub fn start(device_name: Option<String>, on_samples: impl FnMut(&[i16]) + Send + 'static) -> anyhow::Result<Self>;
      pub fn stop(self);
  }
  ```

- [ ] **Step 1:** Implementieren nach dem Muster von `audio_toolkit/audio/recorder.rs` (Gerät auflösen → `build_input_stream` → Callback resampled auf 16 k mono → `f32_to_i16`). Kein RAM-Gesamtpuffer. Fehlerpfad: cpal-Error-Callback loggt und setzt ein `error_flag`, das der Manager (Task 8) als `recording-error`-Event meldet.

- [ ] **Step 2: Kompilations- und Smoke-Check**

Run: `cargo build` und `cargo test --lib` (bestehende Tests unverändert grün — beweist: Diktat-Pfad nicht berührt).
Expected: build ok, Testzahl unverändert PASS.

- [ ] **Step 3: Commit** — `feat(meetings): dedicated meeting mic capture (no VAD, no RAM buffer, dictation path untouched)`

---

### Task 6: `transcribe_segments()` am TranscriptionManager

**Files:**
- Modify: `apps/local-voice/src-tauri/src/managers/transcription.rs` (neben `transcribe()`, Z. 1492)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
  pub struct TimedSegment { pub start_ms: u64, pub end_ms: u64, pub text: String }
  impl TranscriptionManager {
      /// Wie transcribe(), aber mit Zeitstempeln. Engines ohne Segment-Support
      /// liefern EIN Segment über die volle Audiolänge — nie einen Fehler.
      pub fn transcribe_segments(&self, audio: Vec<f32>) -> anyhow::Result<Vec<TimedSegment>>;
  }
  /// Pure Konvertierung, einzeln testbar:
  pub fn segments_from_result(full_text: &str,
      segments: Option<Vec<transcribe_rs::TranscriptionSegment>>, audio_ms: u64) -> Vec<TimedSegment>;
  ```

- [ ] **Step 1: Failing Tests** (in `transcription.rs`-Testmodul)

```rust
#[test]
fn engine_segments_become_ms_and_keep_their_text() {
    let segs = vec![
        transcribe_rs::TranscriptionSegment { start: 0.0, end: 1.5, text: " Hallo.".into() },
        transcribe_rs::TranscriptionSegment { start: 1.62, end: 3.0, text: " Wie geht's?".into() },
    ];
    let out = segments_from_result("Hallo. Wie geht's?", Some(segs), 3_000);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].start_ms, 0);
    assert_eq!(out[0].end_ms, 1_500);
    assert_eq!(out[1].start_ms, 1_620);
    assert_eq!(out[1].text, "Wie geht's?"); // getrimmt
}

#[test]
fn engines_without_segments_yield_one_full_span_segment() {
    let out = segments_from_result("Ganzer Text.", None, 9_150);
    assert_eq!(out.len(), 1);
    assert_eq!((out[0].start_ms, out[0].end_ms), (0, 9_150));
    assert_eq!(out[0].text, "Ganzer Text.");
}

#[test]
fn empty_text_yields_no_segments_at_all() {
    assert!(segments_from_result("   ", None, 5_000).is_empty());
    let ws = vec![transcribe_rs::TranscriptionSegment { start: 0.0, end: 1.0, text: "  ".into() }];
    assert!(segments_from_result("", Some(ws), 1_000).is_empty());
}
```

- [ ] **Step 2: Run** → FAIL. 

- [ ] **Step 3: Implementieren**

- `segments_from_result`: Segmente mappen (`(s.start * 1000.0).round() as u64`, Text `trim`, leere raus); `None`/leer → ein Segment `0..audio_ms` mit `full_text.trim()`, sofern nicht leer.
- `transcribe_segments`: Kopie der `transcribe()`-Struktur (Aktivitäts-Touch, Load-Wait, catch_unwind, Engine-Take — identisch Z. 1492-1611), aber im Engine-Match:
  - `LoadedEngine::Parakeet`: `transcribe_with(&audio, &params)` mit `TimestampGranularity::Segment` → `segments_from_result(&r.text, r.segments, audio_ms)`.
  - Alle anderen transcribe-rs-Engines (`Moonshine`, `MoonshineStreaming`, `SenseVoice`, `GigaAM`, `Canary`, `Cohere`): Ergebnis ist ebenfalls `TranscriptionResult` → gleiche Konvertierung mit `r.segments`.
  - `LoadedEngine::TranscribeCpp`: `session.run(...)` liefert nur Text → `segments_from_result(&t.text, None, audio_ms)`.
  - `audio_ms = (audio.len() as u64 * 1000) / 16_000`.
- **Kein Refactor von `transcribe()`** in diesem Task (Diktat-Pfad bleibt byte-identisch; Duplizierung ist hier der Preis für M3-Stabilität — im Code-Kommentar auf diesen Plan verweisen).

- [ ] **Step 4: Run** `cargo test --lib managers::transcription` → neue 3 PASS, alte unverändert.

- [ ] **Step 5: Commit** — `feat(meetings): transcribe_segments() exposes engine timestamps (segment granularity per spec decision 9)`

---

### Task 7: Audio-Chunker für Live-Blöcke (pure, C-sicher)

**Files:**
- Create: `apps/local-voice/src-tauri/src/managers/meetings/chunker.rs`
- Modify: `apps/local-voice/src-tauri/src/managers/meetings/mod.rs`

**Interfaces:**
- Produces:
  ```rust
  /// Sammelt i16-Samples eines Kanals und schneidet ~20-s-Blöcke für die Transkription.
  /// Schnittpunkt: energieärmste 200-ms-Stelle im letzten Viertel des Fensters, damit
  /// nicht mitten im Wort geschnitten wird. Pure Logik, kein I/O.
  pub struct ChannelChunker { /* buffer: Vec<i16>, consumed_ms: u64 */ }
  pub struct Chunk { pub samples: Vec<f32>, pub offset_ms: u64 }  // f32, weil transcribe_segments das erwartet
  impl ChannelChunker {
      pub fn new(target_ms: u64) -> Self;                     // Produktion: 20_000
      pub fn push(&mut self, samples: &[i16]) -> Option<Chunk>; // Some, sobald ein Block voll ist
      pub fn flush(&mut self) -> Option<Chunk>;                 // Rest bei Stop/Pause (auch < target)
  }
  ```

- [ ] **Step 1: Failing Tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    const RATE: usize = 16_000;

    fn loud(ms: usize) -> Vec<i16> { vec![12_000; ms * RATE / 1000] }
    fn quiet(ms: usize) -> Vec<i16> { vec![50; ms * RATE / 1000] }

    #[test]
    fn no_chunk_before_the_target_is_reached() {
        let mut c = ChannelChunker::new(20_000);
        assert!(c.push(&loud(19_000)).is_none());
    }

    #[test]
    fn the_cut_lands_in_the_quiet_zone_not_mid_word() {
        let mut c = ChannelChunker::new(20_000);
        // 17 s laut, 1 s leise, 3 s laut -> Schnitt muss in der leisen Zone liegen (17-18 s)
        let mut audio = loud(17_000); audio.extend(quiet(1_000)); audio.extend(loud(3_000));
        let chunk = c.push(&audio).expect("21 s > 20 s Ziel");
        let cut_ms = chunk.samples.len() * 1000 / RATE;
        assert!((17_000..=18_000).contains(&cut_ms), "Schnitt bei {cut_ms} ms statt in der Pause");
        assert_eq!(chunk.offset_ms, 0);
    }

    #[test]
    fn offsets_accumulate_across_chunks() {
        let mut c = ChannelChunker::new(20_000);
        let first = c.push(&loud(21_000)).unwrap();
        let consumed = first.samples.len() * 1000 / RATE;
        let second_input = loud(21_000);
        let second = c.push(&second_input).unwrap();
        assert_eq!(second.offset_ms, consumed as u64);
    }

    #[test]
    fn flush_returns_the_tail_and_then_nothing() {
        let mut c = ChannelChunker::new(20_000);
        c.push(&loud(5_000));
        let tail = c.flush().expect("Rest muss kommen");
        assert_eq!(tail.samples.len(), 5 * RATE);
        assert!(c.flush().is_none());
    }

    #[test]
    fn i16_becomes_normalized_f32() {
        let mut c = ChannelChunker::new(1_000);
        let chunk = c.push(&vec![i16::MAX; RATE + 160]).unwrap();
        assert!((chunk.samples[0] - 1.0).abs() < 1e-3);
    }
}
```

- [ ] **Step 2: Run** → FAIL. **Step 3: Implementieren** — Energiefenster: RMS über 200-ms-Fenster im Bereich `[0.75*target, len]`, Schnitt am Minimal-RMS-Fensterende; `consumed_ms` läuft über `samples-Länge` des abgegebenen Blocks; i16→f32 `/ 32768.0`. **Step 4: Run** → 5 PASS.

- [ ] **Step 5: Commit** — `feat(meetings): channel chunker cuts ~20s blocks at the quietest spot`

---

### Task 8: MeetingRecorderManager + Lifecycle-Commands + Indikator

**Files:**
- Create: `apps/local-voice/src-tauri/src/managers/meetings/recorder.rs`
- Create: `apps/local-voice/src-tauri/src/commands/meetings.rs`
- Modify: `apps/local-voice/src-tauri/src/commands/mod.rs` (`pub mod meetings;`)
- Modify: `apps/local-voice/src-tauri/src/lib.rs` — Manager-Registrierung im Init-Block (~Z. 192-387), Commands in `collect_commands![]` (~Z. 836-994), Events in `collect_events![]` (~Z. 990)
- Modify: `apps/local-voice/src-tauri/src/actions.rs` — Diktat-Sperre (siehe Step 3)

**Interfaces:**
- Consumes: `MeetingStore` (T1), `StreamingWavWriter`/`repair_orphan_wav` (T2), `LoopbackCapture` (T4), `MeetingMicCapture` (T5), `TranscriptionManager::transcribe_segments` (T6), `ChannelChunker` (T7).
- Produces:
  ```rust
  pub struct MeetingRecorderManager { /* Arc<Mutex<MeetingRunState>>, store, app_handle */ }
  pub enum MeetingRunState { Idle, Recording { meeting_id: String, paused: bool } }
  impl MeetingRecorderManager {
      pub fn new(app: &tauri::AppHandle, store: Arc<MeetingStore>, tm: Arc<TranscriptionManager>) -> Self;
      pub fn start(&self, title: String, consent_confirmed: bool, capture_system: bool) -> Result<Meeting, String>;
      pub fn pause(&self) -> Result<(), String>;
      pub fn resume(&self) -> Result<(), String>;
      pub fn stop(&self) -> Result<String, String>;      // -> meeting_id, Status wird 'processing' bis Tail-Blöcke fertig, dann 'ready'
      pub fn is_recording(&self) -> bool;
      pub fn recover_orphans(&self);                     // beim App-Start: status='recording' -> WAVs reparieren, Status 'ready' (Segmente aus Deltas sind schon da)
  }
  // Commands (alle #[tauri::command] #[specta::specta], async wo blockierend):
  //   meetings_start(title, consent_confirmed, capture_system) -> Meeting
  //   meetings_pause() / meetings_resume() / meetings_stop() -> String
  //   meetings_list(offset, limit) -> Vec<Meeting>
  //   meetings_get_segments(meeting_id) -> Vec<StoredSegment>
  //   meetings_update_segment(meeting_id, segment_index, text)
  //   meetings_get_documents(meeting_id) -> Vec<MeetingDocument>
  //   meetings_delete(meeting_id)
  // Typisiertes Event (Muster HistoryUpdatePayload, history.rs:42-53):
  #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
  #[serde(tag = "kind")]
  pub enum MeetingEvent {
      #[serde(rename = "state")]   State { meeting_id: String, status: String, paused: bool },
      #[serde(rename = "segments")] Segments { meeting_id: String, appended: Vec<StoredSegment> },
      #[serde(rename = "levels")]  Levels { mic: f32, system: f32 },
      #[serde(rename = "error")]   Error { meeting_id: String, message: String },
  }
  ```

- [ ] **Step 1: Failing Tests für die reine Zustandslogik** (in `recorder.rs`)

```rust
#[test]
fn start_without_consent_is_refused() {
    assert_eq!(consent_gate(false), Err("consent_required".to_string()));
    assert!(consent_gate(true).is_ok());
}

#[test]
fn only_one_meeting_records_at_a_time() {
    let s = MeetingRunState::Recording { meeting_id: "m1".into(), paused: false };
    assert!(!may_start(&s));
    assert!(may_start(&MeetingRunState::Idle));
}

#[test]
fn pause_and_resume_toggle_only_in_recording() {
    let mut s = MeetingRunState::Recording { meeting_id: "m".into(), paused: false };
    assert!(apply_pause(&mut s, true).is_ok());
    assert!(matches!(&s, MeetingRunState::Recording { paused: true, .. }));
    let mut idle = MeetingRunState::Idle;
    assert!(apply_pause(&mut idle, true).is_err());
}
```
(`consent_gate(bool) -> Result<(), String>`, `may_start(&MeetingRunState) -> bool`, `apply_pause(&mut MeetingRunState, bool) -> Result<(), String>` sind freie Funktionen im Modul.)

- [ ] **Step 2: Run** → FAIL. Implementieren der drei Funktionen. Run → PASS.

- [ ] **Step 3: Manager-Orchestrierung implementieren**

`start()`:
1. `consent_gate` + `may_start`; zusätzlich: läuft gerade ein Diktat (`AudioRecordingManager::is_recording()`) → `Err("dictation_active")`.
2. `store.create_meeting(title, Live, Some(now_unix))`; Ordner `<appdata>/meetings/<meeting_id>/` anlegen; zwei `StreamingWavWriter` (`mic.wav`, immer; `system.wav` nur bei `capture_system`).
3. `MeetingMicCapture::start` mit Callback: bei `paused` Samples verwerfen (Zeit läuft weiter — Pause = bewusste Lücke), sonst `wav.append` + je 1 s `flush_header` + `chunker.push`; volle Chunks → Worker-Thread-Queue.
4. `LoopbackCapture::start` analog (channel 1). Fehler beim Loopback-Start: Meeting läuft mic-only weiter, `MeetingEvent::Error` mit Hinweis.
5. Worker-Thread: Chunk → `tm.transcribe_segments(chunk.samples)` → Segmente auf `chunk.offset_ms` verschieben, `channel` setzen, `segment_index` fortlaufend je Meeting → `store.append_delta` → `MeetingEvent::Segments` emitten. Fehler: loggen (nur Länge), `MeetingEvent::Error`, weiterlaufen (ein kaputter Block bricht das Meeting nicht ab).
6. Pegel: RMS je Callback-Block, gedrosselt (~5/s) als `MeetingEvent::Levels`.
7. **Indikator (Spec A1):** Tray-Tooltip + Icon-Zustand „Aufnahme" über das bestehende Tray-Modul (`tray.rs`, `CurrentTrayIconState`); Overlay dauerhaft sichtbar über den `notice`-Mechanismus aus M3 (`overlay.rs`) — erscheint auch bei `overlay_style: none`, Text via i18n-Key `meetings.recordingIndicator`, bleibt bis `stop()`.

`stop()`: Captures stoppen, Chunker `flush()` beider Kanäle in die Queue, Status `processing`, nach Abarbeitung der Queue: WAVs `finalize()`, `set_audio_paths`, `audio_retention_until` gemäß Setting (Task 12 liefert die Berechnung — bis dahin `None`), Status `ready`, Events.

`recover_orphans()` (Aufruf in `lib.rs` nach Manager-Registrierung): alle Meetings mit `status='recording'` → `repair_orphan_wav` auf beide Pfade (falls vorhanden — Pfade dazu vorher via `set_audio_paths` beim Start schreiben, nicht erst beim Stop!), Status `ready`, Log (nur IDs).
**Korrektur dazu in `start()` Schritt 2:** `set_audio_paths` sofort nach dem Anlegen der Writer aufrufen, damit Recovery die Pfade kennt.

Diktat-Sperre in `actions.rs`: an der Stelle, an der der Transkriptions-Hotkey die Aufnahme startet, zuerst `app.state::<Arc<MeetingRecorderManager>>()` prüfen; wenn `is_recording()` → Overlay-Notice `meetings.dictationBlocked` und Abbruch (Muster: bestehende Fehlerpfade in `actions.rs`).

- [ ] **Step 4: Commands + Registrierung** — `commands/meetings.rs` als dünne Hülle über Manager + Store (Muster: `commands/history.rs`); `collect_commands![...]` und `collect_events![MeetingEvent]` ergänzen. Debug-Build starten, damit `src/bindings.ts` regeneriert.

Run: `cargo test --lib` (alles grün) und `cargo build`.

- [ ] **Step 5: Commit** — `feat(meetings): recorder manager — dual capture, live chunk pipeline, consent gate, crash recovery, tray/overlay indicator`

---

### Task 9: Import — Audiodateien und Untertitel

**Files:**
- Create: `apps/local-voice/src-tauri/src/managers/meetings/import.rs`
- Create: `apps/local-voice/src-tauri/src/managers/meetings/subtitle.rs`
- Modify: `apps/local-voice/src-tauri/src/commands/meetings.rs` (+ `meetings_import_file(path)`), `lib.rs` (`collect_commands!`)

**Interfaces:**
- Consumes: `media::ensure_wav(input, 16_000)` (`media.rs:181`), hound (lesen), `transcribe_segments` (T6), `ChannelChunker` (T7, target 60_000 für Batch), `MeetingStore` (T1), `MeetingEvent` (T8).
- Produces:
  ```rust
  pub async fn import_media_file(app: &tauri::AppHandle, store: Arc<MeetingStore>,
      tm: Arc<TranscriptionManager>, path: PathBuf) -> Result<String, String>; // -> meeting_id
  /// Pure: VTT- oder SRT-Text -> Segmente (channel = 2 / MixedCapture).
  pub fn parse_subtitles(content: &str) -> Result<Vec<StoredSegment>, String>;
  ```

- [ ] **Step 1: Failing Tests für den Untertitel-Parser**

```rust
#[test]
fn srt_blocks_become_segments_with_ms_times() {
    let srt = "1\n00:00:01,000 --> 00:00:03,500\nGuten Morgen zusammen.\n\n2\n00:00:04,000 --> 00:00:06,000\nBeginnen wir mit dem Status.\n";
    let segs = parse_subtitles(srt).unwrap();
    assert_eq!(segs.len(), 2);
    assert_eq!((segs[0].start_ms, segs[0].end_ms), (1_000, 3_500));
    assert_eq!(segs[1].text, "Beginnen wir mit dem Status.");
    assert!(segs.iter().all(|s| s.channel == 2));
}

#[test]
fn vtt_header_and_cue_settings_are_tolerated() {
    let vtt = "WEBVTT\n\n00:00:00.500 --> 00:00:02.000 align:start\nHallo.\n\nNOTE irrelevant\n\n00:01:00.000 --> 00:01:02.250\nZweiter Satz.\n";
    let segs = parse_subtitles(vtt).unwrap();
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[1].start_ms, 60_000);
    assert_eq!(segs[0].text, "Hallo.");
}

#[test]
fn garbage_is_an_error_not_an_empty_import() {
    assert!(parse_subtitles("kein untertitelformat").is_err());
}

#[test]
fn multiline_cues_join_with_spaces() {
    let srt = "1\n00:00:01,000 --> 00:00:03,000\nZeile eins\nZeile zwei\n";
    assert_eq!(parse_subtitles(srt).unwrap()[0].text, "Zeile eins Zeile zwei");
}
```

- [ ] **Step 2: Run** → FAIL. Parser implementieren (zeilenbasiert: Timecode-Regex `(\d{2}):(\d{2}):(\d{2})[.,](\d{3})\s*-->\s*...`, Cue-Text bis Leerzeile, `NOTE`/Nummern-Zeilen/`WEBVTT` überspringen; kein Timecode gefunden → Err). Run → PASS.

- [ ] **Step 3: `import_media_file` implementieren**

1. Endung `.vtt`/`.srt` → Datei lesen, `parse_subtitles`, Meeting (`Subtitle`-Source) + ein Delta mit allen Segmenten, Status `ready`. Fertig.
2. Sonst: Meeting (`Import`, Status `processing`) anlegen; `MeetingEvent::State` senden; in `tauri::async_runtime::spawn_blocking`: `media::ensure_wav(&path, 16_000)` → hound-Reader → i16-Samples → `ChannelChunker::new(60_000)`-Schleife: je Chunk `transcribe_segments` → Offset addieren → `channel = 2` → `append_delta` + `Segments`-Event (das IST die Fortschrittsanzeige). WAV-Kopie nach `<appdata>/meetings/<id>/import.wav` (Quelle des Nutzers bleibt unangetastet), `set_audio_paths(mic=import.wav)`. Ende: Status `ready`. Fehler: Status `failed` + `Error`-Event — **niemals** stilles Verwerfen.

- [ ] **Step 4: Run** `cargo test --lib managers::meetings` → PASS; `cargo build` ok.

- [ ] **Step 5: Commit** — `feat(meetings): file import shares the recording pipeline (channel=mixed), vtt/srt import`

---

### Task 10: Redeanteile (deterministisch)

**Files:**
- Create: `apps/local-voice/src-tauri/src/managers/meetings/stats.rs`
- Modify: `apps/local-voice/src-tauri/src/managers/meetings/mod.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
  pub struct SpeakerShare { pub label: String, pub channel: u8, pub speech_ms: u64, pub percent: f64 }
  /// Redeanteile aus Segmentdauern. M8: Label je Kanal ("Ich" / "Gegenseite" / "Aufnahme").
  /// Die Labels sind Anzeige-Fallbacks; das Frontend übersetzt über channel.
  pub fn speaking_shares(segments: &[StoredSegment]) -> Vec<SpeakerShare>;
  ```

- [ ] **Step 1: Failing Tests**

```rust
#[test]
fn shares_sum_to_100_and_split_by_channel() {
    let segs = vec![
        StoredSegment { segment_index: 0, text: "a".into(), start_ms: 0, end_ms: 6_000, channel: 0, speaker_index: None },
        StoredSegment { segment_index: 1, text: "b".into(), start_ms: 6_000, end_ms: 8_000, channel: 1, speaker_index: None },
    ];
    let shares = speaking_shares(&segs);
    assert_eq!(shares.len(), 2);
    assert_eq!(shares[0].speech_ms, 6_000);
    assert!((shares[0].percent - 75.0).abs() < 0.01);
    assert!((shares.iter().map(|s| s.percent).sum::<f64>() - 100.0).abs() < 0.01);
}

#[test]
fn a_single_channel_import_yields_one_share_of_100() {
    let segs = vec![StoredSegment { segment_index: 0, text: "x".into(), start_ms: 0, end_ms: 1_000, channel: 2, speaker_index: None }];
    let shares = speaking_shares(&segs);
    assert_eq!(shares.len(), 1);
    assert!((shares[0].percent - 100.0).abs() < f64::EPSILON);
}

#[test]
fn no_segments_no_shares_no_division_by_zero() {
    assert!(speaking_shares(&[]).is_empty());
}
```

- [ ] **Step 2-4:** Run FAIL → implementieren (Summe je `channel`, Prozent = speech_ms/total*100) → Run PASS.

- [ ] **Step 5: Commit** — `feat(meetings): deterministic speaking shares from segment durations`

---

### Task 11: Protokoll-Erzeugung (`minutes.rs`) + Markdown-Rendering

**Files:**
- Create: `apps/local-voice/src-tauri/src/managers/meetings/minutes.rs`
- Modify: `apps/local-voice/src-tauri/src/commands/meetings.rs` (+ `meetings_generate_minutes(meeting_id)`), `lib.rs`

**Interfaces:**
- Consumes: `llm_client::send_chat_completion_with_schema` (Signatur: `(provider, api_key, model, user_content, system_prompt, json_schema, reasoning_effort, reasoning) -> Result<Option<String>, String>`), Provider-Auflösung nach dem Muster `summarizer.rs:118-155` (`active_post_process_provider`, `post_process_models`, `post_process_api_keys`), `chunk_text` aus `summarizer.rs:87`, `speaking_shares` (T10), Store (T1).
- Produces:
  ```rust
  #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
  pub struct MinutesJson {
      pub summary: String,
      pub scope: String,
      pub decisions: Vec<DecisionItem>,     // { text, context }
      pub tasks: Vec<TaskItem>,             // { text, assignee: Option<String>, due: Option<String> }
      pub next_steps: Vec<OwnedItem>,       // { text, owner: Option<String> }
      pub follow_ups: Vec<ReasonedItem>,    // { text, reason: String }
      pub open_questions: Vec<TextItem>,    // { text }
  }
  pub struct MinutesHead { pub title: String, pub date_iso: String, pub duration_ms: u64,
      pub shares: Vec<SpeakerShare>, pub single_speaker: bool }
  pub fn minutes_schema() -> serde_json::Value;                       // JSON-Schema, strict, additionalProperties:false
  pub fn minutes_system_prompt() -> String;
  pub fn minutes_user_prompt(head: &MinutesHead, transcript: &str) -> String;
  pub fn render_transcript_for_prompt(segments: &[StoredSegment]) -> String;  // "„Ich| Gegenseite [mm:ss]": Text je Zeile
  pub fn validate_minutes(m: &MinutesJson, single_speaker: bool) -> Result<(), String>;
  pub fn minutes_to_markdown(head: &MinutesHead, m: &MinutesJson) -> String;
  pub async fn generate_minutes(app: &tauri::AppHandle, store: Arc<MeetingStore>, meeting_id: &str) -> Result<MeetingDocument, String>;
  ```

- [ ] **Step 1: Failing Tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn head(single: bool) -> MinutesHead {
        MinutesHead { title: "Jour fixe".into(), date_iso: "2026-08-19".into(), duration_ms: 1_800_000,
            shares: vec![SpeakerShare { label: "Ich".into(), channel: 0, speech_ms: 900_000, percent: 60.0 },
                         SpeakerShare { label: "Gegenseite".into(), channel: 1, speech_ms: 600_000, percent: 40.0 }],
            single_speaker: single }
    }

    fn minimal_minutes() -> MinutesJson {
        MinutesJson { summary: "Es wurde der Projektstand besprochen und der Go-Live bestätigt.".into(),
            scope: "Wöchentlicher Projekt-Jour-fixe.".into(),
            decisions: vec![], tasks: vec![], next_steps: vec![], follow_ups: vec![], open_questions: vec![] }
    }

    #[test]
    fn the_user_prompt_carries_head_data_and_transcript_but_no_invented_speakers() {
        let p = minutes_user_prompt(&head(false), "Ich [00:00]: Hallo.");
        assert!(p.contains("Jour fixe"));
        assert!(p.contains("60"));                       // Redeanteil steht als Fakt im Prompt
        assert!(p.contains("Ich [00:00]: Hallo."));
        assert!(p.contains("Do not invent"));            // Anti-Halluzination-Regel
    }

    #[test]
    fn the_schema_forbids_extra_properties_and_requires_all_sections() {
        let s = minutes_schema();
        assert_eq!(s["additionalProperties"], serde_json::json!(false));
        let req = s["required"].as_array().unwrap();
        for k in ["summary","scope","decisions","tasks","next_steps","follow_ups","open_questions"] {
            assert!(req.iter().any(|v| v == k), "{k} fehlt in required");
        }
    }

    #[test]
    fn validation_rejects_empty_summary_but_allows_empty_lists() {
        let mut m = minimal_minutes();
        assert!(validate_minutes(&m, false).is_ok(), "leere Listen sind zulässig");
        m.summary = "  ".into();
        assert!(validate_minutes(&m, false).is_err());
    }

    #[test]
    fn markdown_contains_all_sections_and_shares_table() {
        let md = minutes_to_markdown(&head(false), &minimal_minutes());
        for h in ["# Protokoll: Jour fixe", "## Zusammenfassung", "## Scope", "## Sprecher & Redeanteile",
                  "## Entscheidungen", "## Aufgaben", "## Next Steps", "## Follow-Ups", "## Offene Fragen"] {
            assert!(md.contains(h), "{h} fehlt");
        }
        assert!(md.contains("60,0 %") || md.contains("60.0 %"));
        assert!(md.contains("_keine_"), "leere Sektionen sagen das explizit statt zu fehlen");
    }

    #[test]
    fn single_speaker_markdown_omits_the_shares_table() {
        let md = minutes_to_markdown(&head(true), &minimal_minutes());
        assert!(!md.contains("## Sprecher & Redeanteile"), "Spec: Validator-Ausnahme Ein-Sprecher-Import");
    }

    #[test]
    fn transcript_rendering_prefixes_channel_and_time() {
        let segs = vec![StoredSegment { segment_index: 0, text: "Hallo.".into(), start_ms: 65_000, end_ms: 66_000, channel: 1, speaker_index: None }];
        assert_eq!(render_transcript_for_prompt(&segs), "Gegenseite [01:05]: Hallo.");
    }
}
```

- [ ] **Step 2: Run** → FAIL. 

- [ ] **Step 3: Implementieren**

- `minutes_schema()`: von Hand gebautes `serde_json::json!`-Schema (strict; alle 7 Felder required; Item-Objekte mit `required`-Feldern und `additionalProperties: false`; optionale Strings als `["string","null"]`).
- `minutes_system_prompt()`: englischer System-Prompt (Muster anarlog, eigener Text): Rolle „meeting-minutes writer", Antwortsprache = Transkriptsprache, „Do not invent participants, numbers, dates or decisions that are not in the transcript. Unclear items belong in open_questions."
- `minutes_user_prompt()`: Kopfblock (Titel, Datum, Dauer, Redeanteile als Faktenliste — „these numbers are computed, restate them, never recompute") + `# Transcript`-Block.
- Map-Reduce: `render_transcript_for_prompt` → bei > 16 000 Zeichen `chunk_text` (aus `summarizer`, dort `pub` machen falls nötig) → je Chunk eine Zwischen-Extraktion mit demselben Schema → Merge-Prompt mit den JSON-Zwischenergebnissen. 
- `generate_minutes`: Provider/Modell/Key wie `summarizer::ask_llm`; Aufruf `send_chat_completion_with_schema(provider, key, &model, user, Some(system), Some(minutes_schema()), None, None)`; Antwort `serde_json::from_str::<MinutesJson>` (bei Parse-Fehler: **ein** Retry mit Fehler-Hinweis im Prompt, danach Err); `validate_minutes`; `minutes_to_markdown`; `store.upsert_document(kind="minutes", body_format="markdown@1", generation_metadata_json = {"model":…,"provider":…})`. Klartext-Inhalte nie loggen.
- Command `meetings_generate_minutes(meeting_id)` async → Ergebnis-Dokument; Fehler als String zum Frontend.

- [ ] **Step 4: Mock-LLM-Integrationstest** (Muster `translator.rs:102 spawn_llm_mock`)

```rust
#[tokio::test]
async fn generate_minutes_persists_a_versioned_markdown_document() {
    // spawn_llm_mock liefert ein gültiges MinutesJson als chat.completions-Antwort;
    // Settings-Provider zeigt auf 127.0.0.1:<port>; Store mit einem Meeting + 2 Segmenten.
    // Assert: Dokument existiert, body beginnt mit "# Protokoll:", body_format == "markdown@1",
    // generation_metadata_json enthält den Modellnamen.
}
```
(Vollständig ausschreiben; `spawn_llm_mock` aus `translator.rs` kopieren und auf das Schema-Feld `response_format` tolerant machen.)

- [ ] **Step 5: Run** `cargo test --lib managers::meetings::minutes` → alle PASS. **Commit** — `feat(meetings): structured-output minutes with validator, map-reduce and markdown rendering`

---

### Task 12: Retention & Löschkaskade

**Files:**
- Create: `apps/local-voice/src-tauri/src/managers/meetings/retention.rs`
- Modify: `apps/local-voice/src-tauri/src/settings.rs` (Feld + Default), `apps/local-voice/src-tauri/src/shortcut/mod.rs` (`change_*`-Command nach bestehendem Muster), `src/stores/settingsStore.ts` (`settingUpdaters`-Eintrag), `lib.rs` (Startup-Hook + Command)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
  #[serde(rename_all = "snake_case")]
  pub enum MeetingAudioRetention { AfterMinutes, Days(u32), Forever }   // Default: AfterMinutes
  /// Pure: Ablaufzeitpunkt bestimmen. AfterMinutes -> Some(now) sobald ein minutes-Dokument existiert.
  pub fn retention_until(policy: &MeetingAudioRetention, now_unix: i64, ended_at: i64, has_minutes: bool) -> Option<i64>;
  /// Löscht fällige Audio-Dateien (hart) und nullt die Pfade. Rückgabe: Anzahl gelöschter Dateien.
  pub fn purge_due_audio(store: &MeetingStore, now_unix: i64) -> anyhow::Result<u32>;
  /// Hard-Delete der Audio-Dateien eines soft-gelöschten Meetings (Kaskade, Spec A2).
  pub fn delete_audio_files(paths: &[String]) -> u32;
  ```
- Settings-Feld: `meeting_audio_retention: MeetingAudioRetention` mit `#[serde(default = "default_meeting_audio_retention")]` (= `AfterMinutes`).

- [ ] **Step 1: Failing Tests**

```rust
#[test]
fn after_minutes_policy_expires_once_minutes_exist() {
    let p = MeetingAudioRetention::AfterMinutes;
    assert_eq!(retention_until(&p, 1_000, 900, false), None);
    assert_eq!(retention_until(&p, 1_000, 900, true), Some(1_000));
}

#[test]
fn days_policy_counts_from_meeting_end() {
    let p = MeetingAudioRetention::Days(3);
    assert_eq!(retention_until(&p, 0, 1_000_000, false), Some(1_000_000 + 3 * 86_400));
}

#[test]
fn forever_never_expires() {
    assert_eq!(retention_until(&MeetingAudioRetention::Forever, 5, 1, true), None);
}

#[test]
fn purge_deletes_files_and_clears_paths() {
    // Store mit Meeting, echte Temp-WAV-Dateien, audio_retention_until in der Vergangenheit
    // -> purge_due_audio == 2, Dateien weg, get_meeting(...).mic_audio_path == None
}

#[test]
fn soft_delete_cascade_removes_files_from_disk() {
    // soft_delete_meeting liefert Pfade (Task 1) -> delete_audio_files löscht sie; Datei existiert nicht mehr
}
```
(Die letzten beiden vollständig mit `tempfile::tempdir()` ausschreiben; `MeetingStore` braucht dafür `set_retention_until(&self, id, Option<i64>)` und eine Query `meetings_with_due_audio(now)` — beide in diesem Task ergänzen.)

- [ ] **Step 2: Run** → FAIL → implementieren → PASS.

- [ ] **Step 3: Verdrahtung** — `stop()` (T8) und `generate_minutes` (T11) rufen `retention_until` + `set_retention_until` auf; `lib.rs`-Startup ruft `purge_due_audio` nach `recover_orphans()`; `meetings_delete`-Command (T8) ruft nach `soft_delete_meeting` jetzt `delete_audio_files`. Settings-Feld + `change_meeting_audio_retention`-Command + `settingUpdaters`.

- [ ] **Step 4: Run** `cargo test --lib` gesamt → PASS. **Commit** — `feat(meetings): audio retention policy (default: delete after minutes) with hard-delete cascade`

---

### Task 13: Frontend I — Sidebar-Bereich, Consent-Dialog, Aufnahme, Live-Transkript

**Files:**
- Create: `apps/local-voice/src/components/settings/meetings/MeetingsSettings.tsx`
- Create: `apps/local-voice/src/components/settings/meetings/RecorderCard.tsx`
- Create: `apps/local-voice/src/components/settings/meetings/LiveTranscript.tsx`
- Modify: `apps/local-voice/src/components/settings/index.ts` (Export), `apps/local-voice/src/components/Sidebar.tsx:44-99` (Sektion), `src/i18n/locales/*/translation.json` (21 Locales)

**Interfaces:**
- Consumes: generierte Bindings aus T8 (`commands.meetingsStart`, `meetingsPause`, `meetingsResume`, `meetingsStop`, `meetingsGetSegments`, `events.meetingEvent`), UI-Primitive aus `src/components/ui/` (`Button`, `Dialog`, `SettingsGroup`, `ProgressBar`), Muster `TtsSettings.tsx` (Card-Komposition) und `HistorySettings.tsx` (Event-Sync).
- Produces: Sidebar-Eintrag `meetings` (Icon `Users` aus lucide-react, `enabled: () => true`), i18n-Namespace `meetings.*`.

- [ ] **Step 1: i18n-Keys anlegen** (de + en vollständig, restliche 19 Locales mit en-Werten)

`de/translation.json` (Auszug — vollständige Liste im Step umsetzen):
```json
"sidebar": { "meetings": "Besprechungen" },
"meetings": {
  "title": "Besprechungen",
  "record": { "start": "Aufnahme starten", "stop": "Beenden", "pause": "Pause", "resume": "Weiter",
    "titlePlaceholder": "Titel der Besprechung", "captureSystem": "System-Audio (Gegenseite) mitschneiden",
    "micLevel": "Mikrofon", "systemLevel": "System" },
  "consent": { "title": "Einwilligung erforderlich",
    "body": "Die Aufnahme nichtöffentlich gesprochener Worte ohne Einwilligung ALLER Beteiligten ist strafbar (§ 201 StGB). Bestätigen Sie, dass alle Beteiligten informiert sind und zugestimmt haben.",
    "confirm": "Alle Beteiligten haben zugestimmt", "cancel": "Abbrechen" },
  "recordingIndicator": "● Besprechung wird aufgezeichnet",
  "dictationBlocked": "Diktat ist während einer Besprechungsaufnahme deaktiviert",
  "live": { "empty": "Noch keine Segmente — das erste erscheint nach ~20 Sekunden.", "me": "Ich", "remote": "Gegenseite", "mixed": "Aufnahme" },
  "errors": { "consentRequired": "Ohne bestätigte Einwilligung startet keine Aufnahme.", "dictationActive": "Ein Diktat läuft gerade — bitte zuerst beenden." }
}
```

Run: `bun run check:translations` → Expected: PASS (alle 21 Locales vollständig).

- [ ] **Step 2: Komponenten bauen**

`MeetingsSettings.tsx` — Kompositionsmuster von `TtsSettings.tsx`:
```tsx
export const MeetingsSettings: React.FC = () => (
  <div className="max-w-3xl w-full mx-auto space-y-6">
    <RecorderCard />
    <LiveTranscript />
    {/* Task 14 ergänzt: <MeetingList /> */}
  </div>
);
```
`RecorderCard`: Titel-Input, Checkbox „System-Audio", Start-Button → öffnet `Dialog` (Consent, Text `meetings.consent.body`, Bestätigen-Button ruft `commands.meetingsStart(title, true, captureSystem)`); Zustands-UI (recording/paused) aus `events.meetingEvent` (`kind === "state"`); Pegelbalken aus `kind === "levels"` (zwei `ProgressBar`); Pause/Resume/Stop-Buttons. Fehler (`kind === "error"`) als `Alert`.
`LiveTranscript`: subscribt `meetingEvent` (`kind === "segments"`), hängt Segmente an eine Liste (`useState`, Auto-Scroll ans Ende), Kanal-Badge über `meetings.live.me|remote|mixed` je `channel` 0|1|2, Zeitstempel `mm:ss` aus `start_ms`.
Sidebar: Eintrag zwischen `history` und `models` einfügen (Muster Z. 51-56). Export in `settings/index.ts`.

- [ ] **Step 3: Verifikation**

Run: `bun run build` (TypeScript strict) und `bun run lint`
Expected: beide Exit 0. Dann `bun run tauri dev` kurz starten: Bereich „Besprechungen" sichtbar, Consent-Dialog erscheint vor Start, Ablehnen startet nichts.

- [ ] **Step 4: Commit** — `feat(meetings): meetings section with consent-gated recorder card and live transcript`

---

### Task 14: Frontend II — Meeting-Liste, Detail, Protokoll, Import

**Files:**
- Create: `apps/local-voice/src/components/settings/meetings/MeetingList.tsx`
- Create: `apps/local-voice/src/components/settings/meetings/MeetingDetail.tsx`
- Create: `apps/local-voice/src/components/settings/meetings/MinutesView.tsx`
- Modify: `MeetingsSettings.tsx` (Liste einbinden; Detail als Vollflächen-Wechsel innerhalb des Bereichs via `useState<string | null>(selectedMeetingId)`), i18n (21 Locales, Keys unten)

**Interfaces:**
- Consumes: Bindings (`meetingsList`, `meetingsGetSegments`, `meetingsUpdateSegment`, `meetingsGetDocuments`, `meetingsGenerateMinutes`, `meetingsImportFile`, `meetingsDelete`), Tauri-Dialog-Plugin (bereits vorhanden — Muster Datei-Picker in `ReadingCard.tsx`), `HistorySettings.tsx` als Struktur-Vorlage (Pagination mit IntersectionObserver-Sentinel).

- [ ] **Step 1: i18n-Keys** (de/en voll, Rest en): `meetings.list.*` (empty, import, deleteConfirm), `meetings.detail.*` (transcriptTab, minutesTab, generate, regenerate, export, editSegment, save, shares), `meetings.status.*` (recording, processing, ready, failed). `bun run check:translations` → PASS.

- [ ] **Step 2: Implementieren**

- `MeetingList`: Seiten zu 25 via `meetingsList(offset, limit)`; Zeile: Titel, Datum (`toLocaleString`), Dauer, Status-Badge; Klick → Detail; Lösch-Button mit Bestätigung (`window.confirm`-Ersatz: `Dialog`); „Importieren…"-Button → Datei-Dialog (Filter: wav, mp3, m4a, mp4, mkv, mov, flac, ogg, vtt, srt) → `meetingsImportFile(path)`; Refresh bei `meetingEvent.kind === "state"`.
- `MeetingDetail`: zwei Tabs (Buttons, `useState`): **Transkript** — Segmentliste wie `LiveTranscript`, plus Stift-Icon je Segment → `Textarea` inline → Speichern via `meetingsUpdateSegment` (Spec: Textarea-Korrektur bis M10); **Protokoll** — `MinutesView`.
- `MinutesView`: `meetingsGetDocuments` → neueste `minutes`-Version rendern (Markdown → HTML: die im Projekt vorhandene Markdown-Render-Utility verwenden; falls keine existiert, Segment für M8: `<pre className="whitespace-pre-wrap">`-Fallback — KEINE neue Markdown-Lib ohne Lizenzcheck einführen); Buttons „Erzeugen/Neu erzeugen" (`meetingsGenerateMinutes`, Ladezustand, Fehler-Alert) und „Als .md exportieren" (Tauri save-Dialog + `writeTextFile` — Muster Export in TTS-Bereich); Redeanteile-Sektion steht im Markdown selbst (T11).

- [ ] **Step 3: Verifikation** — `bun run build`, `bun run lint`, `bun run check:translations` → Exit 0. Dev-Lauf: Import einer kurzen WAV erzeugt sichtbares Meeting mit Segmenten; „Erzeugen" ohne konfigurierten LLM-Provider zeigt die Fehlermeldung aus dem Backend (kein Crash).

- [ ] **Step 4: Commit** — `feat(meetings): meeting list, detail with transcript correction, minutes view with export and import`

---

### Task 15: Abnahme-Harness, Fixtures, Evidence (C1/C2-Pflichtszenarien)

**Files:**
- Create: `apps/local-voice/scripts/m8-verify.ps1`
- Create: `apps/local-voice/scripts/make-m8-fixtures.ps1`
- Create: `docs/m8-evidence/harness-report.md` (vom Harness geschrieben)

**Interfaces:**
- Consumes: CLI-Muster aus `scripts/selftest-matrix.ps1` (headless), `tauri_plugin_single_instance`-Remote-Muster aus `m3-verify.ps1`; neue CLI-Flags NICHT nötig — Import + Protokoll laufen über die UI-Commands; für headless nutzt der Harness die Store-DB direkt (sqlite3-Abfragen) plus einen neuen Debug-Command? Nein: **ein neues CLI-Flag** `--import-meeting <datei>` in `cli.rs` + Behandlung in `lib.rs` (Muster `--transcribe-file`, Z. 599-821), das Import synchron ausführt und den DB-Pfad + Meeting-ID auf stdout gibt. Das ist Teil dieses Tasks (klein, folgt exakt dem bestehenden Muster).

- [ ] **Step 1: Fixtures erzeugen** (`make-m8-fixtures.ps1`, nutzt `scripts/bin/TtsGen.exe` + ffmpeg wie `make-fixtures*.ps1`)
  - `m8_short_de.wav` — 60 s deutsche Sprache (TtsGen-Sätze aneinandergehängt).
  - `m8_silence_gap.wav` — 10 min: 3 min Sprache, **3 min echte Stille**, 4 min Sprache (C1-Fixture; per ffmpeg `concat`).
  - `m8_import.mp4` — `m8_short_de.wav` in mp4 gemuxt (Import-Matrix).
  - `m8_sub.vtt` — 3 Cues von Hand.

- [ ] **Step 2: Szenarien in `m8-verify.ps1`** (jede Prüfung schreibt PASS/FAIL in den Report; Skript stirbt nie auf dem eigenen Fehlerpfad — Lehre aus M3):
  1. `import-wav`: `--import-meeting m8_short_de.wav` → sqlite3: Meeting `ready`, ≥ 1 Segment, `channel=2`, `segments_json` nicht leer.
  2. `import-matrix`: dasselbe für mp4 und vtt (vtt: exakt 3 Segmente mit den Cue-Zeiten).
  3. `silence-timeline (C1)`: `m8_silence_gap.wav` importieren → letztes Segment muss `start_ms > 350_000` haben (nach der Stille; beweist: keine Zeitachsen-Kompression im Batch-Pfad). Live-Loopback-Variante: manueller Abschnitt im Report (Medienwiedergabe mit 3 min Pause), Soll/Ist-Tabelle vorformuliert.
  4. `clock-drift (C2)`: Live-Aufnahme ≥ 60 min mit gleichzeitiger Wiedergabe einer Referenzdatei; Harness misst danach `mic.wav`/`system.wav`-Dauern via ffprobe und rechnet Drift/h aus; PASS < 500 ms/h Differenz der Zeitbasen-Länge gegen Wanduhr. (Halbautomatisch: Start/Stop remote via UI, Messung automatisch; als solcher im Report gekennzeichnet.)
  5. `crash-recovery`: Aufnahme starten (UI), Prozess hart killen (`Stop-Process`), App neu starten → Meeting `ready`, WAVs per hound/ffprobe lesbar, Segmente bis zum Kill vorhanden.
  6. `retention`: Meeting importieren, Protokoll mit Mock-Provider erzeugen (Harness startet den Mock-LLM aus den Rust-Tests als eigenständiges kleines PS-HTTP-Listener-Skript ODER setzt Policy `Days(0)`), App-Neustart → Audio-Dateien gelöscht, DB-Pfade genullt, Transkript/Protokoll unverändert vorhanden.
  7. `log-privacy`: Nach `import-wav` + Protokoll: kein Wort aus dem Fixture-Text im `handy.log` (Muster M3 `log-privacy`).
  8. `consent-gate`: `meetings_start` ohne Consent via Dev-Konsole → Fehler `consent_required`, keine Meeting-Row.

- [ ] **Step 3: Lauf + Evidence** — Harness vollständig laufen lassen; `docs/m8-evidence/harness-report.md` mit Szenario-Matrix, Kennzahlen (Drift/h, Segmentlatenz, Importdauer je Format) und offenen manuellen Punkten (Hörtest Loopback-Qualität) füllen.

- [ ] **Step 4: Commit** — `test(meetings): m8 acceptance harness — silence timeline, clock drift, crash recovery, retention, log privacy`

---

## Self-Review (durchgeführt)

**Spec-Coverage:** A1 Consent → T1 (Schema), T8 (Gate + Indikator), T13 (Dialog), T15 (Szenario 8). A2 Retention → T12 + T15/6. B Segment-Granularität → T6 (`granularity 'segment@1'` in T1-Schema). C1 Silence → T3 + T15/3. C2 zwei Dateien/QPC → T2/T4/T8 + T15/4. C3 Format i16/16k → T2. Import → T9. Redeanteile deterministisch → T10 (LLM bekommt sie nur als Fakten, T11-Prompt-Test). Protokoll-Schema/Validator/Ein-Sprecher-Ausnahme → T11. Langzeit/Crash → T2 + T8 + T15/5. Log-Privacy → Global + T15/7. UI-Muster/i18n → T13/T14. Diktat-Pfad unangetastet → T5/T6 (bewusste Duplizierung, in Code-Kommentaren begründet). NICHT in M8 (per Spec): Diarisierung (M9), ProseMirror (M10), Sync (M11), AEC, Hotkey für Meetings (YAGNI, UI reicht).

**Offene Realitäts-Checks für den Implementierer (keine Platzhalter, sondern markierte Messpunkte):** exakte `wasapi`-Crate-API-Namen in T4 gegen die Crate-Doku verifizieren (Konzept und Datenfluss stehen fest); ob `chunk_text` in `summarizer.rs` bereits `pub` ist (sonst in T11 sichtbar machen).

**Typ-Konsistenz:** `StoredSegment` (T1) wird von T6-Ausgabe (`TimedSegment` + channel/index im Manager T8 angereichert), T9, T10, T11 identisch verwendet; `MeetingEvent` einheitlich T8/T13/T14; `MeetingAudioRetention` T12 ↔ Settings ↔ Frontend-Updater.
