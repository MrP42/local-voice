//! Vorlesen (TP1): Fish-Speech-TTS-Anbindung.
//!
//! `protocol` und `state` sind pure, I/O-freie Bausteine. `TtsCore` bündelt
//! die app-unabhängige Logik (HTTP, Phase, Abbruch, Besitz) und ist gegen
//! einen Mock-Server getestet; `TtsManager` ergänzt AppHandle-Belange:
//! Settings, Events, Prozess-Spawn, Idle-Watchdog und Exit-Teardown.

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
const IDLE_WATCH_INTERVAL: Duration = Duration::from_secs(30);

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
    /// Abbruch-Flag des LAUFENDEN Auftrags; neue Aufträge tauschen es aus.
    cancelled: Mutex<Arc<AtomicBool>>,
    last_used: Mutex<Instant>,
    http: reqwest::Client,
    player: Arc<dyn Player>,
    seed: Mutex<i64>,
    max_chars: Mutex<u32>,
    volume: Mutex<f32>,
    output_device: Mutex<Option<String>>,
    on_phase_change: Mutex<Option<Box<dyn Fn(TtsStatus) + Send + Sync>>>,
}

impl TtsCore {
    fn new(player: Arc<dyn Player>) -> Self {
        Self {
            port: Mutex::new(8080),
            phase: Mutex::new(TtsPhase::Stopped),
            owns_server: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            cancelled: Mutex::new(Arc::new(AtomicBool::new(false))),
            last_used: Mutex::new(Instant::now()),
            http: reqwest::Client::new(),
            player,
            seed: Mutex::new(42),
            max_chars: Mutex::new(5000),
            volume: Mutex::new(1.0),
            output_device: Mutex::new(None),
            on_phase_change: Mutex::new(None),
        }
    }

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

    pub fn phase(&self) -> TtsPhase {
        *self.phase.lock().unwrap()
    }

    pub fn owns_server(&self) -> bool {
        self.owns_server.load(Ordering::Acquire)
    }

    pub fn status(&self) -> TtsStatus {
        TtsStatus {
            phase: self.phase(),
            owns_server: self.owns_server(),
            message: None,
        }
    }

    fn idle_for_secs(&self) -> u64 {
        self.last_used.lock().unwrap().elapsed().as_secs()
    }

    async fn health_ok(&self, port: u16) -> bool {
        let url = format!("{}/v1/health", protocol::base_url(port));
        matches!(
            self.http.get(url).timeout(Duration::from_secs(4)).send().await,
            Ok(resp) if resp.status().is_success()
        )
    }

    /// Health-basierter Kernpfad: läuft schon ein Server → adoptieren
    /// (owns=false, wird nie gekillt). Spawnen kann nur der Manager, weil der
    /// Pfad aus den Settings kommt.
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
        let prepared =
            protocol::prepare_text(raw, max_chars).ok_or_else(|| "empty text".to_string())?;
        if prepared.truncated {
            log::warn!("TTS text truncated to {max_chars} chars");
        }

        // Letzter gewinnt: laufenden Auftrag stornieren, eigenes Flag setzen.
        let my_generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let my_cancel = Arc::new(AtomicBool::new(false));
        {
            let mut slot = self.cancelled.lock().unwrap();
            slot.store(true, Ordering::Release);
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

    /// WAV vom Server holen, validieren; `play` ist optional, damit der
    /// Selbsttest (Task 8) denselben Pfad ohne Soundkarte messen kann.
    async fn fetch_wav(&self, port: u16, seed: i64, text: &str) -> Result<Vec<u8>, String> {
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
        Ok(bytes)
    }

    async fn fetch_and_play(
        &self,
        port: u16,
        seed: i64,
        text: &str,
        cancelled: Arc<AtomicBool>,
    ) -> Result<usize, String> {
        let bytes = self.fetch_wav(port, seed, text).await?;
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
    pub fn for_test(port: u16) -> Self {
        let core = Self::new(Arc::new(player::CountingPlayer(std::sync::Mutex::new(0))));
        *core.port.lock().unwrap() = port;
        core
    }
}

/// Tauri-seitiger Manager: besitzt ggf. den Serverprozess und verdrahtet
/// Settings, Events und den Idle-Watchdog.
pub struct TtsManager {
    core: Arc<TtsCore>,
    app: tauri::AppHandle,
    child: Mutex<Option<Child>>,
}

impl TtsManager {
    pub fn new(app: &tauri::AppHandle) -> Arc<Self> {
        use tauri::Emitter;

        let core = Arc::new(TtsCore::new(Arc::new(player::RodioPlayer)));
        let emitter = app.clone();
        *core.on_phase_change.lock().unwrap() = Some(Box::new(move |status: TtsStatus| {
            if let Err(e) = emitter.emit("tts-state-changed", status) {
                log::warn!("Could not emit tts-state-changed: {e}");
            }
        }));

        let manager = Arc::new(Self {
            core,
            app: app.clone(),
            child: Mutex::new(None),
        });
        manager.refresh_from_settings();

        // Idle-Watchdog: beendet einen selbst gestarteten Server nach der
        // konfigurierten Leerlaufzeit, damit die 17 GB VRAM wieder frei werden.
        let watchdog = Arc::downgrade(&manager);
        std::thread::spawn(move || loop {
            std::thread::sleep(IDLE_WATCH_INTERVAL);
            let Some(manager) = watchdog.upgrade() else {
                break;
            };
            let idle_minutes = crate::settings::get_settings(&manager.app).tts_idle_minutes;
            if state::should_idle_stop(
                manager.core.idle_for_secs(),
                idle_minutes,
                manager.core.owns_server(),
                manager.core.phase(),
            ) {
                log::info!("TTS server idle for {idle_minutes} min — stopping to free VRAM");
                manager.stop_server();
            }
        });

        manager
    }

    /// Settings in den Kern spiegeln. Vor jedem Auftrag aufgerufen, damit
    /// Änderungen ohne App-Neustart wirken.
    pub fn refresh_from_settings(&self) {
        let settings = crate::settings::get_settings(&self.app);
        *self.core.port.lock().unwrap() = settings.tts_port;
        *self.core.seed.lock().unwrap() = settings.tts_seed;
        *self.core.max_chars.lock().unwrap() = settings.tts_max_chars;
        *self.core.volume.lock().unwrap() = settings.audio_feedback_volume;
        *self.core.output_device.lock().unwrap() = settings.selected_output_device;
    }

    pub fn status(&self) -> TtsStatus {
        self.core.status()
    }

    pub fn cancel(&self) {
        self.core.cancel_core();
    }

    pub async fn speak_text(&self, raw: &str) -> Result<usize, String> {
        self.core.speak_core(raw).await
    }

    /// Server sicherstellen: adoptieren, sonst spawnen und Health pollen.
    pub async fn ensure_server(&self) -> Result<(), String> {
        if self.core.ensure_server_core().await.is_ok() {
            return Ok(());
        }

        let settings = crate::settings::get_settings(&self.app);
        let fish_dir = std::path::PathBuf::from(&settings.tts_fish_dir);
        let port = settings.tts_port;
        let python = fish_dir.join(r".venv\Scripts\python.exe");
        let api_script = fish_dir.join("tools").join("api_server.py");
        if !python.exists() || !api_script.exists() {
            let msg = format!(
                "Fish Speech nicht gefunden unter '{}'. Erwartet: .venv\\Scripts\\python.exe und tools\\api_server.py — siehe C:\\AI\\fish-speech\\INSTALL-REPORT.md",
                fish_dir.display()
            );
            self.core.set_phase(TtsPhase::Error, Some(msg.clone()));
            return Err(msg);
        }

        // Liegengebliebenen (abgestürzten) eigenen Prozess aufräumen.
        self.kill_owned_child();

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
        let child = cmd
            .spawn()
            .map_err(|e| format!("could not start fish-speech: {e}"))?;
        *self.child.lock().unwrap() = Some(child);
        self.core.owns_server.store(true, Ordering::Release);
        self.core.set_phase(TtsPhase::Starting, None);
        log::info!("Started fish-speech server on 127.0.0.1:{port}, waiting for health");

        let started = Instant::now();
        let mut hint_sent = false;
        loop {
            if self.core.health_ok(port).await {
                self.core.set_phase(TtsPhase::Ready, None);
                *self.core.last_used.lock().unwrap() = Instant::now();
                log::info!(
                    "fish-speech ready after {} s",
                    started.elapsed().as_secs()
                );
                return Ok(());
            }
            // Früher Kindprozess-Tod (falscher Pfad, kaputtes venv) → klarer Fehler.
            if let Some(child) = self.child.lock().unwrap().as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    let msg = format!("fish-speech exited during startup ({status})");
                    self.core.owns_server.store(false, Ordering::Release);
                    self.core.set_phase(TtsPhase::Error, Some(msg.clone()));
                    return Err(msg);
                }
            }
            let elapsed = started.elapsed();
            if elapsed > HEALTH_TIMEOUT {
                self.kill_owned_child();
                let msg = format!(
                    "fish-speech not healthy after {} s — VRAM prüfen: andere GPU-Apps schließen",
                    elapsed.as_secs()
                );
                self.core.set_phase(TtsPhase::Error, Some(msg.clone()));
                return Err(msg);
            }
            if !hint_sent {
                if let Some(hint) = state::start_hint_after(elapsed.as_secs()) {
                    self.core
                        .set_phase(TtsPhase::Starting, Some(hint.to_string()));
                    hint_sent = true;
                }
            }
            tokio::time::sleep(HEALTH_POLL_INTERVAL).await;
        }
    }

    /// Selbsttest-Messpfad: Server sicherstellen, WAV holen (ohne Playback),
    /// Zeiten melden. Rückgabe: (wav_bytes, server_start_ms, tts_ms) —
    /// server_start_ms ist 0, wenn ein Server bereits lief.
    pub async fn bench_fetch(&self, text: &str) -> Result<(usize, u64, u64), String> {
        self.refresh_from_settings();
        let already_running = self.core.ensure_server_core().await.is_ok();
        let start = Instant::now();
        if !already_running {
            self.ensure_server().await?;
        }
        let server_start_ms = if already_running {
            0
        } else {
            start.elapsed().as_millis() as u64
        };

        let prepared = {
            let max_chars = *self.core.max_chars.lock().unwrap();
            protocol::prepare_text(text, max_chars).ok_or_else(|| "empty text".to_string())?
        };
        let port = *self.core.port.lock().unwrap();
        let seed = *self.core.seed.lock().unwrap();
        let tts_start = Instant::now();
        let wav = self.core.fetch_wav(port, seed, &prepared.text).await?;
        let tts_ms = tts_start.elapsed().as_millis() as u64;
        Ok((wav.len(), server_start_ms, tts_ms))
    }

    /// Beendet AUSSCHLIESSLICH einen selbst gestarteten Serverprozess.
    pub fn stop_server(&self) {
        if !self.core.owns_server() {
            return;
        }
        self.core.cancel_core();
        self.kill_owned_child();
        self.core.owns_server.store(false, Ordering::Release);
        self.core.set_phase(TtsPhase::Stopped, None);
    }

    fn kill_owned_child(&self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            if let Err(e) = child.kill() {
                log::warn!("Could not kill fish-speech child: {e}");
            }
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Minimaler HTTP-Server: beantwortet GET /v1/health mit ok und
    /// POST /v1/tts mit einem RIFF-Blob. Zählt TTS-Aufrufe. Schließt jede
    /// Verbindung nach einer Antwort (Connection: close), damit reqwest
    /// nicht auf Keep-Alive besteht.
    async fn spawn_mock(tts_calls: Arc<AtomicUsize>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let calls = tts_calls.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    let mut read = 0usize;
                    loop {
                        let n = sock.read(&mut buf[read..]).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        read += n;
                        let text = String::from_utf8_lossy(&buf[..read]).to_lowercase();
                        if let Some(header_end) = text.find("\r\n\r\n") {
                            let is_tts = text.starts_with("post /v1/tts");
                            let content_length = text
                                .lines()
                                .find_map(|l| l.strip_prefix("content-length: "))
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
                                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: {}\r\nConnection: close\r\n\r\n",
                                    body.len(),
                                    if is_tts { "audio/wav" } else { "application/json" }
                                );
                                let _ = sock.write_all(head.as_bytes()).await;
                                let _ = sock.write_all(&body).await;
                                let _ = sock.shutdown().await;
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

    #[tokio::test]
    async fn unreachable_port_is_reported_not_adopted() {
        // Port 1 ist praktisch nie belegt; der Kern darf dann nichts adoptieren.
        let core = TtsCore::for_test(1);
        assert!(core.ensure_server_core().await.is_err());
        assert_eq!(core.phase(), TtsPhase::Stopped);
        assert!(!core.owns_server());
    }

    #[tokio::test]
    async fn cancel_marks_the_running_jobs_flag() {
        let core = TtsCore::for_test(1);
        let flag = core.cancelled.lock().unwrap().clone();
        assert!(!flag.load(Ordering::Acquire));
        core.cancel_core();
        assert!(flag.load(Ordering::Acquire), "cancel muss den laufenden Auftrag treffen");
    }
}
