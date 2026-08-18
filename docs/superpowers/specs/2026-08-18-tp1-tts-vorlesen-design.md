# TP1 „Vorlesen“ — Fish-Speech-TTS-Fundament für Sprechstift

Datum: 2026-08-18 · Status: freigegeben (Design-Dialog) · Milestone: M4

## Ziel

Sprechstift kann Text vorlesen: per globalem Hotkey die Zwischenablage, per neuem
Bereich im Hauptfenster beliebigen Text. Die Sprachsynthese läuft vollständig
lokal über den bereits installierten Fish-Speech-S2-Pro-Server
(`C:\AI\fish-speech`, API `http://127.0.0.1:8080`). TP1 ist das Fundament für
Stimmen-Klonen (TP2), Audio-Übersetzung (TP3) und Stimmwechsler (TP4).

Nicht-Ziele (bewusst ausgeklammert): Streaming-Playback (TP5), Stimmen-
Verwaltung/Referenzstimmen (TP2), Vorlesen markierten Texts ohne Kopieren,
andere Ausgabeformate als WAV, Änderungen am Fish-Server selbst.

## Kontext

- Sprechstift ist ein Handy-Fork: Tauri 2 (Rust-Backend, React/TS-Frontend),
  Manager-Pattern (`managers/audio|model|transcription|history`),
  Command-Event-Architektur, Settings via tauri-plugin-store, i18n-Pflicht.
- Der Fish-Server belegt geladen ~17,3 GB VRAM und braucht ~68 s zum Start
  (bei freiem VRAM). Real-Time-Factor ohne compile: ~6.
- Playback-Infrastruktur (rodio) existiert in `audio_feedback.rs`;
  Clipboard-Zugriff in `clipboard.rs`; Hotkey-/Action-Infrastruktur in
  `actions.rs`/Shortcut-Modulen; Selftest-Infrastruktur in `selftest.rs`.

## Architektur

### Neuer Manager: `managers/tts.rs` (TtsManager)

Zustandsmaschine:

```
Stopped ──ensure_server()──▶ Starting ──health ok──▶ Ready ──speak()──▶ Speaking
   ▲                            │ Timeout/Fehler        │  ▲               │
   └────────── idle-timeout ────┴──────▶ Error ◀────────┘  └── fertig ─────┘
```

- **Prozess-Management:** Spawnt den Server on-demand als Kindprozess:
  `<fish_dir>\.venv\Scripts\python.exe tools/api_server.py --listen 127.0.0.1:<port>`
  mit Arbeitsverzeichnis `<fish_dir>`, unter Windows mit CREATE_NO_WINDOW und
  `HF_HUB_DISABLE_TELEMETRY=1`. Das Prozess-Handle bleibt im Manager; App-Exit
  und Idle-Timeout beenden den Prozess zuverlässig (kein Orphan). Erkennt einen
  bereits extern laufenden Server (Health-Check vor Spawn) und nutzt ihn dann,
  ohne ihn zu besitzen (kein Kill fremder Prozesse beim Exit).
- **Health-Polling:** Nach Spawn `GET /v1/health` bis `{"status":"ok"}`,
  Poll-Intervall 2 s, Timeout 180 s. Fortschritt als Event (verstrichene
  Sekunden), ab 120 s mit VRAM-Hinweis.
- **Sprechen:** `POST /v1/tts` (JSON: `text`, `format:"wav"`, `seed`) als
  Non-Streaming-Request (Timeout 300 s), Antwort-WAV via rodio abspielen.
  Nur ein Sprechauftrag zugleich; ein neuer Auftrag beendet den laufenden
  vollständig — Playback stoppt und eine noch offene Server-Antwort wird
  verworfen —, bevor der neue Request startet (letzter gewinnt). `cancel()`
  macht dasselbe ohne neuen Auftrag.
- **Idle-Timer:** Nach `idle_minutes` ohne speak()-Aufruf wird ein selbst
  gestarteter Serverprozess beendet (Event an UI). Extern gestartete Server
  werden nie beendet.

### Tauri-Commands (`commands/tts.rs`)

`tts_speak_text(text)`, `tts_speak_clipboard()`, `tts_cancel()`,
`tts_server_start()`, `tts_server_stop()`, `tts_server_status()` →
Status/Fortschritt zusätzlich als Events (`tts-state-changed`).

### Hotkey

Neue Action „Clipboard vorlesen“ in der bestehenden Action-/Shortcut-
Registrierung, Standard-Bindung konfigurierbar, Default: keine (Nutzer weist im
Settings-Bereich zu — vermeidet Kollisionen mit Diktat-Hotkeys).

### Settings (Erweiterung `settings.rs`)

| Feld | Default | Zweck |
|---|---|---|
| `tts_fish_dir` | `C:\AI\fish-speech` | Installationsverzeichnis |
| `tts_port` | `8080` | Server-Port (URL wird daraus gebaut, Host fest 127.0.0.1) |
| `tts_seed` | `42` | feste Stimme vor TP2 (Konsistenz zwischen Aufträgen) |
| `tts_idle_minutes` | `15` | Leerlauf bis Server-Stopp (0 = nie stoppen) |
| `tts_max_chars` | `5000` | Schutz vor versehentlichen Riesen-Clipboards |

### Frontend

Neuer Bereich „Vorlesen“ (`components/settings/tts/`):
Textfeld + Abspielen/Stopp-Button, Server-Status-Badge
(Aus / Startet… (n s) / Bereit / Spricht / Fehler), Buttons Server starten/
stoppen, Einstellungsfelder (Verzeichnis, Idle-Zeit, Seed, Hotkey-Zuweisung),
alle Strings i18n (en als Source + de gepflegt). Typen via tauri-specta.

## Fehlerbehandlung

- Fish-Verzeichnis/venv fehlt → Fehlermeldung mit erwartetem Pfad und Verweis
  auf `C:\AI\fish-speech\INSTALL-REPORT.md`.
- Start-Timeout (180 s) → Prozess beenden, Error-State mit Meldung
  („VRAM prüfen: andere GPU-Apps schließen“).
- HTTP-Fehler/kein RIFF in Antwort → Error-Event mit Klartext, kein stilles
  Scheitern.
- Leere Zwischenablage/nur Nicht-Text → Hinweis-Event, kein Serverstart.
- Text > `tts_max_chars` → Abschneiden mit Hinweis (kein Fehler).

## Sicherheit

- Ausschließlich `127.0.0.1`, Port konfigurierbar; keine externen Requests.
- Vorzulesender Text wird nicht geloggt (Muster aus fda9cf6 fortführen);
  geloggt werden nur Längen und Zeiten.
- Keine Telemetrie (`HF_HUB_DISABLE_TELEMETRY=1` beim Spawn).

## Tests

1. **Rust-Unit/Integration** (cargo test): Zustandsmaschine (Start/Idle/
   Cancel/Fehlerpfade) und Request-/Antwort-Handling gegen einen Mock-HTTP-
   Server auf Ephemeral-Port (liefert Mini-WAV bzw. Fehlercodes); Erkennung
   „extern laufender Server wird nicht gekillt“.
2. **Selftest-Bench** (Erweiterung `selftest.rs`, CLI-aufrufbar analog
   Diktat-Bench): Health → kurzer deutscher Satz → RIFF/Sample-Rate-Validierung,
   Messwerte in ms (Serverstart, Time-to-Audio, Gesamt); Exit 0/≠0.
3. **Manuelle Abnahme** (Evidence `docs/m4-evidence/` nach M2/M3-Muster):
   Hotkey-Flow (kopieren → Hotkey → Sprachausgabe), Tab-Flow, Idle-Stopp
   beobachtet, App-Exit hinterlässt keinen Python-Prozess.

## Abnahmekriterien (TP1 fertig, wenn)

1. Clipboard-Vorlesen per Hotkey funktioniert reproduzierbar (deutscher Text).
2. Erststart aus Stopped inkl. Serverstart läuft sichtbar durch (Statusanzeige)
   und endet in hörbarem Audio.
3. Idle-Stopp gibt VRAM nachweislich frei (nvidia-smi vorher/nachher).
4. App-Exit beendet einen selbst gestarteten Server immer.
5. Alle drei Testebenen grün; Evidence-Dokument liegt vor.

## Git

Branch `feat/m4-tts-vorlesen` ab `feat/m3-stabilize-paste-path`.
Konventionelle Commits (`feat(tts): …`), Warum-fokussierte Messages.

## Offene Punkte für spätere TPs (nicht TP1)

- TP2 nutzt `reference_id` statt `seed`; Settings-Feld `tts_voice` kommt dann.
- TP5 ersetzt Non-Streaming-Playback durch Chunk-Playback (TTFA-Ziel) und
  behandelt `--compile`/triton-windows am Server.
