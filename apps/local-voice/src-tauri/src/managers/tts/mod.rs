//! Vorlesen (TP1): Fish-Speech-TTS-Anbindung.
//!
//! `protocol` und `state` sind pure, I/O-freie Bausteine. `TtsCore` bündelt
//! die app-unabhängige Logik (HTTP, Phase, Abbruch, Besitz) und ist gegen
//! einen Mock-Server getestet; `TtsManager` ergänzt AppHandle-Belange:
//! Settings, Events, Prozess-Spawn, Idle-Watchdog und Exit-Teardown.

pub mod player;
pub mod protocol;
pub mod state;
pub mod voices;

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
    speed: Mutex<f32>,
    export_format: Mutex<String>,
    output_device: Mutex<Option<String>>,
    /// Aktive Referenzstimme (reference_id) oder None = Seed-Standardstimme.
    voice: Mutex<Option<String>>,
    /// Satz-Level-WAV-Cache: unveränderter Text (gleicher Satz, Seed und
    /// Stimme) wird beim erneuten Vorlesen nicht neu synthetisiert.
    wav_cache: Mutex<WavCache>,
    on_phase_change: Mutex<Option<Box<dyn Fn(TtsStatus) + Send + Sync>>>,
}

/// Prozess-lebenszeitiger Audio-Cache mit Byte-Limit und FIFO-Verdrängung —
/// bewusst simpel: Wiederholungen (gleicher Text, Resume, Zurückspringen im
/// Hörbuch) treffen ihn, Speicher bleibt begrenzt.
struct WavCache {
    map: std::collections::HashMap<u64, Vec<u8>>,
    order: std::collections::VecDeque<u64>,
    bytes: usize,
}

const WAV_CACHE_LIMIT_BYTES: usize = 200 * 1024 * 1024;

impl WavCache {
    fn new() -> Self {
        Self {
            map: std::collections::HashMap::new(),
            order: std::collections::VecDeque::new(),
            bytes: 0,
        }
    }

    fn key(text: &str, seed: i64, voice: Option<&str>) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        seed.hash(&mut h);
        voice.hash(&mut h);
        h.finish()
    }

    fn get(&self, key: u64) -> Option<Vec<u8>> {
        self.map.get(&key).cloned()
    }

    fn insert(&mut self, key: u64, wav: Vec<u8>) {
        if wav.len() > WAV_CACHE_LIMIT_BYTES || self.map.contains_key(&key) {
            return;
        }
        while self.bytes + wav.len() > WAV_CACHE_LIMIT_BYTES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.map.remove(&oldest) {
                self.bytes -= evicted.len();
            }
        }
        self.bytes += wav.len();
        self.order.push_back(key);
        self.map.insert(key, wav);
    }
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
            speed: Mutex::new(1.0),
            export_format: Mutex::new("wav".to_string()),
            output_device: Mutex::new(None),
            voice: Mutex::new(None),
            wav_cache: Mutex::new(WavCache::new()),
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
        let sentences = protocol::split_sentences(&prepared.text);
        self.speak_sentence_run(sentences, 0, None).await
    }

    /// Gemeinsamer Sprechpfad für Freitext und Hörbuch: Sätze pipelined
    /// sprechen, ab `start_index`, mit optionalem Callback nach jedem
    /// VOLLSTÄNDIG abgespielten Satz (absoluter Index) — die Basis für die
    /// persistente Fortschrittsanzeige.
    pub async fn speak_sentence_run(
        &self,
        sentences: Vec<String>,
        start_index: usize,
        on_played: Option<Arc<dyn Fn(usize) + Send + Sync>>,
    ) -> Result<usize, String> {
        if sentences.is_empty() {
            return Err("empty text".into());
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
        let result = self
            .fetch_and_play_pipelined(
                port,
                seed,
                &sentences,
                start_index,
                my_cancel.clone(),
                on_played,
            )
            .await;
        // Nur der jüngste, NICHT stornierte Auftrag darf den Endzustand
        // setzen — nach einem Abbruch gehört die Phase dem Abbrecher
        // (cancel_core → Ready, stop_server → Stopped).
        if self.generation.load(Ordering::Acquire) == my_generation
            && !my_cancel.load(Ordering::Acquire)
        {
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
        let voice = self.voice.lock().unwrap().clone();
        // Unveränderter Satz + gleiche Stimme/Seed → aus dem Cache, ohne Server.
        let cache_key = WavCache::key(text, seed, voice.as_deref());
        if let Some(cached) = self.wav_cache.lock().unwrap().get(cache_key) {
            return Ok(cached);
        }
        let body = protocol::tts_request_body(text, seed, voice.as_deref());
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
        self.wav_cache.lock().unwrap().insert(cache_key, bytes.clone());
        Ok(bytes)
    }

    /// Satz-Pipeline: Satz N wird abgespielt, während Satz N+1 bereits beim
    /// Server liegt. Die gefühlte Latenz ist damit die Synthese des ersten
    /// Satzes; bei RTF < 1 (compile) bleibt die Wiedergabe lückenlos.
    /// `on_played` feuert nach jedem vollständig abgespielten Satz mit dessen
    /// absolutem Index — bei Abbruch mitten im Satz feuert es NICHT (der Satz
    /// wird beim Fortsetzen erneut gehört, wie bei einem Hörbuch üblich).
    async fn fetch_and_play_pipelined(
        &self,
        port: u16,
        seed: i64,
        sentences: &[String],
        start_index: usize,
        cancelled: Arc<AtomicBool>,
        on_played: Option<Arc<dyn Fn(usize) + Send + Sync>>,
    ) -> Result<usize, String> {
        let max_chars = *self.max_chars.lock().unwrap();
        let mut previous_playback: Option<(
            usize,
            tauri::async_runtime::JoinHandle<Result<(), String>>,
        )> = None;
        let mut total_bytes = 0usize;
        let mut failure: Option<String> = None;

        let notify = |idx: usize, was_cancelled: bool| {
            if was_cancelled {
                return;
            }
            if let Some(cb) = on_played.as_ref() {
                cb(idx);
            }
        };

        for (offset, sentence) in sentences.iter().enumerate().skip(start_index) {
            if cancelled.load(Ordering::Acquire) {
                break;
            }
            // Einzelne Sätze absichern (leere überspringen, Monster kappen).
            let Some(prepared) = protocol::prepare_text(sentence, max_chars) else {
                notify(offset, cancelled.load(Ordering::Acquire));
                continue;
            };
            match self.fetch_wav(port, seed, &prepared.text).await {
                Ok(bytes) => {
                    total_bytes += bytes.len();
                    // Vorherigen Satz zu Ende spielen lassen (Reihenfolge!).
                    if let Some((done_idx, handle)) = previous_playback.take() {
                        match handle.await {
                            Ok(Ok(())) => {
                                notify(done_idx, cancelled.load(Ordering::Acquire));
                            }
                            Ok(Err(e)) => {
                                failure = Some(e);
                                break;
                            }
                            Err(e) => {
                                failure = Some(e.to_string());
                                break;
                            }
                        }
                    }
                    if cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    let player = self.player.clone();
                    let device = self.output_device.lock().unwrap().clone();
                    let volume = *self.volume.lock().unwrap();
                    let speed = *self.speed.lock().unwrap();
                    let cancel_flag = cancelled.clone();
                    previous_playback = Some((
                        offset,
                        tauri::async_runtime::spawn_blocking(move || {
                            player.play(bytes, device, volume, speed, cancel_flag)
                        }),
                    ));
                }
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
        if let Some((done_idx, handle)) = previous_playback {
            match handle.await {
                Ok(Ok(())) => {
                    notify(done_idx, cancelled.load(Ordering::Acquire));
                }
                Ok(Err(e)) => {
                    failure.get_or_insert(e);
                }
                Err(e) => {
                    failure.get_or_insert(e.to_string());
                }
            }
        }
        match failure {
            Some(e) => Err(e),
            None => Ok(total_bytes),
        }
    }

    pub fn cancel_core(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.cancelled.lock().unwrap().store(true, Ordering::Release);
        // Der stornierte Auftrag darf die Phase nicht mehr anfassen (Guard) —
        // also stellt der Abbrecher selbst den Ruhezustand wieder her. Ohne
        // das bleibt die UI auf „Spricht" hängen und der Idle-Stopp greift nie.
        if self.phase() == TtsPhase::Speaking {
            self.set_phase(TtsPhase::Ready, None);
        }
        *self.last_used.lock().unwrap() = Instant::now();
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
    /// Letzte Referenzaufnahme (16 kHz mono), wartet zwischen Stopp und
    /// Speichern auf Namen + bestätigtes Transkript.
    pending_reference: Mutex<Option<Vec<f32>>>,
    /// Geöffnetes Hörbuch/Dokument (Sätze + Identität); die Position lebt im
    /// persistenten Fortschritts-Store.
    reading: Mutex<Option<ReadingSession>>,
    /// Letzter Freitext-Sprechauftrag (Sätze + Position) — Basis für
    /// Pause/Weiter im Vorlesen-Feld, bewusst nicht persistiert.
    speak_session: Mutex<Option<SpeakSession>>,
}

struct SpeakSession {
    sentences: Vec<String>,
    position: usize,
}

struct ReadingSession {
    key: String,
    title: String,
    sentences: Vec<String>,
}

/// Fortschritt eines Dokuments — Persistenz-Eintrag und Event-Payload.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct ReadingInfo {
    /// Absoluter Dateipfad = stabile Identität des Dokuments.
    pub key: String,
    pub title: String,
    /// Nächster zu spielender Satz (0-basiert) = Anzahl fertig gehörter Sätze.
    pub position: u32,
    pub total: u32,
    pub finished: bool,
    pub playing: bool,
}

const READING_STORE: &str = "reading_progress.json";

/// Binding-Id des Referenzaufnahme-Flows im AudioRecordingManager.
const REFERENCE_BINDING: &str = "voice_reference";
/// Binding-Id des Übersetzungsaufnahme-Flows.
const TRANSLATE_BINDING: &str = "translate_input";
/// Binding-Id des Stimmwechsler-Flows.
const VOICECHANGE_BINDING: &str = "voicechange_input";

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
            pending_reference: Mutex::new(None),
            reading: Mutex::new(None),
            speak_session: Mutex::new(None),
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
        *self.core.volume.lock().unwrap() = settings.tts_volume;
        *self.core.speed.lock().unwrap() = settings.tts_speed;
        *self.core.export_format.lock().unwrap() = settings.tts_export_format;
        *self.core.output_device.lock().unwrap() = settings.selected_output_device;
        *self.core.voice.lock().unwrap() = settings.tts_voice;
    }

    pub fn status(&self) -> TtsStatus {
        self.core.status()
    }

    pub fn cancel(&self) {
        self.core.cancel_core();
    }

    /// Freitext sprechen: legt eine Pause/Weiter-fähige Session an und meldet
    /// den Satzfortschritt als `tts-speak-progress`-Event.
    pub async fn speak_text(self: &Arc<Self>, raw: &str) -> Result<usize, String> {
        let max_chars = *self.core.max_chars.lock().unwrap();
        let prepared = protocol::prepare_text(raw, max_chars)
            .ok_or_else(|| "empty text".to_string())?;
        let sentences = protocol::split_sentences(&prepared.text);
        *self.speak_session.lock().unwrap() = Some(SpeakSession {
            sentences: sentences.clone(),
            position: 0,
        });
        self.run_speak_session(sentences, 0).await
    }

    /// Pausiertes Freitext-Vorlesen ab dem letzten vollständig gehörten Satz
    /// fortsetzen.
    pub async fn speak_resume(self: &Arc<Self>) -> Result<usize, String> {
        let (sentences, position) = {
            let guard = self.speak_session.lock().unwrap();
            let session = guard.as_ref().ok_or("nichts zum Fortsetzen")?;
            if session.position >= session.sentences.len() {
                return Err("bereits zu Ende gelesen".into());
            }
            (session.sentences.clone(), session.position)
        };
        self.run_speak_session(sentences, position).await
    }

    async fn run_speak_session(
        self: &Arc<Self>,
        sentences: Vec<String>,
        start: usize,
    ) -> Result<usize, String> {
        use tauri::Emitter;
        let total = sentences.len() as u32;
        let cb_manager = Arc::clone(self);
        let on_played: Arc<dyn Fn(usize) + Send + Sync> = Arc::new(move |idx| {
            if let Some(session) = cb_manager.speak_session.lock().unwrap().as_mut() {
                session.position = idx + 1;
            }
            let _ = cb_manager.app.emit(
                "tts-speak-progress",
                serde_json::json!({ "position": idx as u32 + 1, "total": total }),
            );
        });
        self.core
            .speak_sentence_run(sentences, start, Some(on_played))
            .await
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
        if settings.tts_compile {
            // 9x schnellere Synthese (RTF ~0,65 statt ~6), kostet ~60 s beim Start.
            cmd.arg("--compile");
        }
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

    fn fish_dir(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(crate::settings::get_settings(&self.app).tts_fish_dir)
    }

    pub fn list_voice_ids(&self) -> Vec<String> {
        voices::list_voices(&self.fish_dir())
    }

    /// Referenzaufnahme starten (VAD aus — auch leise Passagen gehören in die
    /// Referenz). Stößt parallel das STT-Modell-Laden an, damit das Transkript
    /// beim Stopp ohne Wartezeit entsteht.
    pub fn record_reference_start(&self) -> Result<(), String> {
        use tauri::Manager;
        let tm = self
            .app
            .state::<Arc<crate::managers::transcription::TranscriptionManager>>();
        tm.initiate_model_load();
        let rm = self
            .app
            .state::<Arc<crate::managers::audio::AudioRecordingManager>>();
        rm.try_start_recording(REFERENCE_BINDING, crate::audio_toolkit::VadPolicy::Disabled)
    }

    /// Aufnahme beenden, Samples einbehalten, Transkript per STT liefern.
    /// Ein STT-Fehler verwirft die Aufnahme nicht — das Transkript kommt dann
    /// leer zurück und wird im UI von Hand ergänzt.
    pub fn record_reference_stop(&self) -> Result<String, String> {
        use tauri::Manager;
        let rm = self
            .app
            .state::<Arc<crate::managers::audio::AudioRecordingManager>>();
        let generation = rm.cancel_generation();
        let samples = rm
            .stop_recording(REFERENCE_BINDING, generation)
            .ok_or_else(|| "no reference recording in progress".to_string())?;
        if !voices::reference_long_enough(samples.len()) {
            return Err(format!(
                "Aufnahme zu kurz ({:.1} s) — mindestens {} s einsprechen",
                samples.len() as f32 / 16_000.0,
                voices::MIN_REFERENCE_SECS
            ));
        }
        let tm = self
            .app
            .state::<Arc<crate::managers::transcription::TranscriptionManager>>();
        let transcript = match tm.transcribe(samples.clone()) {
            Ok(text) => text,
            Err(e) => {
                log::warn!("reference transcription failed, keeping audio: {e}");
                String::new()
            }
        };
        *self.pending_reference.lock().unwrap() = Some(samples);
        Ok(transcript)
    }

    /// Einbehaltene Aufnahme unter einem Namen als Stimme speichern.
    /// Rückgabe: die sanierte Stimm-Id.
    pub fn save_pending_voice(&self, name: &str, transcript: &str) -> Result<String, String> {
        let id = voices::sanitize_voice_id(name)
            .ok_or_else(|| "Name ergibt keine gültige Stimm-Id".to_string())?;
        let samples = self
            .pending_reference
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| "keine Referenzaufnahme vorhanden".to_string())?;
        if let Err(e) = voices::save_voice(&self.fish_dir(), &id, &samples, transcript) {
            // Aufnahme zurücklegen, damit ein Tippfehler sie nicht kostet.
            *self.pending_reference.lock().unwrap() = Some(samples);
            return Err(e);
        }
        Ok(id)
    }

    /// WAV-Datei als Stimme übernehmen. Ohne mitgeliefertes Transkript wird
    /// die Datei für die STT auf 16 kHz mono gewandelt und transkribiert; die
    /// Referenz selbst bleibt das unveränderte Original.
    pub fn import_voice_file(
        &self,
        name: &str,
        wav_path: &str,
        transcript: Option<String>,
    ) -> Result<(String, String), String> {
        use tauri::Manager;
        let id = voices::sanitize_voice_id(name)
            .ok_or_else(|| "Name ergibt keine gültige Stimm-Id".to_string())?;
        // Nicht-WAV-Quellen (mp3, m4a, mp4, …) über ffmpeg in hochwertiges
        // Mono-WAV wandeln; WAV geht unverändert durch.
        let (source, _tmp_guard) =
            crate::media::ensure_wav(std::path::Path::new(wav_path), 44_100)?;
        let transcript = match transcript.filter(|t| !t.trim().is_empty()) {
            Some(t) => t,
            None => {
                let samples = voices::load_wav_mono_16k(&source)?;
                let tm = self
                    .app
                    .state::<Arc<crate::managers::transcription::TranscriptionManager>>();
                tm.initiate_model_load();
                tm.transcribe(samples).map_err(|e| {
                    format!("Transkription fehlgeschlagen ({e}) — Transkript bitte manuell angeben")
                })?
            }
        };
        voices::import_voice(&self.fish_dir(), &id, &source, &transcript)?;
        Ok((id, transcript))
    }

    /// Text übersetzen und die Übersetzung sprechen. Die Rückgabe (der
    /// übersetzte Text) kommt sofort; das Sprechen läuft im Hintergrund und
    /// meldet Fehler über die tts-state-changed-Events.
    pub async fn translate_and_speak(
        self: &Arc<Self>,
        text: &str,
        target_lang: &str,
    ) -> Result<String, String> {
        let settings = crate::settings::get_settings(&self.app);
        let translation = crate::translator::translate(&settings, text, target_lang).await?;
        self.speak_in_background(translation.clone());
        Ok(translation)
    }

    /// Aufnahme für die Sprach-zu-Sprach-Übersetzung starten (VAD wie beim
    /// Diktat; STT-Modell wird parallel geladen).
    pub fn record_translate_start(&self) -> Result<(), String> {
        use tauri::Manager;
        let tm = self
            .app
            .state::<Arc<crate::managers::transcription::TranscriptionManager>>();
        tm.initiate_model_load();
        let rm = self
            .app
            .state::<Arc<crate::managers::audio::AudioRecordingManager>>();
        rm.try_start_recording(TRANSLATE_BINDING, crate::audio_toolkit::VadPolicy::Offline)
    }

    /// Aufnahme beenden: transkribieren, übersetzen, Übersetzung sprechen.
    /// Rückgabe: (Transkript, Übersetzung) — das Sprechen läuft im Hintergrund.
    pub async fn record_translate_stop(
        self: &Arc<Self>,
        target_lang: &str,
    ) -> Result<(String, String), String> {
        use tauri::Manager;
        let rm = self
            .app
            .state::<Arc<crate::managers::audio::AudioRecordingManager>>();
        let generation = rm.cancel_generation();
        let samples = rm
            .stop_recording(TRANSLATE_BINDING, generation)
            .ok_or_else(|| "no translate recording in progress".to_string())?;
        if samples.is_empty() {
            return Err("Aufnahme enthielt keine Sprache".into());
        }
        let tm = self
            .app
            .state::<Arc<crate::managers::transcription::TranscriptionManager>>();
        let transcript = tm
            .transcribe(samples)
            .map_err(|e| format!("Transkription fehlgeschlagen: {e}"))?;
        if transcript.trim().is_empty() {
            return Err("Es wurde keine Sprache erkannt".into());
        }
        let translation = self.translate_and_speak(&transcript, target_lang).await?;
        Ok((transcript, translation))
    }

    /// Stimmwechsler: Aufnahme starten (Kaskade Aufnahme → STT → TTS in der
    /// aktiven Stimme; offline, kein Echtzeit-Effekt).
    pub fn record_voicechange_start(&self) -> Result<(), String> {
        use tauri::Manager;
        let tm = self
            .app
            .state::<Arc<crate::managers::transcription::TranscriptionManager>>();
        tm.initiate_model_load();
        let rm = self
            .app
            .state::<Arc<crate::managers::audio::AudioRecordingManager>>();
        rm.try_start_recording(VOICECHANGE_BINDING, crate::audio_toolkit::VadPolicy::Offline)
    }

    /// Stimmwechsler-Aufnahme beenden: transkribieren und in der aktiven
    /// Stimme nachsprechen. Rückgabe: das Transkript (Sprechen läuft im
    /// Hintergrund, Fehler kommen über tts-state-changed).
    pub async fn record_voicechange_stop(self: &Arc<Self>) -> Result<String, String> {
        use tauri::Manager;
        let rm = self
            .app
            .state::<Arc<crate::managers::audio::AudioRecordingManager>>();
        let generation = rm.cancel_generation();
        let samples = rm
            .stop_recording(VOICECHANGE_BINDING, generation)
            .ok_or_else(|| "no voice-change recording in progress".to_string())?;
        let tm = self
            .app
            .state::<Arc<crate::managers::transcription::TranscriptionManager>>();
        let transcript = tm
            .transcribe(samples)
            .map_err(|e| format!("Transkription fehlgeschlagen: {e}"))?;
        if transcript.trim().is_empty() {
            return Err("Es wurde keine Sprache erkannt".into());
        }
        self.speak_in_background(transcript.clone());
        Ok(transcript)
    }

    /// Stimmwechsler für eine Audio-/Videodatei (WAV direkt, alles andere
    /// über ffmpeg): transkribieren und in der aktiven Stimme nachsprechen.
    /// Rückgabe: das Transkript.
    pub async fn respeak_file(self: &Arc<Self>, wav_path: &str) -> Result<String, String> {
        use tauri::Manager;
        let (wav_source, _tmp_guard) =
            crate::media::ensure_wav(std::path::Path::new(wav_path), 16_000)?;
        let samples = voices::load_wav_mono_16k(&wav_source)?;
        let tm = self
            .app
            .state::<Arc<crate::managers::transcription::TranscriptionManager>>();
        tm.initiate_model_load();
        let transcript = tm
            .transcribe(samples)
            .map_err(|e| format!("Transkription fehlgeschlagen: {e}"))?;
        if transcript.trim().is_empty() {
            return Err("In der Datei wurde keine Sprache erkannt".into());
        }
        self.speak_in_background(transcript.clone());
        Ok(transcript)
    }

    fn speak_in_background(self: &Arc<Self>, text: String) {
        let manager = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            manager.refresh_from_settings();
            if let Err(e) = manager.ensure_server().await {
                log::error!("respeak: server start failed: {e}");
                return;
            }
            if let Err(e) = manager.speak_text(&text).await {
                log::warn!("respeak: speaking failed: {e}");
            }
        });
    }

    /// Text in der aktiven Stimme als Audiodatei synthetisieren (ein Request,
    /// ohne Playback) — der Datei-Export. Format aus `tts_export_format`
    /// (wav/mp3/opus, der Fish-Server encodiert direkt).
    pub async fn synthesize_to_file(&self, text: &str, out_path: &str) -> Result<usize, String> {
        self.refresh_from_settings();
        self.ensure_server().await?;
        let prepared = {
            let max_chars = *self.core.max_chars.lock().unwrap();
            protocol::prepare_text(text, max_chars).ok_or_else(|| "empty text".to_string())?
        };
        let port = *self.core.port.lock().unwrap();
        let seed = *self.core.seed.lock().unwrap();
        let voice = self.core.voice.lock().unwrap().clone();
        let format = self.core.export_format.lock().unwrap().clone();
        let url = format!("{}/v1/tts", protocol::base_url(port));
        let body =
            protocol::tts_request_body_in_format(&prepared.text, seed, voice.as_deref(), &format);
        let resp = self
            .core
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
        let audio = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
        if !protocol::looks_like_audio(&audio, &format) {
            return Err(format!("TTS response is not valid {format} audio"));
        }
        std::fs::write(out_path, &audio).map_err(|e| format!("could not write {out_path}: {e}"))?;
        *self.core.last_used.lock().unwrap() = Instant::now();
        Ok(audio.len())
    }

    /// Aktuell konfiguriertes Export-Format ("wav" | "mp3" | "opus") — für
    /// den Save-Dialog des Frontends.
    pub fn export_format(&self) -> String {
        crate::settings::get_settings(&self.app).tts_export_format
    }

    // ------------------------------------------------------------------
    // Hörbuch / Dokument-Vorlesen mit persistentem Fortschritt
    // ------------------------------------------------------------------

    fn reading_store(&self) -> Option<std::sync::Arc<tauri_plugin_store::Store<tauri::Wry>>> {
        use tauri_plugin_store::StoreExt;
        self.app
            .store(crate::portable::store_path(READING_STORE))
            .map_err(|e| log::warn!("reading store unavailable: {e}"))
            .ok()
    }

    fn stored_reading(&self, key: &str) -> Option<ReadingInfo> {
        let value = self.reading_store()?.get(key)?;
        Some(ReadingInfo {
            key: key.to_string(),
            title: value["title"].as_str().unwrap_or(key).to_string(),
            position: value["position"].as_u64().unwrap_or(0) as u32,
            total: value["total"].as_u64().unwrap_or(0) as u32,
            finished: value["finished"].as_bool().unwrap_or(false),
            playing: false,
        })
    }

    fn persist_reading(&self, key: &str, title: &str, position: u32, total: u32) {
        if let Some(store) = self.reading_store() {
            store.set(
                key.to_string(),
                serde_json::json!({
                    "title": title,
                    "position": position,
                    "total": total,
                    "finished": position >= total && total > 0,
                    "updated": chrono::Utc::now().to_rfc3339(),
                }),
            );
        }
    }

    fn emit_reading(&self, info: &ReadingInfo) {
        use tauri::Emitter;
        if let Err(e) = self.app.emit("tts-reading-progress", info.clone()) {
            log::warn!("Could not emit tts-reading-progress: {e}");
        }
    }

    /// Dokument öffnen (txt/md/pdf/docx): Text extrahieren, in Sätze teilen,
    /// gespeicherten Fortschritt übernehmen. Der Eintrag erscheint sofort in
    /// der Bibliotheksliste.
    pub fn reading_open(&self, path: &str) -> Result<ReadingInfo, String> {
        let p = std::path::Path::new(path);
        let text = crate::media::extract_document_text(p)?;
        let sentences = protocol::split_sentences(&text);
        let total = sentences.len() as u32;
        if total == 0 {
            return Err("Das Dokument enthält keine vorlesbaren Sätze".into());
        }
        let title = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
            .to_string();
        let position = self
            .stored_reading(path)
            .map(|info| state::resume_position(info.position, total))
            .unwrap_or(0);
        *self.reading.lock().unwrap() = Some(ReadingSession {
            key: path.to_string(),
            title: title.clone(),
            sentences,
        });
        self.persist_reading(path, &title, position, total);
        let info = ReadingInfo {
            key: path.to_string(),
            title,
            position,
            total,
            finished: false,
            playing: false,
        };
        self.emit_reading(&info);
        Ok(info)
    }

    /// Wiedergabe des geöffneten Dokuments ab der gespeicherten Position.
    /// Kehrt sofort zurück; Fortschritt kommt als `tts-reading-progress`.
    pub fn reading_play(self: &Arc<Self>) -> Result<(), String> {
        let (key, title, sentences) = {
            let guard = self.reading.lock().unwrap();
            let session = guard.as_ref().ok_or("kein Dokument geöffnet")?;
            (
                session.key.clone(),
                session.title.clone(),
                session.sentences.clone(),
            )
        };
        let total = sentences.len() as u32;
        let start = self
            .stored_reading(&key)
            .map(|info| state::resume_position(info.position, total))
            .unwrap_or(0);

        let manager = Arc::clone(self);
        let task_key = key.clone();
        let task_title = title.clone();
        tauri::async_runtime::spawn(async move {
            let (key, title) = (task_key, task_title);
            manager.refresh_from_settings();
            if let Err(e) = manager.ensure_server().await {
                log::error!("reading: server start failed: {e}");
                return;
            }
            let cb_manager = Arc::clone(&manager);
            let cb_key = key.clone();
            let cb_title = title.clone();
            let on_played: Arc<dyn Fn(usize) + Send + Sync> = Arc::new(move |idx| {
                let position = idx as u32 + 1;
                cb_manager.persist_reading(&cb_key, &cb_title, position, total);
                cb_manager.emit_reading(&ReadingInfo {
                    key: cb_key.clone(),
                    title: cb_title.clone(),
                    position,
                    total,
                    finished: position >= total,
                    playing: position < total,
                });
            });
            let result = manager
                .core
                .speak_sentence_run(sentences, start as usize, Some(on_played))
                .await;
            if let Err(e) = result {
                log::warn!("reading: playback ended with error: {e}");
            }
            // Endzustand melden (Pause oder fertig): playing=false.
            if let Some(info) = manager.stored_reading(&key) {
                manager.emit_reading(&info);
            }
        });
        // Startzustand sofort melden.
        self.emit_reading(&ReadingInfo {
            key,
            title,
            position: start,
            total,
            finished: false,
            playing: true,
        });
        Ok(())
    }

    /// Pause = Abbruch des laufenden Sprechens; die Position des letzten
    /// vollständig gehörten Satzes ist bereits persistiert.
    pub fn reading_pause(&self) {
        self.core.cancel_core();
    }

    /// Bibliothek: alle gespeicherten Dokumente mit Fortschritt.
    pub fn reading_list(&self) -> Vec<ReadingInfo> {
        let Some(store) = self.reading_store() else {
            return Vec::new();
        };
        let mut list: Vec<ReadingInfo> = store
            .keys()
            .into_iter()
            .filter_map(|key| self.stored_reading(&key))
            .collect();
        list.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        list
    }

    /// Fortschritt eines Dokuments auf Anfang zurücksetzen.
    pub fn reading_reset(&self, key: &str) -> Result<(), String> {
        let info = self.stored_reading(key).ok_or("unbekanntes Dokument")?;
        self.persist_reading(key, &info.title, 0, info.total);
        self.emit_reading(&ReadingInfo {
            position: 0,
            finished: false,
            playing: false,
            ..info
        });
        Ok(())
    }

    /// Satzweises Springen im geöffneten Dokument (delta z. B. -1/+1).
    /// Läuft die Wiedergabe, setzt sie an der neuen Position fort.
    pub fn reading_seek(self: &Arc<Self>, delta: i32) -> Result<ReadingInfo, String> {
        let (key, title, total) = {
            let guard = self.reading.lock().unwrap();
            let session = guard.as_ref().ok_or("kein Dokument geöffnet")?;
            (
                session.key.clone(),
                session.title.clone(),
                session.sentences.len() as u32,
            )
        };
        let current = self.stored_reading(&key).map(|i| i.position).unwrap_or(0);
        let new_pos = (i64::from(current) + i64::from(delta)).clamp(0, i64::from(total) - 1) as u32;
        let was_playing = self.core.phase() == TtsPhase::Speaking;
        self.persist_reading(&key, &title, new_pos, total);
        let info = ReadingInfo {
            key,
            title,
            position: new_pos,
            total,
            finished: false,
            playing: was_playing,
        };
        self.emit_reading(&info);
        if was_playing {
            self.core.cancel_core();
            self.reading_play()?;
        }
        Ok(info)
    }

    /// Dokument aus der Bibliothek entfernen (Datei bleibt unberührt).
    pub fn reading_remove(&self, key: &str) -> Result<(), String> {
        if let Some(store) = self.reading_store() {
            store.delete(key.to_string());
        }
        let mut guard = self.reading.lock().unwrap();
        if guard.as_ref().is_some_and(|s| s.key == key) {
            *guard = None;
        }
        Ok(())
    }

    /// Stimme löschen; war sie aktiv, fällt die Auswahl auf die
    /// Seed-Standardstimme zurück.
    pub fn delete_voice_id(&self, id: &str) -> Result<(), String> {
        voices::delete_voice(&self.fish_dir(), id)?;
        let mut settings = crate::settings::get_settings(&self.app);
        if settings.tts_voice.as_deref() == Some(id) {
            settings.tts_voice = None;
            crate::settings::write_settings(&self.app, settings);
        }
        self.refresh_from_settings();
        Ok(())
    }

    /// Selbsttest-Messpfad: Server sicherstellen, WAV holen (ohne Playback),
    /// Zeiten melden. Rückgabe: (wav, server_start_ms, tts_ms) —
    /// server_start_ms ist 0, wenn ein Server bereits lief. `voice_override`
    /// übersteuert die persistierte Stimme nur für diesen Lauf.
    pub async fn bench_fetch(
        &self,
        text: &str,
        voice_override: Option<&str>,
    ) -> Result<(Vec<u8>, u64, u64), String> {
        self.refresh_from_settings();
        if let Some(voice) = voice_override {
            *self.core.voice.lock().unwrap() = Some(voice.to_string());
        }
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
        Ok((wav, server_start_ms, tts_ms))
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
    async fn spawn_mock(
        tts_calls: Arc<AtomicUsize>,
        tts_bodies: Arc<std::sync::Mutex<Vec<String>>>,
    ) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let calls = tts_calls.clone();
                let bodies = tts_bodies.clone();
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
                                    let received = String::from_utf8_lossy(
                                        &buf[header_end + 4..header_end + 4 + content_length],
                                    )
                                    .to_string();
                                    bodies.lock().unwrap().push(received);
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
        let port = spawn_mock(calls, Arc::new(std::sync::Mutex::new(Vec::new()))).await;
        let core = TtsCore::for_test(port);
        core.ensure_server_core().await.unwrap();
        assert_eq!(core.phase(), TtsPhase::Ready);
        assert!(!core.owns_server(), "extern erkannt → kein Besitz, kein Kill");
    }

    #[tokio::test]
    async fn speak_fetches_wav_and_hands_it_to_the_player() {
        let calls = Arc::new(AtomicUsize::new(0));
        let port = spawn_mock(calls.clone(), Arc::new(std::sync::Mutex::new(Vec::new()))).await;
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
        let port = spawn_mock(calls.clone(), Arc::new(std::sync::Mutex::new(Vec::new()))).await;
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
    async fn multi_sentence_text_is_pipelined_as_separate_requests() {
        let calls = Arc::new(AtomicUsize::new(0));
        let bodies = Arc::new(std::sync::Mutex::new(Vec::new()));
        let port = spawn_mock(calls.clone(), bodies.clone()).await;
        let core = TtsCore::for_test(port);
        core.ensure_server_core().await.unwrap();
        let total = core
            .speak_core("Der erste Satz ist lang genug. Der zweite Satz ist es ebenfalls.")
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2, "ein Request pro Satz");
        assert!(total > 2 * 1024, "beide WAVs gezählt");
        let all = bodies.lock().unwrap().join("|");
        assert!(all.contains("Der erste Satz"));
        assert!(all.contains("Der zweite Satz"));
    }

    #[tokio::test]
    async fn unchanged_text_is_served_from_the_cache_on_replay() {
        let calls = Arc::new(AtomicUsize::new(0));
        let bodies = Arc::new(std::sync::Mutex::new(Vec::new()));
        let port = spawn_mock(calls.clone(), bodies).await;
        let core = TtsCore::for_test(port);
        core.ensure_server_core().await.unwrap();
        let text = "Dieser Satz ist lang genug für den Cache-Test.";
        core.speak_core(text).await.unwrap();
        core.speak_core(text).await.unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "der zweite, unveränderte Lauf kommt aus dem Cache"
        );
        core.speak_core("Ein anderer Satz erzwingt eine neue Synthese.")
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2, "neuer Text → neuer Request");
    }

    #[tokio::test]
    async fn a_selected_voice_travels_as_reference_id() {
        let calls = Arc::new(AtomicUsize::new(0));
        let bodies = Arc::new(std::sync::Mutex::new(Vec::new()));
        let port = spawn_mock(calls, bodies.clone()).await;
        let core = TtsCore::for_test(port);
        *core.voice.lock().unwrap() = Some("patrick".into());
        core.ensure_server_core().await.unwrap();
        core.speak_core("Hallo").await.unwrap();
        let all = bodies.lock().unwrap().join("");
        assert!(
            all.contains(r#""reference_id":"patrick""#),
            "Request muss die Stimme tragen, war: {all}"
        );
        assert!(
            all.contains(r#""use_memory_cache":"on""#),
            "Referenz-Cache muss aktiv sein"
        );
    }

    #[tokio::test]
    async fn cancel_marks_the_running_jobs_flag() {
        let core = TtsCore::for_test(1);
        let flag = core.cancelled.lock().unwrap().clone();
        assert!(!flag.load(Ordering::Acquire));
        core.cancel_core();
        assert!(flag.load(Ordering::Acquire), "cancel muss den laufenden Auftrag treffen");
    }

    /// Regression (19.08.2026): Nach Pause blieb die Phase auf „Spricht"
    /// hängen — Server-Stopp wirkte blockiert und der Idle-Stopp griff nie.
    #[tokio::test]
    async fn cancel_returns_a_speaking_phase_to_ready() {
        let core = TtsCore::for_test(1);
        core.set_phase(TtsPhase::Speaking, None);
        core.cancel_core();
        assert_eq!(core.phase(), TtsPhase::Ready);
        // In anderen Phasen (z. B. Starting) mischt sich cancel nicht ein.
        core.set_phase(TtsPhase::Starting, None);
        core.cancel_core();
        assert_eq!(core.phase(), TtsPhase::Starting);
    }
}
