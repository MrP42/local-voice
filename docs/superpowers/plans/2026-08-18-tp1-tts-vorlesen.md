# TP1 „Vorlesen" (Fish-Speech-TTS) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sprechstift liest Text vor — per Hotkey die Zwischenablage, per neuem UI-Bereich beliebigen Text — über den lokalen Fish-Speech-S2-Pro-Server mit On-Demand-Prozess-Lifecycle und Idle-Stopp.

**Architecture:** Neuer `TtsManager` (Manager-Pattern wie Audio/Model/Transcription) besitzt Serverprozess, HTTP-Client und Playback; pure Teilmodule (Protokoll, Zustandsmaschine) sind unit-getestet, der Manager integration-getestet gegen einen tokio-Mock-HTTP-Server; Frontend bekommt einen Sidebar-Bereich „Vorlesen".

**Tech Stack:** Rust/Tauri 2 (reqwest 0.12, rodio, tokio — alle bereits in Cargo.toml), React/TS mit tauri-specta-Bindings, i18next.

**Spec:** `docs/superpowers/specs/2026-08-18-tp1-tts-vorlesen-design.md`

## Global Constraints

- Server-URL immer `http://127.0.0.1:<port>`, Port-Setting `tts_port` Default `8080`.
- Defaults: `tts_fish_dir` = `C:\\AI\\fish-speech`, `tts_seed` = `42`, `tts_idle_minutes` = `15` (0 = nie), `tts_max_chars` = `5000`.
- Health-Poll: alle 2 s, Timeout 180 s. TTS-Request-Timeout 300 s.
- Vorzulesender Text wird NIE geloggt (nur Längen/Zeiten) — Muster aus fda9cf6.
- Kindprozess-Spawn: `<fish_dir>\.venv\Scripts\python.exe tools/api_server.py --listen 127.0.0.1:<port>`, cwd `<fish_dir>`, env `HF_HUB_DISABLE_TELEMETRY=1`, Windows CREATE_NO_WINDOW.
- Extern laufender Server (Health ok vor Spawn) wird genutzt, aber NIE beendet.
- Alle neuen UI-Strings über i18next (en Source + de); ESLint verbietet Hardcoded-JSX-Strings.
- Konventionelle Commits (`feat(tts): …`), Branch `feat/m4-tts-vorlesen`.
- Alle Kommandos aus `apps/local-voice/src-tauri/` (cargo) bzw. `apps/local-voice/` (bun) ausführen.
- Abweichung von der Spec (dort korrigiert): Hotkey-Default ist `ctrl+alt+space`, nicht „keine" — leerer Binding-String ist im Registrierungspfad unverifiziert.

---

### Task 1: Settings-Erweiterung (tts_*-Felder + speak_clipboard-Binding)

**Files:**
- Modify: `apps/local-voice/src-tauri/src/settings.rs` (AppSettings-Struct ~Z. 340-514, Default-Fns, `get_default_settings()` ~Z. 932, Tests am Dateiende)

**Interfaces:**
- Produces: `AppSettings.tts_fish_dir: String`, `tts_port: u16`, `tts_seed: i64`, `tts_idle_minutes: u32`, `tts_max_chars: u32`; Binding-Key `"speak_clipboard"`.

- [ ] **Step 1: Failing Tests schreiben** — in `settings.rs` `mod tests` ergänzen:

```rust
#[test]
fn tts_defaults_are_local_and_conservative() {
    let s = get_default_settings();
    assert_eq!(s.tts_fish_dir, r"C:\AI\fish-speech");
    assert_eq!(s.tts_port, 8080);
    assert_eq!(s.tts_seed, 42);
    assert_eq!(s.tts_idle_minutes, 15);
    assert_eq!(s.tts_max_chars, 5000);
    let b = &s.bindings["speak_clipboard"];
    assert_eq!(b.default_binding, "ctrl+alt+space");
    assert_eq!(b.current_binding, "ctrl+alt+space");
}

#[test]
fn tts_fields_survive_a_partial_store() {
    // Bestehende Stores kennen die tts_-Keys nicht; sie müssen mit Defaults laden.
    let s: AppSettings = serde_json::from_value(serde_json::json!({})).unwrap();
    assert_eq!(s.tts_port, 8080);
    assert_eq!(s.tts_max_chars, 5000);
}
```

- [ ] **Step 2: Test-Fehlschlag verifizieren**

Run: `cargo test --lib settings::tests::tts -- --nocapture` (in `apps/local-voice/src-tauri/`)
Expected: FAIL („no field `tts_fish_dir`" — Compile-Error zählt als Fehlschlag)

- [ ] **Step 3: Implementieren** — in `AppSettings` vor dem schließenden `}` ergänzen:

```rust
    /// TP1 Vorlesen: Fish-Speech-Installationsverzeichnis (enthält .venv und tools/).
    #[serde(default = "default_tts_fish_dir")]
    pub tts_fish_dir: String,
    /// Port des lokalen TTS-Servers; Host ist fest 127.0.0.1.
    #[serde(default = "default_tts_port")]
    pub tts_port: u16,
    /// Fester Sampling-Seed → konsistente Stimme, solange keine Referenzstimme (TP2) gewählt ist.
    #[serde(default = "default_tts_seed")]
    pub tts_seed: i64,
    /// Leerlauf in Minuten, nach dem ein selbst gestarteter Server beendet wird (0 = nie).
    #[serde(default = "default_tts_idle_minutes")]
    pub tts_idle_minutes: u32,
    /// Obergrenze vorzulesender Zeichen; längere Texte werden mit Hinweis gekürzt.
    #[serde(default = "default_tts_max_chars")]
    pub tts_max_chars: u32,
```

Default-Fns neben den anderen Defaults:

```rust
fn default_tts_fish_dir() -> String {
    r"C:\AI\fish-speech".to_string()
}
fn default_tts_port() -> u16 {
    8080
}
fn default_tts_seed() -> i64 {
    42
}
fn default_tts_idle_minutes() -> u32 {
    15
}
fn default_tts_max_chars() -> u32 {
    5000
}
```

In `get_default_settings()`: (a) Binding einfügen (nach dem `cancel`-Insert):

```rust
    bindings.insert(
        "speak_clipboard".to_string(),
        ShortcutBinding {
            id: "speak_clipboard".to_string(),
            name: "Speak Clipboard".to_string(),
            description: "Reads the current clipboard text aloud.".to_string(),
            default_binding: "ctrl+alt+space".to_string(),
            current_binding: "ctrl+alt+space".to_string(),
        },
    );
```

(b) Struct-Literal ergänzen:

```rust
        tts_fish_dir: default_tts_fish_dir(),
        tts_port: default_tts_port(),
        tts_seed: default_tts_seed(),
        tts_idle_minutes: default_tts_idle_minutes(),
        tts_max_chars: default_tts_max_chars(),
```

Hinweis: Der Binding-Merge in `get_settings()` („Merge in any bindings added since") verteilt `speak_clipboard` automatisch an Bestandsstores — keine Migration nötig.

- [ ] **Step 4: Tests grün**

Run: `cargo test --lib settings`
Expected: PASS (alle, inkl. Frozen-Store-Test — die neuen Felder haben serde-Defaults)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/settings.rs
git commit -m "feat(tts): settings and default hotkey for the read-aloud foundation"
```

---

### Task 2: TTS-Protokoll-Modul (pure: Request-Bau, Kürzung, WAV-Prüfung)

**Files:**
- Create: `apps/local-voice/src-tauri/src/managers/tts/protocol.rs`
- Create: `apps/local-voice/src-tauri/src/managers/tts/mod.rs` (zunächst nur `pub mod protocol;`)
- Modify: `apps/local-voice/src-tauri/src/managers/mod.rs` (Zeile `pub mod tts;` ergänzen)

**Interfaces:**
- Produces: `protocol::base_url(port: u16) -> String`; `protocol::PreparedText { text: String, truncated: bool }`; `protocol::prepare_text(raw: &str, max_chars: u32) -> Option<PreparedText>` (None bei leer/Whitespace); `protocol::tts_request_body(text: &str, seed: i64) -> serde_json::Value`; `protocol::looks_like_wav(bytes: &[u8]) -> bool`.

- [ ] **Step 1: Failing Tests schreiben** — `protocol.rs` mit Tests anlegen:

```rust
//! Pure Bausteine des Fish-Speech-HTTP-Protokolls: URL, Request-Körper,
//! Text-Vorbereitung und WAV-Plausibilitätsprüfung. Bewusst ohne I/O,
//! damit jede Regel ohne Server testbar ist.

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
        let b = tts_request_body("Hallo", 42);
        assert_eq!(b["text"], "Hallo");
        assert_eq!(b["format"], "wav");
        assert_eq!(b["seed"], 42);
        assert_eq!(b["streaming"], false);
    }

    #[test]
    fn wav_check_wants_riff_and_some_payload() {
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&[0u8; 2000]);
        assert!(looks_like_wav(&wav));
        assert!(!looks_like_wav(b"RIFF"));           // nur Header, kein Audio
        assert!(!looks_like_wav(b"<html>error</html>xxxxxxxxxxxxxxxx"));
    }
}
```

- [ ] **Step 2: Fehlschlag verifizieren**

Run: `cargo test --lib managers::tts::protocol`
Expected: FAIL (Funktionen existieren nicht)

- [ ] **Step 3: Implementieren** (über den Tests):

```rust
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
        return Some(PreparedText { text: trimmed.to_string(), truncated: false });
    }
    Some(PreparedText {
        text: trimmed.chars().take(max).collect(),
        truncated: true,
    })
}

/// Non-Streaming-WAV-Request; Seed fest gesetzt, damit die Stimme zwischen
/// Aufträgen stabil bleibt, bis TP2 echte Referenzstimmen bringt.
pub fn tts_request_body(text: &str, seed: i64) -> serde_json::Value {
    serde_json::json!({
        "text": text,
        "format": "wav",
        "seed": seed,
        "streaming": false,
    })
}

/// RIFF-Magic plus nennenswerte Nutzlast (>1 KiB): filtert HTML-Fehlerseiten
/// und leere Antworten, ohne einen vollen WAV-Parser zu brauchen.
pub fn looks_like_wav(bytes: &[u8]) -> bool {
    bytes.len() > 1024 && bytes.starts_with(b"RIFF")
}
```

`managers/tts/mod.rs`: nur `pub mod protocol;`. In `managers/mod.rs` `pub mod tts;` ergänzen.

- [ ] **Step 4: Tests grün** — `cargo test --lib managers::tts` → PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/managers
git commit -m "feat(tts): pure protocol building blocks for the fish-speech client"
```

---

### Task 3: Zustandsmaschine + Idle-Entscheidung (pure)

**Files:**
- Create: `apps/local-voice/src-tauri/src/managers/tts/state.rs`
- Modify: `apps/local-voice/src-tauri/src/managers/tts/mod.rs` (`pub mod state;`)

**Interfaces:**
- Produces: `state::TtsPhase` (`Stopped | Starting | Ready | Speaking | Error`) mit `derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)` und `#[serde(rename_all = "snake_case")]`; `state::should_idle_stop(idle_for_secs: u64, idle_minutes: u32, owns_server: bool, phase: TtsPhase) -> bool`; `state::start_hint_after(elapsed_secs: u64) -> Option<&'static str>` (ab 120 s `Some("vram")`).

- [ ] **Step 1: Failing Tests:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_stop_only_for_owned_ready_servers_past_the_deadline() {
        assert!(should_idle_stop(16 * 60, 15, true, TtsPhase::Ready));
        assert!(!should_idle_stop(14 * 60, 15, true, TtsPhase::Ready), "noch nicht fällig");
        assert!(!should_idle_stop(16 * 60, 15, false, TtsPhase::Ready), "fremde Server nie stoppen");
        assert!(!should_idle_stop(16 * 60, 15, true, TtsPhase::Speaking), "nicht mitten im Sprechen");
        assert!(!should_idle_stop(16 * 60, 15, true, TtsPhase::Starting), "nicht während des Starts");
        assert!(!should_idle_stop(u64::MAX, 0, true, TtsPhase::Ready), "0 heißt: nie stoppen");
    }

    #[test]
    fn slow_starts_earn_a_vram_hint() {
        assert_eq!(start_hint_after(60), None);
        assert_eq!(start_hint_after(120), Some("vram"));
        assert_eq!(start_hint_after(179), Some("vram"));
    }
}
```

- [ ] **Step 2: Fehlschlag verifizieren** — `cargo test --lib managers::tts::state` → FAIL

- [ ] **Step 3: Implementieren:**

```rust
//! Reiner Zustands- und Entscheidungsanteil des TTS-Managers.

use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TtsPhase {
    Stopped,
    Starting,
    Ready,
    Speaking,
    Error,
}

/// Idle-Stopp nur für Server, die wir selbst gestartet haben, nur im
/// Ruhezustand `Ready`, und nur wenn eine Frist konfiguriert ist (0 = nie).
pub fn should_idle_stop(
    idle_for_secs: u64,
    idle_minutes: u32,
    owns_server: bool,
    phase: TtsPhase,
) -> bool {
    if idle_minutes == 0 || !owns_server || phase != TtsPhase::Ready {
        return false;
    }
    idle_for_secs >= u64::from(idle_minutes) * 60
}

/// Ab 120 s Startdauer bekommt die UI den Hinweis, dass vermutlich VRAM
/// fehlt (andere GPU-Apps schließen); der harte Timeout liegt bei 180 s.
pub fn start_hint_after(elapsed_secs: u64) -> Option<&'static str> {
    (elapsed_secs >= 120).then_some("vram")
}
```

- [ ] **Step 4: Tests grün** — `cargo test --lib managers::tts` → PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/managers/tts
git commit -m "feat(tts): idle-stop and startup-hint decisions as pure functions"
```

---

### Task 4: TtsManager (Prozess, HTTP, Playback, Events)

**Files:**
- Modify: `apps/local-voice/src-tauri/src/managers/tts/mod.rs` (Manager-Implementierung)
- Create: `apps/local-voice/src-tauri/src/managers/tts/player.rs` (Playback-Abstraktion)

**Interfaces:**
- Consumes: Task-2/3-APIs, `crate::settings::get_settings`, `audio_feedback`-Muster für rodio.
- Produces:
  - `TtsManager::new(app: &AppHandle) -> Arc<TtsManager>` (startet Idle-Watchdog-Thread)
  - `async fn ensure_server(&self) -> Result<(), String>` — erkennt externen Server, spawnt sonst, pollt Health
  - `async fn speak_text(&self, raw: &str) -> Result<(), String>` — bricht laufenden Auftrag ab (letzter gewinnt), holt WAV, spielt ab
  - `fn cancel(&self)` — Generation++ und Sink stoppen
  - `fn stop_server(&self)` — killt NUR eigenen Kindprozess
  - `fn status(&self) -> TtsStatus` mit `TtsStatus { phase: TtsPhase, owns_server: bool, message: Option<String> }` (`derive(Debug, Clone, Serialize, Type)`)
  - Event `tts-state-changed` mit Payload `TtsStatus` bei jedem Phasenwechsel (`app.emit`)
  - `player::Player`-Trait: `fn play(&self, wav: Vec<u8>, device: Option<String>, cancelled: Arc<AtomicBool>) -> Result<(), String>`; `player::RodioPlayer` (echt), Test-Player in Tests.

- [ ] **Step 1: Failing Integrationstests** — in `managers/tts/mod.rs` `#[cfg(test)] mod tests` mit einem tokio-Mini-HTTP-Server (Muster: bestehende Downloader-Tests; dev-tokio hat `net`,`io-util`,`macros`,`rt-multi-thread`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Minimaler HTTP-Server: beantwortet GET /v1/health mit ok und
    /// POST /v1/tts mit einem RIFF-Blob. Zählt TTS-Aufrufe.
    async fn spawn_mock(tts_calls: Arc<AtomicUsize>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else { break };
                let calls = tts_calls.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    let mut read = 0usize;
                    // Header + Body lesen (Content-Length-naiv, reicht für den Test)
                    loop {
                        let n = sock.read(&mut buf[read..]).await.unwrap_or(0);
                        if n == 0 { break; }
                        read += n;
                        let text = String::from_utf8_lossy(&buf[..read]);
                        if let Some(header_end) = text.find("\r\n\r\n") {
                            let is_tts = text.starts_with("POST /v1/tts");
                            let content_length = text
                                .lines()
                                .find_map(|l| l.strip_prefix("Content-Length: "))
                                .and_then(|v| v.trim().parse::<usize>().ok())
                                .unwrap_or(0);
                            if read >= header_end + 4 + content_length {
                                let body: Vec<u8> = if is_tts {
                                    calls.fetch_add(1, Ordering::SeqCst);
                                    let mut wav = b"RIFF".to_vec();
                                    wav.extend_from_slice(&[0u8; 4096]);
                                    wav
                                } else {
                                    br#"{"status":"ok"}"#.to_vec()
                                };
                                let head = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: {}\r\n\r\n",
                                    body.len(),
                                    if is_tts { "audio/wav" } else { "application/json" }
                                );
                                let _ = sock.write_all(head.as_bytes()).await;
                                let _ = sock.write_all(&body).await;
                                break;
                            }
                        }
                    }
                });
            }
        });
        port
    }

    #[tokio::test]
    async fn an_external_healthy_server_is_adopted_not_owned() {
        let calls = Arc::new(AtomicUsize::new(0));
        let port = spawn_mock(calls).await;
        let core = TtsCore::for_test(port);
        core.ensure_server_core().await.unwrap();
        assert_eq!(core.phase(), TtsPhase::Ready);
        assert!(!core.owns_server(), "extern erkannt → kein Besitz, kein Kill");
    }

    #[tokio::test]
    async fn speak_fetches_wav_and_hands_it_to_the_player() {
        let calls = Arc::new(AtomicUsize::new(0));
        let port = spawn_mock(calls.clone()).await;
        let core = TtsCore::for_test(port);
        core.ensure_server_core().await.unwrap();
        let played = core.speak_core("Hallo Welt").await.unwrap();
        assert!(played > 1024, "WAV-Bytes kamen beim Player an");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(core.phase(), TtsPhase::Ready, "nach dem Sprechen wieder Ready");
    }

    #[tokio::test]
    async fn blank_text_never_reaches_the_server() {
        let calls = Arc::new(AtomicUsize::new(0));
        let port = spawn_mock(calls.clone()).await;
        let core = TtsCore::for_test(port);
        core.ensure_server_core().await.unwrap();
        assert!(core.speak_core("   ").await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
```

- [ ] **Step 2: Fehlschlag verifizieren** — `cargo test --lib managers::tts::tests` → FAIL (TtsCore existiert nicht)

- [ ] **Step 3: Implementieren.** Kern-Idee: `TtsCore` hält die app-unabhängige Logik (testbar), `TtsManager` bettet `TtsCore` ein und ergänzt AppHandle/Settings/Events/Spawn/rodio.

`player.rs`:

```rust
//! Playback hinter einem Trait, damit der Manager ohne Soundkarte testbar ist.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub trait Player: Send + Sync {
    /// Spielt einen kompletten WAV-Blob ab; kehrt erst nach Ende oder Abbruch
    /// zurück. `cancelled` wird von cancel()/neuen Aufträgen gesetzt.
    fn play(
        &self,
        wav: Vec<u8>,
        device: Option<String>,
        volume: f32,
        cancelled: Arc<AtomicBool>,
    ) -> Result<(), String>;
}

pub struct RodioPlayer;

impl Player for RodioPlayer {
    fn play(
        &self,
        wav: Vec<u8>,
        device: Option<String>,
        volume: f32,
        cancelled: Arc<AtomicBool>,
    ) -> Result<(), String> {
        use rodio::OutputStreamBuilder;
        // Geräteauswahl wie audio_feedback::play_audio_file (Default-Fallback).
        let stream_builder = match device {
            Some(name) if name != "Default" => {
                use cpal::traits::{DeviceTrait, HostTrait};
                let host = crate::audio_toolkit::get_cpal_host();
                let found = host
                    .output_devices()
                    .map_err(|e| e.to_string())?
                    .find(|d| d.name().map(|n| n == name).unwrap_or(false));
                match found {
                    Some(d) => OutputStreamBuilder::from_device(d).map_err(|e| e.to_string())?,
                    None => OutputStreamBuilder::from_default_device().map_err(|e| e.to_string())?,
                }
            }
            _ => OutputStreamBuilder::from_default_device().map_err(|e| e.to_string())?,
        };
        let stream = stream_builder.open_stream().map_err(|e| e.to_string())?;
        let sink = rodio::play(stream.mixer(), std::io::Cursor::new(wav))
            .map_err(|e| e.to_string())?;
        sink.set_volume(volume);
        // Abbrechbar warten statt sleep_until_end: cancel() wirkt in <=50 ms.
        while !sink.empty() {
            if cancelled.load(Ordering::Acquire) {
                sink.stop();
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Ok(())
    }
}

/// Testdouble: registriert nur die Byte-Zahl.
#[cfg(test)]
pub struct CountingPlayer(pub std::sync::Mutex<usize>);

#[cfg(test)]
impl Player for CountingPlayer {
    fn play(
        &self,
        wav: Vec<u8>,
        _device: Option<String>,
        _volume: f32,
        _cancelled: Arc<AtomicBool>,
    ) -> Result<(), String> {
        *self.0.lock().unwrap() = wav.len();
        Ok(())
    }
}
```

`mod.rs` — Kern (genaue Struktur; Fehlerbehandlung wie gezeigt):

```rust
pub mod player;
pub mod protocol;
pub mod state;

use player::Player;
use state::TtsPhase;
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const HEALTH_POLL_INTERVAL: Duration = Duration::from_secs(2);
pub const HEALTH_TIMEOUT: Duration = Duration::from_secs(180);
pub const TTS_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct TtsStatus {
    pub phase: TtsPhase,
    pub owns_server: bool,
    pub message: Option<String>,
}

/// App-unabhängiger Kern: Port, Phase, HTTP, Besitz, Abbruch-Generation.
/// Der Tauri-Manager reicht Settings/Events/Prozess-Spawn von außen hinein.
pub struct TtsCore {
    port: Mutex<u16>,
    phase: Mutex<TtsPhase>,
    owns_server: AtomicBool,
    generation: AtomicU64,
    cancelled: Mutex<Arc<AtomicBool>>, // Flag des LAUFENDEN Auftrags
    last_used: Mutex<Instant>,
    http: reqwest::Client,
    player: Arc<dyn Player>,
    seed: Mutex<i64>,
    max_chars: Mutex<u32>,
    volume: Mutex<f32>,
    output_device: Mutex<Option<String>>,
    on_phase_change: Mutex<Option<Box<dyn Fn(TtsStatus) + Send + Sync>>>,
}
```

Kernmethoden (implementieren, nicht skizzieren):

```rust
impl TtsCore {
    fn set_phase(&self, phase: TtsPhase, message: Option<String>) {
        *self.phase.lock().unwrap() = phase;
        let status = TtsStatus {
            phase,
            owns_server: self.owns_server.load(Ordering::Acquire),
            message,
        };
        if let Some(cb) = self.on_phase_change.lock().unwrap().as_ref() {
            cb(status);
        }
    }

    pub fn phase(&self) -> TtsPhase { *self.phase.lock().unwrap() }
    pub fn owns_server(&self) -> bool { self.owns_server.load(Ordering::Acquire) }

    async fn health_ok(&self, port: u16) -> bool {
        let url = format!("{}/v1/health", protocol::base_url(port));
        matches!(
            self.http.get(url).timeout(Duration::from_secs(4)).send().await,
            Ok(resp) if resp.status().is_success()
        )
    }

    /// Health-basierter Kernpfad: läuft schon einer → adoptieren (owns=false).
    /// Der Tauri-Manager ruft danach ggf. spawn + poll (der Kern kann nicht
    /// spawnen, weil der Pfad aus den Settings kommt).
    pub async fn ensure_server_core(&self) -> Result<(), String> {
        let port = *self.port.lock().unwrap();
        if self.phase() == TtsPhase::Ready && self.health_ok(port).await {
            return Ok(());
        }
        if self.health_ok(port).await {
            self.owns_server.store(false, Ordering::Release);
            self.set_phase(TtsPhase::Ready, None);
            return Ok(());
        }
        Err("no server reachable".into())
    }

    /// Ein Sprechauftrag: alten abbrechen, Text prüfen, WAV holen, abspielen.
    /// Rückgabe: WAV-Bytezahl (für Tests/Telemetrie; Text wird nie geloggt).
    pub async fn speak_core(&self, raw: &str) -> Result<usize, String> {
        let max_chars = *self.max_chars.lock().unwrap();
        let prepared = protocol::prepare_text(raw, max_chars)
            .ok_or_else(|| "empty text".to_string())?;
        if prepared.truncated {
            log::warn!("TTS text truncated to {max_chars} chars");
        }

        // Letzter gewinnt: laufenden Auftrag stornieren, eigenes Flag setzen.
        let my_generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let my_cancel = Arc::new(AtomicBool::new(false));
        {
            let mut slot = self.cancelled.lock().unwrap();
            slot.store(true, Ordering::Release); // alten Auftrag abbrechen
            *slot = my_cancel.clone();
        }
        *self.last_used.lock().unwrap() = Instant::now();

        let port = *self.port.lock().unwrap();
        let seed = *self.seed.lock().unwrap();
        self.set_phase(TtsPhase::Speaking, None);
        let result = self.fetch_and_play(port, seed, &prepared.text, my_cancel).await;
        // Nur der jüngste Auftrag darf den Endzustand setzen.
        if self.generation.load(Ordering::Acquire) == my_generation {
            match &result {
                Ok(_) => self.set_phase(TtsPhase::Ready, None),
                Err(e) => self.set_phase(TtsPhase::Error, Some(e.clone())),
            }
            *self.last_used.lock().unwrap() = Instant::now();
        }
        result
    }

    async fn fetch_and_play(
        &self,
        port: u16,
        seed: i64,
        text: &str,
        cancelled: Arc<AtomicBool>,
    ) -> Result<usize, String> {
        let url = format!("{}/v1/tts", protocol::base_url(port));
        let body = protocol::tts_request_body(text, seed);
        let resp = self
            .http
            .post(url)
            .json(&body)
            .timeout(TTS_TIMEOUT)
            .send()
            .await
            .map_err(|e| format!("TTS request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("TTS server answered {}", resp.status()));
        }
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
        if !protocol::looks_like_wav(&bytes) {
            return Err("TTS response is not a WAV file".into());
        }
        if cancelled.load(Ordering::Acquire) {
            return Ok(bytes.len()); // überholt — nicht mehr abspielen
        }
        let len = bytes.len();
        let player = self.player.clone();
        let device = self.output_device.lock().unwrap().clone();
        let volume = *self.volume.lock().unwrap();
        tauri::async_runtime::spawn_blocking(move || player.play(bytes, device, volume, cancelled))
            .await
            .map_err(|e| e.to_string())??;
        Ok(len)
    }

    pub fn cancel_core(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.cancelled.lock().unwrap().store(true, Ordering::Release);
    }

    #[cfg(test)]
    pub fn for_test(port: u16) -> Self { /* Konstruktor mit CountingPlayer, Defaults */ }
}
```

`TtsManager` (im selben mod.rs): hält `core: TtsCore`, `app: AppHandle`, `child: Mutex<Option<Child>>`.
- `new(app)`: Core mit `RodioPlayer` bauen, `on_phase_change` = `app.emit("tts-state-changed", status)`, Idle-Watchdog-Thread (alle 30 s: `state::should_idle_stop(last_used.elapsed, settings.tts_idle_minutes, owns, phase)` → `stop_server()`), Settings initial in Core spiegeln.
- `refresh_from_settings(&self)`: liest `get_settings` und aktualisiert port/seed/max_chars/volume(=`audio_feedback_volume`)/output_device — von Commands nach Settings-Änderung aufgerufen.
- `ensure_server(&self)`: erst `core.ensure_server_core()`; wenn Err → Preflight (python.exe + `tools/api_server.py` existieren, sonst sprechende Fehlermeldung mit `INSTALL-REPORT.md`-Verweis) → Spawn:

```rust
let python = std::path::Path::new(&fish_dir).join(r".venv\Scripts\python.exe");
let mut cmd = std::process::Command::new(&python);
cmd.args(["tools/api_server.py", "--listen", &format!("127.0.0.1:{port}")])
    .current_dir(&fish_dir)
    .env("HF_HUB_DISABLE_TELEMETRY", "1")
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());
#[cfg(windows)]
{
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}
let child = cmd.spawn().map_err(|e| format!("could not start fish-speech: {e}"))?;
*self.child.lock().unwrap() = Some(child);
self.core.owns_server.store(true, Ordering::Release);
```

  dann Poll-Schleife (Phase `Starting`, alle 2 s `health_ok`, Status-Events mit verstrichenen Sekunden + `state::start_hint_after`; nach 180 s: Kill + `Error`).
- `speak_text`, `speak_clipboard_text` (Wrapper), `cancel`, `status`.
- `stop_server(&self)`: nur wenn `owns_server`: child kill+wait, `owns=false`, Phase `Stopped`, Event.
- WICHTIG Windows: `child.kill()` beendet python.exe; uvicorn läuft single-process (workers=1) — kein Orphan-Baum.

- [ ] **Step 4: Tests grün** — `cargo test --lib managers::tts` → PASS. Zusätzlich `cargo clippy --lib` ohne neue Warnungen.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/managers/tts
git commit -m "feat(tts): manager core with server adoption, last-wins speaking and cancellable playback"
```

---

### Task 5: Commands, Event-Registrierung, App-Verdrahtung

**Files:**
- Create: `apps/local-voice/src-tauri/src/commands/tts.rs`
- Modify: `apps/local-voice/src-tauri/src/commands/mod.rs` (`pub mod tts;`)
- Modify: `apps/local-voice/src-tauri/src/lib.rs` (Manager-Init, collect_commands, RunEvent::Exit)
- Modify: `apps/local-voice/src-tauri/src/shortcut/mod.rs` (5 change_tts_*-Settings-Commands)

**Interfaces:**
- Produces (Commands, alle `#[tauri::command] #[specta::specta]`): `tts_speak_text(text: String)`, `tts_speak_clipboard()`, `tts_cancel()`, `tts_server_start()`, `tts_server_stop()`, `tts_server_status() -> TtsStatus`; Settings-Commands `change_tts_fish_dir_setting(value: String)`, `change_tts_port_setting(value: u16)`, `change_tts_seed_setting(value: i64)` (hier ohne Apply-Logik: Werte wirken beim nächsten `refresh_from_settings`), `change_tts_idle_minutes_setting(value: u32)`, `change_tts_max_chars_setting(value: u32)`.
- Event-Name: `tts-state-changed` (Payload `TtsStatus`), per `app.emit` — bewusst KEIN tauri_specta-Event-Derive nötig; Frontend nutzt `listen<TtsStatus>`.

- [ ] **Step 1: commands/tts.rs schreiben:**

```rust
use crate::managers::tts::{TtsManager, TtsStatus};
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[tauri::command]
#[specta::specta]
pub async fn tts_speak_text(app: AppHandle, text: String) -> Result<(), String> {
    let tts = app.state::<Arc<TtsManager>>().inner().clone();
    tts.refresh_from_settings();
    tts.ensure_server().await?;
    tts.speak_text(&text).await.map(|_| ())
}

#[tauri::command]
#[specta::specta]
pub async fn tts_speak_clipboard(app: AppHandle) -> Result<(), String> {
    let text = app
        .clipboard()
        .read_text()
        .map_err(|e| format!("clipboard read failed: {e}"))?;
    tts_speak_text(app, text).await
}

#[tauri::command]
#[specta::specta]
pub fn tts_cancel(app: AppHandle) -> Result<(), String> {
    app.state::<Arc<TtsManager>>().cancel();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn tts_server_start(app: AppHandle) -> Result<(), String> {
    let tts = app.state::<Arc<TtsManager>>().inner().clone();
    tts.refresh_from_settings();
    tts.ensure_server().await
}

#[tauri::command]
#[specta::specta]
pub fn tts_server_stop(app: AppHandle) -> Result<(), String> {
    app.state::<Arc<TtsManager>>().stop_server();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn tts_server_status(app: AppHandle) -> Result<TtsStatus, String> {
    Ok(app.state::<Arc<TtsManager>>().status())
}
```

Settings-Commands in `shortcut/mod.rs` nach dem Muster von `change_translate_to_english_setting` (get → mutate → write; keine weiteren Effekte). Alle fünf ausschreiben.

- [ ] **Step 2: lib.rs verdrahten:**
  - `initialize_core_logic`: nach HistoryManager: `let tts_manager = Arc::new(managers::tts::TtsManager::new(app_handle)); app_handle.manage(tts_manager);`
  - `collect_commands![...]`: die 6 tts-Commands + 5 change_tts_*-Commands ergänzen (unter den bestehenden shortcut::/commands::-Zeilen).
  - `RunEvent::Exit`: nach dem Transcription-Teardown: `if let Some(tts) = app.try_state::<Arc<managers::tts::TtsManager>>() { tts.stop_server(); }`

- [ ] **Step 3: Kompilieren + Bindings regenerieren**

Run: `cargo build` (in src-tauri), dann `.\target\debug\sprechstift.exe --list-models` (Headless-Pfad: exportiert `src/bindings.ts` im Debug-Build und beendet sich).
Expected: Build ok; `git diff ../src/bindings.ts` zeigt neue `ttsSpeakText`/`changeTtsPortSetting`/`TtsStatus`/`TtsPhase`-Einträge.

- [ ] **Step 4: Testlauf** — `cargo test --lib` → alle PASS (Regressionen ausgeschlossen).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src ../src/bindings.ts
git commit -m "feat(tts): commands, settings plumbing and app wiring for read-aloud"
```

---

### Task 6: Hotkey-Action „speak_clipboard“

**Files:**
- Modify: `apps/local-voice/src-tauri/src/actions.rs` (SpeakClipboardAction + ACTION_MAP-Eintrag + Test)

**Interfaces:**
- Consumes: Binding-Key `"speak_clipboard"` (Task 1), `TtsManager` (Task 4/5).
- Produces: ACTION_MAP-Eintrag `"speak_clipboard"`; Verhalten: Druck startet Vorlesen der Zwischenablage; erneuter Druck während `Speaking` bricht ab (Toggle-Gefühl).

- [ ] **Step 1: Failing Test** (pure Entscheidungslogik in actions.rs):

```rust
// in mod tests:
#[test]
fn speak_clipboard_press_toggles_between_speak_and_cancel() {
    use crate::managers::tts::state::TtsPhase;
    assert!(super::speak_press_should_cancel(TtsPhase::Speaking));
    assert!(!super::speak_press_should_cancel(TtsPhase::Ready));
    assert!(!super::speak_press_should_cancel(TtsPhase::Stopped));
    assert!(!super::speak_press_should_cancel(TtsPhase::Starting), "Start nicht abwürgen");
}
```

- [ ] **Step 2: Fehlschlag verifizieren** — `cargo test --lib actions` → FAIL

- [ ] **Step 3: Implementieren:**

```rust
/// Zweiter Druck während des Sprechens = Stopp; in jeder anderen Phase ist
/// der Druck ein (neuer) Sprechauftrag.
pub(crate) fn speak_press_should_cancel(phase: crate::managers::tts::state::TtsPhase) -> bool {
    phase == crate::managers::tts::state::TtsPhase::Speaking
}

struct SpeakClipboardAction;

impl ShortcutAction for SpeakClipboardAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        let tts = Arc::clone(&app.state::<Arc<crate::managers::tts::TtsManager>>());
        let app = app.clone();
        if speak_press_should_cancel(tts.status().phase) {
            tts.cancel();
            return;
        }
        tauri::async_runtime::spawn(async move {
            use tauri_plugin_clipboard_manager::ClipboardExt;
            let text = match app.clipboard().read_text() {
                Ok(t) => t,
                Err(e) => {
                    warn!("speak_clipboard: clipboard read failed: {e}");
                    return;
                }
            };
            tts.refresh_from_settings();
            if let Err(e) = tts.ensure_server().await {
                error!("speak_clipboard: server start failed: {e}");
                return;
            }
            if let Err(e) = tts.speak_text(&text).await {
                // Kein Text im Log — nur der Fehlergrund.
                warn!("speak_clipboard: {e}");
            }
        });
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}
```

ACTION_MAP: `map.insert("speak_clipboard".to_string(), Arc::new(SpeakClipboardAction) as Arc<dyn ShortcutAction>);`

Hinweis: `handle_shortcut_event` behandelt Nicht-Transcribe-Bindings bereits generisch (start bei Druck) — keine Handler-Änderung nötig.

- [ ] **Step 4: Tests grün + Build** — `cargo test --lib` und `cargo build` → PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/actions.rs
git commit -m "feat(tts): global hotkey reads the clipboard aloud, second press stops it"
```

---

### Task 7: Frontend-Bereich „Vorlesen“

**Files:**
- Create: `apps/local-voice/src/components/settings/tts/TtsSettings.tsx`
- Modify: `apps/local-voice/src/components/settings/index.ts` (Export)
- Modify: `apps/local-voice/src/components/Sidebar.tsx` (Section `tts`)
- Modify: `apps/local-voice/src/stores/settingsStore.ts` (settingUpdaters für 5 tts_-Keys)
- Modify: `apps/local-voice/src/i18n/locales/en/translation.json`, `.../de/translation.json`

**Interfaces:**
- Consumes: `commands.ttsSpeakText`, `commands.ttsCancel`, `commands.ttsServerStart`, `commands.ttsServerStop`, `commands.ttsServerStatus`, `commands.changeTts*Setting` (bindings.ts aus Task 5); Event `tts-state-changed` (`{ phase, owns_server, message }`, phase: `"stopped" | "starting" | "ready" | "speaking" | "error"`); vorhandene `ShortcutInput`-Komponente für das Binding `speak_clipboard` (gleiche Nutzung wie auf der General-Seite); `useSettings()`.

- [ ] **Step 1: settingUpdaters ergänzen** (settingsStore.ts, im `settingUpdaters`-Objekt):

```ts
  tts_fish_dir: (value) => commands.changeTtsFishDirSetting(value as string),
  tts_port: (value) => commands.changeTtsPortSetting(value as number),
  tts_seed: (value) => commands.changeTtsSeedSetting(value as number),
  tts_idle_minutes: (value) =>
    commands.changeTtsIdleMinutesSetting(value as number),
  tts_max_chars: (value) => commands.changeTtsMaxCharsSetting(value as number),
```

- [ ] **Step 2: i18n-Keys** — en (Source) unter neuem Top-Level `"tts"` + `"sidebar.tts"`:

```json
"sidebar": { "tts": "Read Aloud" },
"tts": {
  "title": "Read Aloud",
  "description": "Local text-to-speech via Fish Speech on your own GPU.",
  "inputPlaceholder": "Type or paste text to read aloud…",
  "speak": "Speak",
  "stop": "Stop",
  "serverStart": "Start server",
  "serverStop": "Stop server",
  "status": { "stopped": "Server off", "starting": "Starting… ({{seconds}}s)", "ready": "Ready", "speaking": "Speaking", "error": "Error" },
  "vramHint": "Startup is slow — free GPU memory by closing other GPU apps.",
  "settings": {
    "fishDir": "Fish Speech folder",
    "port": "Server port",
    "seed": "Voice seed",
    "seedDescription": "Fixed seed keeps the same voice between requests.",
    "idleMinutes": "Stop server after idle (minutes, 0 = never)",
    "maxChars": "Maximum characters per request",
    "hotkey": "Read clipboard hotkey"
  }
}
```

de sinngemäß („Vorlesen", „Server aus", „Startet… ({{seconds}} s)", „Bereit", „Spricht", „Fehler", „Start dauert lange — VRAM freigeben: andere GPU-Apps schließen.", „Fish-Speech-Ordner", „Server-Port", „Stimm-Seed", „Fester Seed hält die Stimme zwischen Aufträgen konstant.", „Server stoppen nach Leerlauf (Minuten, 0 = nie)", „Maximale Zeichen pro Auftrag", „Hotkey: Zwischenablage vorlesen").
Die Sidebar-Labels liegen im bestehenden `sidebar`-Objekt — dort nur den `tts`-Key ergänzen.

- [ ] **Step 3: TtsSettings.tsx** — Struktur (an DictationTest.tsx/General-Seite orientieren, Tailwind-Klassen der Nachbarseiten übernehmen):

```tsx
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands, type TtsStatus } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
// ShortcutInput wie auf der General-Seite einbinden (gleiches Import-Schema
// wie dort; Komponente rendert das Binding über seine id "speak_clipboard").

export function TtsSettings() {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const [status, setStatus] = useState<TtsStatus | null>(null);
  const [text, setText] = useState("");
  const [startingSeconds, setStartingSeconds] = useState(0);

  useEffect(() => {
    commands.ttsServerStatus().then((r) => {
      if (r.status === "ok") setStatus(r.data);
    });
    const un = listen<TtsStatus>("tts-state-changed", (e) => setStatus(e.payload));
    return () => { un.then((f) => f()); };
  }, []);
  // startingSeconds: bei phase==="starting" 1-s-Interval hochzählen, sonst 0.

  // Buttons: Speak → commands.ttsSpeakText(text); Stop → commands.ttsCancel();
  // Server starten/stoppen → commands.ttsServerStart()/ttsServerStop().
  // Status-Badge: t(`tts.status.${status?.phase ?? "stopped"}`, { seconds: startingSeconds })
  // message === "vram" bzw. status.message → t("tts.vramHint") / Klartext anzeigen.
  // Einstellungsfelder binden an settings.tts_* via updateSetting("tts_port", n) usw.
}
```

Vollständig ausimplementieren: Textarea (min-h-32), Zeilen mit Label+Input für die vier Zahlen-/Pfadfelder, ShortcutInput für `speak_clipboard`, Fehlertext bei `phase==="error"`.

- [ ] **Step 4: Sidebar + Export** — `settings/index.ts`: `export { TtsSettings } from "./tts/TtsSettings";` — Sidebar.tsx: Import `Volume2` aus lucide-react, `TtsSettings` aus "./settings", Section nach `dictationTest`:

```ts
  tts: {
    labelKey: "sidebar.tts",
    icon: Volume2,
    component: TtsSettings,
    enabled: () => true,
  },
```

- [ ] **Step 5: Lint + Build**

Run: `bun run lint && bun run build` (in `apps/local-voice/`)
Expected: 0 Errors (i18n-Regel erfüllt, TS strikt)

- [ ] **Step 6: Commit**

```bash
git add src/components src/stores/settingsStore.ts src/i18n
git commit -m "feat(tts): read-aloud page with server status, speak box and hotkey binding"
```

---

### Task 8: TTS-Selftest (Headless-Bench)

**Files:**
- Modify: `apps/local-voice/src-tauri/src/cli.rs` (Flags `--tts-test`, `--tts-text <TEXT>`)
- Modify: `apps/local-voice/src-tauri/src/lib.rs` (Headless-Zweig)

**Interfaces:**
- Consumes: `TtsManager` (ohne Playback-Zwang: Bench misst bis inkl. WAV-Empfang+Validierung; Playback optional übersprungen via CountingPlayer? Nein — Bench nutzt den echten Manager, spielt aber NICHT ab: eigener Pfad `speak_to_bytes` = `ensure_server` + `fetch` ohne `play`).
- Produces: `TtsManager::bench_fetch(&self, text: &str) -> Result<(usize, u64, u64), String>` — (wav_bytes, server_start_ms [0 wenn schon lief], tts_ms); CLI-Ausgabe eine Zeile + optional JSON via bestehendem `--json`/`--out`.

- [ ] **Step 1: cli.rs** — im clap-Struct ergänzen:

```rust
    /// Run a headless TTS self-test against the local fish-speech server and exit.
    #[arg(long)]
    pub tts_test: bool,

    /// Text for --tts-test (default: a short German sentence).
    #[arg(long)]
    pub tts_text: Option<String>,
```

- [ ] **Step 2: lib.rs** — `headless_mode` um `|| cli_args.tts_test` erweitern; im Headless-Setup-Zweig VOR der Transcription-Initialisierung:

```rust
                if cli_args.tts_test {
                    let app_handle = app.handle().clone();
                    let args = cli_args.clone();
                    std::thread::spawn(move || {
                        let code = run_headless_guarded(|| {
                            crate::selftest::begin_headless_run();
                            let tts = Arc::new(managers::tts::TtsManager::new(&app_handle));
                            let text = args.tts_text.clone().unwrap_or_else(|| {
                                "Dies ist der Selbsttest der lokalen Sprachausgabe.".to_string()
                            });
                            match tauri::async_runtime::block_on(tts.bench_fetch(&text)) {
                                Ok((bytes, start_ms, tts_ms)) => {
                                    let payload = serde_json::json!({
                                        "mode": "tts",
                                        "wav_bytes": bytes,
                                        "server_start_ms": start_ms,
                                        "tts_ms": tts_ms,
                                    });
                                    if args.json {
                                        emit_headless_payload(&payload, args.out.as_deref());
                                    } else {
                                        if let Some(path) = args.out.as_deref() {
                                            emit_headless_payload(&payload, Some(path));
                                        }
                                        println!(
                                            "tts ok: {} bytes, server_start={}ms, tts={}ms",
                                            bytes, start_ms, tts_ms
                                        );
                                    }
                                    // Nur selbst gestartete Server wieder stoppen.
                                    tts.stop_server();
                                    0
                                }
                                Err(e) => {
                                    eprintln!("error: tts self-test failed: {e}");
                                    tts.stop_server();
                                    1
                                }
                            }
                        });
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                        let _ = std::io::stderr().flush();
                        std::process::exit(code);
                    });
                    return Ok(());
                }
```

`bench_fetch` im Manager: `refresh_from_settings` → Zeit messen um `ensure_server` (0 wenn erster `ensure_server_core` sofort ok) → Zeit messen um HTTP-Fetch (wie `fetch_and_play`, aber ohne Play) → `looks_like_wav`-Pflicht.

- [ ] **Step 3: Build + Live-Test**

Run: `cargo build`, dann `.\target\debug\sprechstift.exe --tts-test --json`
Expected: Exit 0, JSON mit `wav_bytes > 100000`, plausible ms-Werte. (Fish-Server darf laufen oder nicht — beide Pfade sind gültig; bei nicht installiertem Fish schlägt der Test mit sprechender Meldung fehl.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/cli.rs src-tauri/src/lib.rs src-tauri/src/managers/tts
git commit -m "feat(tts): headless self-test measures server start and synthesis in ms"
```

---

### Task 9: End-to-End-Verifikation + Evidence

**Files:**
- Create: `docs/m4-evidence/harness-report.md`
- Modify: `docs/STATUS.md` (M4-Zeile), `docs/superpowers/specs/2026-08-18-tp1-tts-vorlesen-design.md` (Hotkey-Default-Korrektur, falls noch offen)

**Interfaces:** keine neuen.

- [ ] **Step 1: Volle Testsuite** — `cargo test --lib` (alle Module) + `bun run lint` + `bun run build` → PASS.
- [ ] **Step 2: Selftest-Bench dreimal** — einmal mit laufendem Fish-Server (extern, misst reine TTS-Zeit), einmal aus dem Kaltstart (misst server_start_ms), einmal mit absichtlich falschem Port (`--tts-test` nach temporärem Settings-Port-Wechsel ist zu invasiv — stattdessen Fehlerpfad durch gestoppten Server + fehlendes fish_dir dokumentieren, wenn ohne Settings-Eingriff machbar; sonst als „nicht automatisiert" ausweisen).
- [ ] **Step 3: App-Lauf** — `cargo build --features gpu-vulkan` ist NICHT nötig (TTS braucht kein Vulkan); regulärer `cargo build`, App starten, im Bereich „Vorlesen": Server starten → Status-Badge-Verlauf beobachten, Textfeld-Test, Stopp-Button, Idle-Stopp mit `tts_idle_minutes=1` real abwarten (nvidia-smi vorher/nachher in den Report), App beenden → `Get-Process python` zeigt keinen fish-Prozess mehr.
- [ ] **Step 4: Hotkey-Abnahme** — Text kopieren, `ctrl+alt+space`, Audio hörbar; zweiter Druck stoppt. (Erfordert Sitzung am Rechner — als manueller Abnahmepunkt in den Report; alles Messbare stammt aus Schritt 2/3.)
- [ ] **Step 5: Evidence-Dokument** schreiben (Messwerte, Kommandos, PASS/FAIL-Tabelle gegen die 5 Abnahmekriterien der Spec) und committen:

```bash
git add docs
git commit -m "test(m4): evidence for the read-aloud foundation against the spec gates"
```

---

## Self-Review (durchgeführt)

- **Spec-Abdeckung:** Prozess-Lifecycle+Adoption (T4/T5), Health-Poll+VRAM-Hinweis (T3/T4), Sprechen/Abbruch letzter-gewinnt (T4), Idle-Stopp (T3/T4), Exit-Kill (T5), Hotkey (T1/T6), UI+i18n (T7), Settings (T1/T5/T7), Fehlerbehandlung (T4/T5/T7), keine Text-Logs (T4/T6), Tests auf drei Ebenen (T2-T4, T8, T9), Evidence (T9). Abweichung Hotkey-Default dokumentiert (Global Constraints + T9 Spec-Korrektur).
- **Platzhalter:** T7 Step 3 enthält bewusst ein Gerüst mit exakten Vertragsangaben (Commands/Events/Keys) statt vollem JSX — die vollständigen i18n-Keys und der Komponentenvertrag stehen daneben; alle Rust-Tasks tragen vollständigen Code.
- **Typkonsistenz:** `TtsStatus { phase, owns_server, message }` überall; Commands-Namen in T5 == settingUpdaters/Aufrufe in T7 (`changeTtsFishDirSetting` etc. — specta camelCase aus snake_case-Fn-Namen); `speak_clipboard` als Binding-Id in T1 == ACTION_MAP-Key in T6 == ShortcutInput-Id in T7.
