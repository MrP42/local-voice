# Design: Besprechungsprotokolle in Local Voice AI (M8–M12)

**Datum:** 2026-08-19 · **Status:** Review eingearbeitet (2026-08-19), freigegeben für M8-Planung
**Vorbild-Analyse:** fastrepl/anarlog (Nachfolger von Hyprnote), Commit-Stand 2026-08-19,
analysiert durch drei Explore-Agents (Pipeline, Editor/Sync/Lizenz, eigene App-Karte).

---

## 1. Ziel

Local Voice AI wird um ein Subsystem **Meetings** erweitert: Besprechungen werden **live**
(Mikrofon + System-Audio) oder **nachträglich** (Audio-/Videodateien, Untertitel) erfasst,
lokal transkribiert, Sprechern zugeordnet und zu einem **standardisierten Protokoll**
verdichtet (Zusammenfassung, Scope, Sprecher, Redeanteile, Entscheidungen, Aufgaben,
Next Steps, Follow-Ups). Transkript und Protokoll sind in einem **WYSIWYG-Editor**
bearbeitbar. Lokale Verarbeitung hat Vorrang; ein **komplett lokaler Modus** (STT lokal,
LLM via Ollama) ist Erstklasse-Bürger. Später: **Sync über einen Server auf IONOS
Webhosting** (User-Registrierung, alle Geräte, verlustsicher) und eine **iOS-App**.

**Rechtsrahmen als Produktanforderung (§ 201 StGB):** Das Aufzeichnen nichtöffentlich
gesprochener Worte ohne Einwilligung aller Beteiligten ist in Deutschland strafbar.
Die App behandelt Einwilligung deshalb nicht als Doku-Fußnote, sondern als Feature:
Bestätigungsdialog vor jedem Aufnahmestart, nicht unterdrückbarer OS-weit sichtbarer
Aufnahmeindikator (Tray + Overlay, unabhängig von `overlay_style`), Einwilligungsvermerk
im Datenmodell (`consent_confirmed_at`), und derselbe Vermerk im Import-Pfad — eine
fremde Aufnahme belegt keine eigene Einwilligung. **Hinweis für den dienstlichen
Einsatz (kein Code):** personenbezogene Redeanteil-Auswertung ist mitbestimmungspflichtig
(z. B. § 74 LPVG BW); die Doku weist Nutzer darauf hin.

Nicht-Ziele (bewusst): Meeting-Bots, die Calls beitreten (anarlog-Enterprise-Terrain);
Echtzeit-Kollaboration mehrerer Nutzer am selben Dokument; Kalender-Integrationen in v1;
**mehrsprachige Meetings** (Sprecherwechsel DE/EN innerhalb einer Sitzung) in v1 —
englisches Fachvokabular in deutscher Rede deckt Parakeet V3 ab, echte Sprachmischung
wird erst nach M8-Praxiserfahrung bewertet.

## 2. Was die anarlog-Analyse ergab (Kurzfassung)

**Lizenz:** Kern MIT (© Fastrepl, Inc.) — Code-Übernahme inkl. kommerzieller Nutzung
erlaubt, Pflicht ist nur der Lizenz-/Copyright-Hinweis (THIRD-PARTY-NOTICES). Tabu:
`enterprise/**` (Meeting-Bots) und das vendorte `sqlite-sync`-Binary (Drittprodukt von
SQLite Cloud **ohne** Lizenzdatei im Repo — nicht übernehmen, nur das Muster).
Modell-Lizenzen (pyannote/wespeaker-ONNX) sind je Modell separat zu prüfen.

**Übernehmenswerte Muster (mit Beleg im anarlog-Repo):**

| Muster | Beleg |
|---|---|
| Zwei-Kanal-Capture: Mic + Loopback getrennt bis zur Engine (`CaptureFrame{raw_mic, raw_speaker}`); Kanal = Sprecherklasse (`DirectMic/RemoteParty/MixedCapture`) → „ich vs. andere" gratis und 100 % korrekt | `crates/audio`, `crates/audio-actual/src/speaker/windows.rs` (WASAPI-Loopback via `wasapi`-Crate) |
| Delta-Protokoll für Live-Transkripte: `TranscriptDelta{new_words, replaced_ids, partials}` + `WordState::{Final,Pending}`, sequenzierte DB-Deltas → crash-sicher, idempotent | `crates/transcript`, Migration `transcript_live_deltas` |
| Sprecher-Identität vierstufig: Kanal → lokale Diarisierung (pyannote-ONNX) → Voiceprint-Wiedererkennung (Vektoren nie synchronisiert) → optionale LLM-Namenszuordnung mit Beweiszitat, confidence ≥ 0.9 | `crates/pyannote-local`, `crates/voiceprint`, `speaker-attribution.ts` |
| Protokoll-Abschnitte sind **Daten** (Template-Tabelle mit `sections_json`), nicht Prompt-Hardcode; Validator prüft Sektions-Treue und triggert Retry | `templates`-Migrationen, `enhance-validator.ts` |
| Import = derselbe Pfad wie Aufnahme (Datei → Attachment → Batch-STT → Enhance), nur `channel = MixedCapture` | `useUploadFile.ts`, `listener2-core/src/batch/` |
| Sync: **nur eine verschlüsselte Tabelle** (`e2ee_records`) verlässt das Gerät; Fachtabellen bleiben lokal; Feld-LWW mit uhrfreier Ordnung; Server sieht nur Blobs | `crates/db-app/src/e2ee/`, `cloudsync.rs` Z. 100–129 |
| Krypto: XChaCha20-Poly1305 + HKDF-SHA256, versionierte Domain-Separation, Recovery-Key als Wurzel | `crates/e2ee` |
| Mobile: Rust-Datenkern (DB, Migrationen, Sync, Krypto) via **UniFFI** an natives UI — NICHT Tauri mobile, nicht geteiltes UI | `crates/mobile-bridge`, `apps/mobile` (Expo/RN) |
| Editor: ProseMirror pur (MIT), Speicherformat ProseMirror-JSON; Drei-Tab-Modell raw/enhanced/transcript statt Split-View; Transkript-Tab ist **kein** ProseMirror, sondern Segment-Renderer mit Edit-Modus | `packages/editor`, `note-input/` |

**Bekannte anarlog-Fallen, die wir vermeiden:** fremdes Sync-Binary im kritischen Pfad;
Markdown-Konvertierung doppelt (TS + Rust); irreführende Feldnamen (`raw_md` enthält JSON);
Feld-LWW verliert bei parallelem Offline-Editieren desselben Dokuments eine Fassung
komplett (Abschnitt „M11" löst das über `parent_revision`-Kausalität + Konflikt-Kopien).

## 3. Leitentscheidungen

1. **Lokal-first wie bisher:** SQLite ist kanonisch, die App funktioniert vollständig
   ohne Konto/Netz. Sync ist ein optionales Feature obendrauf (anarlog-Prinzip).
2. **Kanaltrennung vor Diarisierung:** M8 liefert Sprechertrennung „ich vs. Gegenseite"
   über Mic/Loopback; echte Diarisierung (mehrere Remote-Sprecher) folgt in M9 als
   ONNX-Stufe. So gibt es früh ein korrektes, nützliches Ergebnis.
3. **Protokoll als Structured Output:** Das LLM liefert JSON gegen ein festes Schema
   (`llm_client::send_chat_completion_with_schema` existiert). Redeanteile werden
   **deterministisch** aus Segmentdauern berechnet, nie vom LLM geschätzt.
4. **Editor: ProseMirror** (MIT, keine TipTap-Abhängigkeit), Drei-Tab-Modell.
   Speicherformat `body_format='prosemirror_json@1'` von Tag 1 versioniert.
5. **Sync nach dem E2EE-Blob-Muster auf eigenem Server:** anarlog-*Architektur*
   (eine verschlüsselte Record-Tabelle, Feld-LWW, Token-Auth), aber als **eigene,
   schlanke PHP-API auf IONOS Webhosting Pro** im Stil des vorhandenen WAI-Portals
   (PHP 8.x + MySQL, kein Composer/Node). Kein WebSocket — Polling-Pull/Push
   (30-s-Intervall, sofortiger Push nach lokalem Schreiben). Der Server sieht
   ausschließlich verschlüsselte Blobs → datenschutzfreundlich und Shared-Hosting-tauglich.
6. **iOS nach dem UniFFI-Muster:** meetings-Kern (Datenmodell, Sync, Krypto) wird als
   eigenständige Rust-Crate geschnitten, damit er später via UniFFI an eine native
   iOS-App bindet. Externe Voraussetzungen siehe Abschnitt 9/M12.
7. **Datenschutz-Disziplin wie D9:** Transkript-/Protokoll-Klartext erscheint nie in
   Release-Logs; Voiceprint-Vektoren (M9) sind `local_only` und werden nie synchronisiert.
8. **Lizenz-Disziplin wie D1/M0.1:** Übernommener anarlog-Code wird mit Herkunft
   (Repo, Commit, Pfad, MIT-Notice) in THIRD-PARTY-NOTICES dokumentiert;
   `cargo deny check licenses` bleibt grün; keine GPL-Modelle/Crates.
9. **Kanonische Transkript-Granularität v1 = Segment (Satz), ehrlich benannt.**
   Entscheidung des Widerspruchs Wort- vs. Segmentebene (Review-Punkt B) zugunsten
   der Segmentebene: `segments_json` (nicht `words_json`), Formatfeld
   `granularity: "segment@1"` / `word_level: false`. Begründung: Der whisper.cpp-
   DTW-Pfad für Wort-Timestamps deckt nur Whisper-Modelle mit Alignment-Heads ab —
   das Default-Modell **Parakeet V3** und der übrige Katalog blieben außen vor; die
   Satz-Segmente des vorhandenen `SentenceSegmenter` sind mit typisch 3–10 s fein
   genug für Klick-ins-Audio (M10) und Redeanteile. Konsequenz für M9, explizit:
   **Diarisierung auf Segmentgrenzen; Feinalignment/Wortebene später** als
   versioniertes Format-Upgrade (`word_level: true`), ohne Migration der Semantik.

## 4. Dekomposition (jedes Teilprojekt: eigener Plan → Umsetzung → Evidence)

### M8 — Meetings-Fundament (Windows, komplett lokal) ← **Start**
Der vertikale Pfad: aufnehmen → live mitlesen → Protokoll → Liste.

- **Einwilligung (A1):** Bestätigungsdialog vor Aufnahmestart („alle Beteiligten
  informiert/einverstanden"), Zeitpunkt → `meetings.consent_confirmed_at`;
  nicht unterdrückbarer Aufnahmeindikator im Tray **und** Overlay (erscheint auch bei
  `overlay_style: none`, wie der `notice`-Zustand aus M3); Import-Dialog mit demselben
  Vermerk.
- **Langzeit-Aufnahme:** inkrementeller WAV-Writer (Streaming-to-Disk statt
  RAM-`Vec<f32>`), Pause/Resume (`RecordingState` erweitern), Absturzsicherung
  (Wiederaufnahme aus teilgeschriebener Datei + persistierten Live-Deltas).
- **Zwei-Kanal-Capture (Hauptrisiko M8, Punkte C1–C3):**
  - Mikrofon über vorhandenen `AudioRecorder` (cpal); **neu** WASAPI-Loopback als
    zweite Quelle (`wasapi`-Crate wie anarlog oder `IAudioClient` mit
    `AUDCLNT_STREAMFLAGS_LOOPBACK` über die vorhandene `windows`-Crate).
  - **Zwei getrennte WAV-Dateien** (`mic.wav`, `system.wav`), keine Stereo-Datei:
    Mic- und Render-Endpoint laufen auf verschiedenen Hardware-Clocks (typisch
    10–100 ppm Drift = 36–360 ms/h); je Datei eigene Zeitbasis, je Buffer
    QPC-Timestamp protokolliert. Segmente tragen `channel` + Zeit in ihrer
    Kanal-Zeitbasis; die Protokoll-Pipeline arbeitet je Kanal und merged über
    QPC-verankerte Zeiten.
  - **Silence-Handling Loopback:** WASAPI-Loopback liefert bei Stille keine Buffer
    bzw. `AUDCLNT_BUFFERFLAGS_SILENT`. Zeitachse wird **nie** aus dem Framecount
    abgeleitet, sondern aus Device-Position/QPC; Lücken werden beim Schreiben mit
    Silence gepaddet. Pflicht-Testfall: 10-min-Fixture mit 3 min Stille im
    System-Kanal → keine Zeitachsen-Kompression, Drift < definierter Schwelle.
  - **Aufnahmeformat:** i16 / 16 kHz / mono **je Kanal** (was STT ohnehin braucht),
    Downsampling beim Schreiben. f32/48k/stereo läge nach ~3 h am 4-GB-RIFF-Limit;
    i16/16k mono reicht ~17 h je Datei und spart I/O.
  - AEC übernehmen wir in v1 **nicht** — bei Headset-Nutzung (Regelfall) ist
    Übersprechen gering; Kanaltrennung macht Echo zum Qualitäts-, nicht zum
    Zuordnungsproblem. Neubewertung nach M8-Praxistest.
- **`transcribe_segments()`:** zweite Signatur am `TranscriptionManager`, die
  `Vec<Segment{start_ms, end_ms, text}>` liefert (Engines liefern Timestamps bereits,
  `transcribe()` wirft sie heute weg). Live-Pfad: `SentenceSegmenter`-Ausgabe geht in
  den Meeting-Store statt in `paste`.
- **Retention (A2):** Setting `meeting_audio_retention` mit Default **„Rohaudio nach
  Protokollerstellung löschen"** (Alternativen: behalten X Tage / immer behalten);
  `meetings.audio_retention_until` steuert den Ablauf; Soft-Delete eines Meetings
  löscht die Audio-Dateien **hart** (Tombstones dürfen keine verwaisten WAVs
  hinterlassen).
- **Datenmodell:** neue DB `meetings.db` (eigener Sync-Scope, `rusqlite_migration`),
  Schema in Abschnitt 5.
- **Import nachträglicher Aufnahmen:** Datei-Dialog/Drag&Drop → Einwilligungsvermerk →
  `media::ensure_wav()` (ffmpeg-Pfad existiert, 13 Formate inkl. mp4/m4a/mkv) →
  Batch-Transkription in Blöcken mit Fortschritts-Event → gleicher
  Weiterverarbeitungspfad, `channel = MixedCapture`. Zusätzlich VTT/SRT-Import als
  fertiges Transkript. **Bekannte Einschränkung:** ohne Diarisierung (erst M9) hat
  ein Import genau einen Sprecher — die Protokoll-Sektionen Sprecher/Redeanteile
  bleiben dann leer, der Validator wertet das **nicht** als Pflichtfeldfehler.
- **Protokoll-Erzeugung:** `minutes.rs` nach dem Muster von `summarizer.rs`
  (Map-Reduce > 16 k Zeichen vorhanden) mit Structured-Output-Schema (Abschnitt 6),
  Provider wie gehabt (Ollama lokal als Default, Cloud per API-Key optional).
  Template-Sektionen als Daten (Tabelle `meeting_templates`), Default
  „Standardprotokoll". Validator prüft Sektionstreue, ein Retry.
- **UI:** neuer Sidebar-Bereich `meetings` nach dem 7-Schritte-Muster (Sidebar-Config,
  i18n in 21 Locales, specta-Commands/Events). v1-Ansichten: Aufnahme-Card
  (Pegel je Kanal, Dauer, Pause/Stop, Consent-Status), Live-Transkript (Segmentliste),
  Meeting-Liste (Klon-Vorlage `HistorySettings.tsx`), Protokoll-Ansicht
  (Markdown-Render, Kopieren/Export als .md). Der echte Editor kommt in M10 —
  bis dahin Textarea-Korrektur je Segment.
- **Evidence:** Harness-Szenarien „60-min-Fixture aufnehmen → Protokoll mit allen
  Pflichtsektionen", **Loopback-Stille (C1-Fixture)**, **Clock-Drift-Messung über
  ≥ 60 min (C2)**, Crash-Recovery, Import-Matrix (wav/mp3/m4a/mp4/vtt),
  Retention-Löschung inkl. Soft-Delete-Kaskade.

### M9 — Sprecher: Diarisierung, Namen, Redeanteile
- pyannote-ONNX-Stufe (Segmentierung + Embeddings) für den Remote-/MixedCapture-Kanal,
  **auf Segmentgrenzen** (Leitentscheidung 9); anarlog `pyannote-local`/`segmentation`/
  `embedding` (MIT) als Code-Vorlage; ONNX-Runtime ist über `transcribe-rs`/`vad-rs`
  bereits im Projekt. Modell-Lizenzen vorab prüfen (M0.1-Standard), Modelle in den
  Katalog + Download-Pfad aufnehmen.
- Sprecherverwaltung: Personen anlegen/zuordnen (UI im Transkript), stabile
  „Sprecher 1/2/…"-Labels, optionale LLM-Namenszuordnung (nur mit Beweiszitat, ≥ 0.9).
- Redeanteile: deterministisch aus Segmentdauern; Anzeige als Liste + Balken, fließt
  in das Protokoll-JSON ein.
- Optional (Messung entscheidet): Voiceprint-Wiedererkennung über Meetings hinweg.
  **Speicher-Realität Windows:** Credential Manager begrenzt einen Blob auf ~2,5 KB;
  ein 512-dim-f32-Embedding ist base64-kodiert bereits ~2,7 KB. Deshalb: Embeddings
  **f16-quantisiert** ablegen oder verschlüsselte lokale Datei mit Schlüssel im
  Keyring; in jedem Fall `local_only`, nie im Sync.

### M10 — WYSIWYG-Editor
- ProseMirror-Editor als neue Komponente (Schema: Absätze, Überschriften, Listen,
  Task-Listen, Tabellen; Custom-Node für Transkript-Verweise), Debounce-Persistenz
  (500 ms) + `flushPendingChanges` bei Tab-/Meeting-Wechsel, IME-Composition beachten.
- Drei Tabs pro Meeting: **Notizen** (eigene Mitschrift, ProseMirror) · **Protokoll**
  (generiert, ProseMirror, nachbearbeitbar; Regenerieren erzeugt neue Version statt
  Überschreiben) · **Transkript** (Segment-Renderer mit Edit-Modus, Sprecher-Dropdown,
  Klick-ins-Audio auf Segmentebene über die M8-Timestamps).
- Markdown-Konvertierung **einmal** implementieren (TS), Export .md; DOCX/PDF-Export
  über vorhandene Dokumentwege später.

### M11 — Sync-Server (IONOS) + Multi-Device
- **Client (Rust, eigenständige Crate `meetings-core`):** E2EE-Schicht nach
  anarlog-Muster — Feld-Verschlüsselung XChaCha20-Poly1305/HKDF, `records`-Tabelle,
  Feld-LWW `(key_id, revision, writer_id, payload_hash)`, Dirty-Row-Trigger,
  Recovery-Key (einmal angezeigt, Warnung). Sync-Schleife: Push nach lokalem
  Schreiben, Pull per Polling (30 s), pausiert während Aufnahme, Chunking großer
  Payloads.
  - **Konflikt-Erkennung (D1):** Die LWW-Ordnung allein ist total und trägt keine
    Kausalität — sie kann „parallel entstanden" nicht von „normales Update"
    unterscheiden. Deshalb enthält der **verschlüsselte Payload** zusätzlich
    `parent_revision` (die Revision, auf der der Schreiber aufgesetzt hat).
    Echter Konflikt = ein Record verliert das LWW **und** sein `parent_revision`
    ist kein Vorfahre des Gewinners → die unterlegene Dokument-Fassung wird als
    **Konflikt-Kopie** (eigenes `meeting_documents`-Row, gekennzeichnet) erhalten
    statt verworfen. Kostet den Server nichts; gehört von Anfang an ins
    Payload-Format (nachträglich wäre es eine E2EE-Formatmigration).
  - **Nonce-Invariante (festgehalten):** Die `payload_hash`-Idempotenz funktioniert
    nur, weil XChaCha20 **zufällige** Nonces nutzt — deterministische Nonces würden
    echte Updates wegdeduplizieren. Diese Invariante wird im Code am Nonce-Erzeuger
    dokumentiert und per Test abgesichert.
  - **Key-Ableitung (D3, Format-Slot jetzt reserviert):** Master-Key zufällig,
    **doppelt gewrappt** — (a) KEK aus Passphrase via Argon2id, (b) Recovery-Key.
    Passwortwechsel ist dann ein Rewrap statt Re-Encryption aller Records.
    Detailentscheidung im M11-Plan; das Record-/Keyring-Format sieht die zwei
    Wrap-Slots von Beginn an vor.
- **Server (PHP 8.x + MySQL auf IONOS Webhosting Pro):** schlanke REST-API im
  WAI-Portal-Stil (abhängigkeitsfrei, dialekt-portable Migrationen, eigenes
  Test-Harness). Endpunkte: Registrierung/Login (Vorlage WAI-Portal-IAM),
  Geräte-Registrierung (Limit z. B. 5), `POST /records` (append, idempotent per
  `payload_hash`), `GET /records?since=<seq>` (Cursor), Quota/Rate-Limit.
  Server speichert **nur** `(user_id, seq, record_blob, hash, created_at)`.
  - **Cursor-Korrektheit (D2):** MySQL-AUTO_INCREMENT vergibt `seq` bei Insert-Beginn;
    committet T2 (seq 10) vor T1 (seq 9), sieht ein Pull 10, setzt den Cursor, und
    9 wird nie ausgeliefert — stiller Datenverlust. Lösung: `seq`-Vergabe **erst beim
    Commit** über eine eigene Sequenztabelle mit `SELECT … FOR UPDATE` (bevorzugt),
    alternativ Auslieferung nur für `created_at < NOW() - 5s`. Pflicht-Testfall im
    PHP-Harness: nebenläufige Writer, kein Record fehlt.
  - **users-Struktur analog WAI-Portal:** Tabellen/Feldzuschnitt der Registrierung
    orientieren sich am Portal-IAM, damit ein späteres **Account-Linking gegen das
    WAI-Portal offen bleibt** (bewusst nicht v1, aber nicht verbaut).
  - Deployment über das vorhandene SFTP-Skript-Muster; eigene Subdomain
    (z. B. `sync.wolffappliedai.de`). Schwester-App neben dem Portal (bestätigt).
- **Audio-Sync:** in v1 **nicht** enthalten (Größe/Quota Shared Hosting) — aber der
  Record-Typ `attachment_chunk` und dessen Chunk-Format (Record-Header:
  `{attachment_id, chunk_index, chunk_count, content_sha256}`) werden im M11-Schema
  **jetzt reserviert**, damit Audio-Sync später ohne Formatbruch nachrüstbar ist.
- Witness-Log (Rollback-Erkennung) ist v2 — Schema so anlegen, dass es nachrüstbar
  ist (append-only `seq` existiert ohnehin).

### M12 — iOS-App
- Rust-Kern (`meetings-core`: Schema, Migrationen, E2EE, Sync, Protokoll-Typen) via
  **UniFFI** als Swift-Package; UI nativ (SwiftUI) oder Expo/RN — Entscheidung im
  M12-Plan nach Prototyp.
- iOS-Realität: kein System-Loopback → Anwendungsfälle sind **Präsenz-Meetings per
  Mikrofon**, Import von Dateien, und Lesen/Bearbeiten synchronisierter Protokolle.
  STT lokal via whisper.cpp (Metal/CoreML) oder Apple SpeechAnalyzer; Protokoll-LLM
  auf dem Gerät nur eingeschränkt → Default: Protokoll am PC oder per eigenem API-Key.
- **Externe Voraussetzungen (Blocker, vor M12 klären):** Apple Developer Program
  (99 USD/Jahr) und eine macOS-Build-Umgebung (Mac-Hardware oder macOS-CI, z. B.
  GitHub Actions). Ohne beides kein iOS-Build — Patrick arbeitet auf Windows.
- **Risiko-Hinweis zur Schätzung:** erste native App des Projekts + UniFFI-Neuland +
  Code-Signing/Provisioning — die Bandbreite (400–700 kTok) ist bewusst breit und
  kann trotzdem reißen; vor M12 wird sie gegen den dann realen `meetings-core`-Schnitt
  neu geschätzt.

## 5. Datenmodell (meetings.db, M8-Stand)

Vereinfachtes anarlog-Schema; überall `id` (ULID als TEXT), `created_at`, `updated_at`,
`deleted_at` (Soft-Delete, Sync-Voraussetzung).

```
meetings           (title, status: recording|processing|ready, started_at, ended_at,
                    language, source: live|import|subtitle,
                    mic_audio_path, system_audio_path, duration_ms,
                    consent_confirmed_at, audio_retention_until, metadata_json)
meeting_documents  (meeting_id, kind: note|minutes|minutes_conflict, template_id,
                    title, body_format: 'prosemirror_json@1', body,
                    generation_metadata_json, version)
transcripts        (meeting_id, provider, model, language,
                    granularity: 'segment@1', segments_json,
                    speaker_hints_json, content_revision)
transcript_deltas  (transcript_id, sequence, delta_json)      -- Live, crash-sicher
speakers           (meeting_id, channel, speaker_index, human_id, display_name,
                    consent_state NULL|confirmed|declined)     -- optional befüllt
humans             (name, email, memo)
action_items       (meeting_id, text, assignee_human_id, due_at,
                    status: todo|done, source: llm|manual, kind: task|follow_up)
meeting_templates  (title, sections_json, pinned)
```

`segments_json`-Segmentschema: `{text, start_ms, end_ms, channel, speaker_index?}` —
Zeiten in der Zeitbasis des jeweiligen Kanals (QPC-verankert, siehe M8/C2);
`word_level: false` bis zum Format-Upgrade.

**`humans` liegt bewusst in `meetings.db`** (Entscheidung Review-Punkt E): Personen
gehören fachlich zu Meetings und sollen in M11 mitsynchronisieren; `history.db` bleibt
reiner Diktat-Verlauf. Die frühere Kennzeichnung „app-weit" ist damit korrigiert zu
„meetings-weit, geräteübergreifend ab M11".

M11 ergänzt `records` (E2EE) + Dirty-Trigger je Tabelle; Audio bleibt lokal
(Record-Typ `attachment_chunk` reserviert, siehe M11).

## 6. Standardprotokoll (Structured-Output-Schema)

Deterministisch vorab berechnet und ins Prompt-Kontextfeld gegeben: Datum/Dauer,
Sprecherliste, Redeanteile (% je Sprecher aus Segmentdauern). Das LLM füllt:

```json
{
  "summary":      "3–8 Sätze Ergebnis-Zusammenfassung",
  "scope":        "Anlass/Thema/Abgrenzung der Besprechung",
  "decisions":    [{"text", "context"}],
  "tasks":        [{"text", "assignee?", "due?"}],
  "next_steps":   [{"text", "owner?"}],
  "follow_ups":   [{"text", "reason"}],
  "open_questions": [{"text"}]
}
```

Rendering zum Protokoll-Dokument (ProseMirror-JSON) erfolgt in Rust aus JSON + den
deterministischen Kopfdaten — das LLM formatiert kein Gesamtdokument (verhindert
erfundene Teilnehmer/Zahlen; Validator prüft zusätzlich Schema + leere Pflichtfelder).
Sektionen/Reihenfolge kommen aus `meeting_templates.sections_json`; „Standardprotokoll"
enthält alle obigen Sektionen, weitere Vorlagen (Jour fixe, Retro, Kundengespräch)
später. **Validator-Ausnahme:** Bei Ein-Sprecher-Transkripten (Import ohne
Diarisierung, M8) sind Sprecher/Redeanteile zulässig leer.

## 7. Fehlerbehandlung & Datensicherheit

- Aufnahme: Schreiben auf Platte in 1-s-Blöcken; App-Crash verliert maximal den letzten
  Block; beim Start werden verwaiste `recording`-Meetings erkannt und finalisiert.
- Aufnahmeindikator: OS-weit sichtbar (Tray + Overlay), nicht unterdrückbar (A1).
- Retention: Rohaudio-Löschung nach Policy (A2); Soft-Delete kaskadiert hart auf
  Audio-Dateien.
- STT-/LLM-Fehler: Meeting bleibt im Zustand `processing` mit sichtbarem Fehler und
  Retry-Knopf; niemals stiller Verlust. Auto-Retry mit Backoff wie anarlog (30 s … 15 min).
- Logs: nur Längen/Gates, nie Inhalte (D9-Standard gilt unverändert).
- Sync (M11): lokal kanonisch; Server ist Backup + Verteiler; Konflikt-Kopien über
  `parent_revision`-Kausalität statt stillem Verwerfen; Recovery-Key-Verlust heißt
  „kein neues Gerät", nicht Datenverlust.

## 8. Teststrategie

Wie im Projekt etabliert: Entscheidungslogik als reine Funktionen mit Unit-Tests
(Segment-Zuordnung, LWW-Ordnung inkl. `parent_revision`-Vorfahren-Prüfung,
Schema-Validator, Redeanteil-Berechnung, Retention-Ablauf); Mock-Server auf
`TcpListener` für LLM-Aufrufe; In-Memory-SQLite für Store-Tests; Fixtures
(zweikanalige Meeting-WAVs, Stille-Fixture C1, VTT) + PowerShell-Abnahme-Harness je
Milestone mit Evidence unter `docs/m<N>-evidence/`. Pflicht-Szenarien M8:
Loopback-Stille (C1), Clock-Drift-Messung ≥ 60 min (C2), Crash-Recovery,
Retention-Kaskade. PHP-Server (M11): WAI-Portal-Test-Harness-Muster (`tests/run.php`)
inkl. Idempotenz-, Cursor- und **Nebenläufigkeits-Tests (D2: kein Record darf
übersprungen werden)**.

## 9. Annahmen — Review-Stand 2026-08-19

Alle sechs ursprünglichen Annahmen sind von Patrick **bestätigt**:

1. Reihenfolge M8 → M9 → M10 → M11 → M12.
2. Kein AEC in v1 (Kanaltrennung macht Echo zum Qualitäts-, nicht zum
   Zuordnungsproblem); Neubewertung nach M8-Praxistest.
3. Kein Audio-Sync in v1 — **mit Auflage:** Record-Typ + Chunk-Format im M11-Schema
   jetzt reservieren (eingearbeitet, siehe M11).
4. Sync-Server als Schwester-App mit eigener Registrierung — **mit Auflage:**
   `users`-Struktur analog WAI-Portal, Account-Linking bleibt offen (eingearbeitet).
5. iOS-UI-Technologie erst im M12-Plan.
6. Fish-Speech/TTS bleibt unberührt; Meetings nutzt die GPU nur für STT/Ollama.

Zusätzlich entschieden (Review-Punkt B): Segmentebene als kanonische Granularität v1
(Leitentscheidung 9).

## 10. Aufwandsschätzung (CLAUDE.md-Pflicht; Zwischenstand bei 50 %/80 % je Etappe)

| Etappe | Inhalt | Schätzung |
|---|---|---|
| M8 | Fundament: Capture (inkl. C1–C3: Silence-Padding, Clock-Drift, Format + Fixtures/Drift-Messung), Langzeit-Recording, Consent + Retention, Segmente, Import, Protokoll, UI | **400–650 kTok** |
| M9 | Diarisierung, Sprecher, Redeanteile | 150–250 kTok |
| M10 | ProseMirror-Editor, drei Tabs, Export | 200–350 kTok |
| M11 | E2EE-Sync-Client + PHP-Server + Abnahme | 250–400 kTok |
| M12 | iOS (UniFFI-Kern, native App) — Bandbreite bewusst breit, Risiko-Hinweis siehe M12; Neuschätzung vor Start | 400–700 kTok + externe Kosten (Apple, ggf. Mac/CI) |
| **Summe** | | **grob 1,4–2,35 MTok**, verteilt über mehrere Sessions/Etappen |

---

**Review-Gate:** Design am 2026-08-19 von Patrick reviewt; Blocker A1/A2, Widerspruch B,
Risiken C1–C3, Sync-Korrekturen D1–D3 und Punkte E/G eingearbeitet. Freigegeben für
den M8-Implementierungsplan (writing-plans-Skill).
