//! Vorlesen (TP1): Fish-Speech-TTS-Anbindung.
//!
//! `protocol` und `state` sind pure, I/O-freie Bausteine. `TtsCore` bündelt
//! die app-unabhängige Logik (HTTP, Phase, Abbruch, Besitz) und ist gegen
//! einen Mock-Server getestet; `TtsManager` ergänzt AppHandle-Belange:
//! Settings, Events, Prozess-Spawn, Idle-Watchdog und Exit-Teardown.

pub mod compile_cache;
pub mod loudness;
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

/// Ein zu sprechender Satz und die Stimme dafür. `None` heißt „die
/// eingestellte Stimme"; damit ist einstimmiges Vorlesen der Sonderfall
/// „überall None" und braucht keinen eigenen Pfad.
pub type Utterance = (String, Option<String>);

/// Sätze, die alle die eingestellte Stimme sprechen.
pub fn single_voice(sentences: Vec<String>) -> Vec<Utterance> {
    sentences.into_iter().map(|text| (text, None)).collect()
}
const IDLE_WATCH_INTERVAL: Duration = Duration::from_secs(30);

/// Wie weit ein einzelner Satz vom Pegel seiner Stimme abweichen darf.
/// Groß genug, damit jeder Satz den Zielpegel praktisch erreicht; klein
/// genug, dass die Betonung eines Satzes erhalten bleibt.
const SENTENCE_TRIM_DB: f32 = 3.0;

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
    /// Persistenter Ableger des Caches auf Platte — bereits synthetisierte
    /// Bücher/Dokumente sind damit auch OHNE laufenden Fish-Server anhörbar.
    cache_dir: Mutex<Option<std::path::PathBuf>>,
    /// Genau EIN Startversuch zur Zeit. Bewusst atomar und nicht ueber die
    /// Phase geprueft: die Phasenpruefung lag VOR dem Spawn, das Setzen der
    /// Phase danach — dazwischen lagen eine Gesundheitsabfrage und ein
    /// Prozessstart. Zwei Ausloeser in diesem Fenster (etwa Vorlesen und ein
    /// Stimmwechsel) starteten beide einen Server; der zweite belegte weitere
    /// 17 GB VRAM und gehoerte niemandem. Beobachtet am 21.08.2026.
    start_claim: AtomicBool,
    /// Der Nutzer hat waehrend eines laufenden Starts abgebrochen. Dann darf
    /// kein Wiederholungsversuch anlaufen — sonst startet die App genau das
    /// wieder, was gerade beendet wurde.
    stop_requested: AtomicBool,
    /// Ob die Wiedergabe alle Stimmen auf denselben Pegel zieht.
    normalize: AtomicBool,
    /// Korrekturfaktor je Stimme, einmal je Sitzung aus dem ersten
    /// synthetisierten Satz dieser Stimme gemessen. Schlüssel ist die
    /// reference_id, leer für die Seed-Standardstimme.
    ///
    /// Warum nicht Satz für Satz: die Lautheit schwankt zwischen Sätzen
    /// derselben Stimme absichtlich — ein Fragesatz ist anders betont als
    /// eine Aufzählung. Wer jeden Satz einzeln auf denselben Wert zöge,
    /// bügelte diese Betonung glatt und erzeugte hörbares Pumpen. Was
    /// wirklich stört, ist der Sprung ZWISCHEN Stimmen; genau den nimmt ein
    /// konstanter Faktor je Stimme heraus.
    voice_gains: Mutex<std::collections::HashMap<String, f32>>,
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
            cache_dir: Mutex::new(None),
            start_claim: AtomicBool::new(false),
            stop_requested: AtomicBool::new(false),
            normalize: AtomicBool::new(true),
            voice_gains: Mutex::new(std::collections::HashMap::new()),
            on_phase_change: Mutex::new(None),
        }
    }

    fn disk_cache_path(&self, key: u64) -> Option<std::path::PathBuf> {
        self.cache_dir
            .lock()
            .unwrap()
            .as_ref()
            .map(|dir| dir.join(format!("{key:016x}.wav")))
    }

    /// Ist dieser Satz (mit aktueller Stimme/Seed) bereits synthetisiert —
    /// im RAM oder auf Platte?
    pub fn has_cached(&self, text: &str) -> bool {
        let seed = *self.seed.lock().unwrap();
        let voice = self.voice.lock().unwrap().clone();
        let key = WavCache::key(text, seed, voice.as_deref());
        if self.wav_cache.lock().unwrap().get(key).is_some() {
            return true;
        }
        self.disk_cache_path(key).is_some_and(|p| p.exists())
    }

    fn set_phase(&self, phase: TtsPhase, message: Option<String>) {
        {
            let mut slot = self.phase.lock().unwrap();
            if *slot != phase {
                log::info!("tts phase: {:?} -> {:?}", *slot, phase);
            }
            *slot = phase;
        }
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
            // `owns_server` wird hier NICHT mehr auf false gesetzt. Der Wert ist
            // nur an zwei Stellen wahr gemeint: true beim eigenen Spawn, false
            // beim eigenen Stopp. Diese Zeile hat ihn bei JEDER Gesundheits-
            // pruefung auf false gezwungen — also auch fuer einen Server, den
            // die App selbst gestartet hatte. Danach hielt sie ihn fuer fremd,
            // "Server stoppen" war ausgegraut und der Prozess blieb mit seinem
            // VRAM stehen, bis jemand ihn im Taskmanager erschoss. Wer nichts
            // gespawnt hat, hat hier ohnehin schon false stehen.
            //
            // NICHT waehrend eines laufenden Auftrags auf `Ready` stellen:
            // die Phase ist zugleich die Anzeige "spricht gerade", und die
            // Oberflaeche haengt ihren Stopp-Knopf daran. Ein Serverbefund
            // mitten im Vorlesen hat die Anzeige auf "Bereit" zurueckgesetzt —
            // damit war der Knopf ausgegraut und das Vorlesen nicht mehr zu
            // beenden.
            if !matches!(self.phase(), TtsPhase::Speaking | TtsPhase::Starting) {
                self.set_phase(TtsPhase::Ready, None);
            }
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
        let sentences = single_voice(protocol::split_sentences(&prepared.text));
        self.speak_sentence_run(sentences, 0, None, None).await
    }

    /// Gemeinsamer Sprechpfad für Freitext und Hörbuch: Sätze pipelined
    /// sprechen, ab `start_index`, mit optionalem Callback nach jedem
    /// VOLLSTÄNDIG abgespielten Satz (absoluter Index) — die Basis für die
    /// persistente Fortschrittsanzeige.
    pub async fn speak_sentence_run(
        &self,
        sentences: Vec<Utterance>,
        start_index: usize,
        on_playing: Option<Arc<dyn Fn(usize) + Send + Sync>>,
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
                on_playing,
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
    /// `voice = None` heisst "die eingestellte Stimme" — so bleibt der
    /// einstimmige Pfad exakt wie vorher. Ein `Some(..)` uebersteuert sie fuer
    /// genau diesen Satz; das ist die Grundlage des Dialog-Vorlesens.
    async fn fetch_wav(
        &self,
        port: u16,
        seed: i64,
        text: &str,
        voice: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        let url = format!("{}/v1/tts", protocol::base_url(port));
        let voice = match voice {
            Some(explicit) => Some(explicit.to_string()),
            None => self.voice.lock().unwrap().clone(),
        };
        // Unveränderter Satz + gleiche Stimme/Seed → aus dem Cache, ohne Server.
        let cache_key = WavCache::key(text, seed, voice.as_deref());
        if let Some(cached) = self.wav_cache.lock().unwrap().get(cache_key) {
            return Ok(cached);
        }
        // Platten-Cache: macht bereits Vorgelesenes offline abspielbar.
        if let Some(path) = self.disk_cache_path(cache_key) {
            if let Ok(bytes) = std::fs::read(&path) {
                if protocol::looks_like_wav(&bytes) {
                    self.wav_cache
                        .lock()
                        .unwrap()
                        .insert(cache_key, bytes.clone());
                    return Ok(bytes);
                }
            }
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
        self.wav_cache
            .lock()
            .unwrap()
            .insert(cache_key, bytes.clone());
        if let Some(path) = self.disk_cache_path(cache_key) {
            if let Err(e) = std::fs::write(&path, &bytes) {
                log::warn!("could not persist tts cache file: {e}");
            }
        }
        Ok(bytes)
    }

    /// Wiedergabefaktor für einen synthetisierten Satz.
    ///
    /// Zwei Stufen. Der Pegel der *Stimme* ist der gleitende Mittelwert aller
    /// bisher gehörten Sätze dieser Stimme — nicht die Messung des ersten:
    /// ein kurzer Einstiegssatz misst leicht daneben, und dieser Fehler
    /// bliebe sonst für die ganze Sitzung stehen. Darauf kommt die Korrektur
    /// des *Satzes*, gedämpft auf ±3 dB um den Stimmenpegel — so wird jeder
    /// Satz auf den Zielpegel gezogen, ohne dass Betonung glattgebügelt wird
    /// oder die Lautheit zwischen zwei Sätzen hörbar pumpt.
    fn playback_gain(&self, voice: Option<&str>, wav: &[u8]) -> f32 {
        if !self.normalize.load(Ordering::Acquire) {
            return 1.0;
        }
        let key = match voice {
            Some(explicit) => explicit.to_string(),
            // "die eingestellte Stimme" muss denselben Schlüssel ergeben wie
            // ihr expliziter Name — sonst bekäme dieselbe Stimme zwei Faktoren.
            None => self.voice.lock().unwrap().clone().unwrap_or_default(),
        };
        let Some((mono, rate, peak)) = decode_wav(wav) else {
            return 1.0;
        };
        let sentence = loudness::gain_to_target(&mono, rate, peak);

        // Gemittelt wird in dB, nicht im Faktor: Lautheit ist logarithmisch,
        // der arithmetische Mittelwert zweier Faktoren träfe die Mitte nicht.
        let base = {
            let mut gains = self.voice_gains.lock().unwrap();
            let mixed = match gains.get(&key) {
                Some(&previous) => {
                    let db = |g: f32| 20.0 * g.max(1e-6).log10();
                    10f32.powf((db(previous) * 0.75 + db(sentence) * 0.25) / 20.0)
                }
                None => sentence,
            };
            gains.insert(key, mixed);
            mixed
        };

        let limit = 10f32.powf(SENTENCE_TRIM_DB / 20.0);
        let corrected = sentence.clamp(base / limit, base * limit);
        if peak <= f32::EPSILON {
            return 1.0;
        }
        // Die Spitze hat immer das letzte Wort: die Dämpfung oben darf den
        // Faktor wieder über die Aussteuerungsgrenze gehoben haben.
        corrected.min(loudness::PEAK_CEILING / peak)
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
        sentences: &[Utterance],
        start_index: usize,
        cancelled: Arc<AtomicBool>,
        on_playing: Option<Arc<dyn Fn(usize) + Send + Sync>>,
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

        for (offset, (sentence, voice)) in sentences.iter().enumerate().skip(start_index) {
            if cancelled.load(Ordering::Acquire) {
                break;
            }
            // Einzelne Sätze absichern (leere überspringen, Monster kappen).
            let Some(prepared) = protocol::prepare_text(sentence, max_chars) else {
                notify(offset, cancelled.load(Ordering::Acquire));
                continue;
            };
            // Der naechste Satz wird geholt, WAEHREND der vorige noch spielt
            // (siehe `previous_playback` unten) — deshalb faellt ein
            // Stimmwechsel nicht als Pause auf, solange der Server die
            // Referenz im Speicher haelt (`use_memory_cache`).
            match self
                .fetch_wav(port, seed, &prepared.text, voice.as_deref())
                .await
            {
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
                    // Live-Anzeige: dieser Satz beginnt jetzt zu spielen.
                    if let Some(cb) = on_playing.as_ref() {
                        cb(offset);
                    }
                    let player = self.player.clone();
                    let device = self.output_device.lock().unwrap().clone();
                    // Stimmen gleich laut: der Nutzerregler skaliert den
                    // ausgeglichenen Pegel, nicht den rohen des Servers.
                    let volume =
                        *self.volume.lock().unwrap() * self.playback_gain(voice.as_deref(), &bytes);
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
        self.cancelled
            .lock()
            .unwrap()
            .store(true, Ordering::Release);
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
    /// Abbruch-Flag des laufenden Datei-Exports. EIGENES Flag, nicht das der
    /// Wiedergabe: sonst würde ein Klick auf Stopp im Player den Export
    /// abwürgen (und umgekehrt) — zwei Vorgänge, zwei Schalter.
    export_cancel: Mutex<Arc<AtomicBool>>,
}

struct SpeakSession {
    /// Satz samt Stimme — sonst spraeche ein "Fortsetzen" den Rest eines
    /// Dialogs mit der falschen Stimme weiter.
    sentences: Vec<Utterance>,
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

/// Obergrenze des Platten-Caches; darüber fliegen die ältesten Dateien.
const DISK_CACHE_LIMIT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// FIFO-Verdrängung nach Änderungszeit — läuft einmal beim App-Start.
/// Haelt die Startsperre, solange ein Startversuch laeuft, und gibt sie beim
/// Verlassen wieder frei — auch auf jedem Fehlerpfad. Genau deshalb ein
/// Drop-Typ und kein Flag von Hand: ein vergessener Ruecksetzer bedeutete,
/// dass der Server nie wieder startet.
struct StartClaim<'a>(&'a AtomicBool);

impl Drop for StartClaim<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Die aussagekraeftigsten Zeilen aus dem Startprotokoll des Servers.
///
/// Der Kindprozess schrieb seine Ausgabe bisher nach `Stdio::null()`. Faellt
/// er beim Start um, sah der Nutzer nur "exit code: 3" — die Erklaerung stand
/// derweil in einem Traceback, den niemand je zu Gesicht bekam. Beobachtet am
/// 21.08.2026: hinter Code 3 steckte eine durch einen Bluescheck zerstoerte
/// Datei im Compile-Cache von PyTorch, sichtbar nur im Traceback.
///
/// Gesucht wird die letzte Zeile, die wie eine Fehlerursache aussieht
/// (Exception-Zeilen tragen sie in Python am Ende), sonst die letzte nicht
/// leere Zeile. Bewusst wenige Zeichen: das gehoert in eine Fehlermeldung,
/// nicht in ein Protokollfenster — der vollstaendige Text steht in der Datei.
pub fn startup_error_summary(log: &str) -> Option<String> {
    const MARKERS: [&str; 6] = [
        "Error",
        "error:",
        "Exception",
        "raised",
        "failed",
        "Traceback",
    ];
    let lines: Vec<&str> = log
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    let picked = lines
        .iter()
        .rev()
        // Die Zeile mit dem Ursachenwort, aber nicht die Rahmenzeilen des
        // Tracebacks selbst ("Traceback (most recent call last):", "File ...").
        .find(|l| {
            MARKERS.iter().any(|m| l.contains(m))
                && !l.starts_with("File \"")
                && !l.starts_with("Traceback")
        })
        .or_else(|| lines.last())?;
    let mut summary: String = picked.chars().take(300).collect();
    if picked.chars().count() > 300 {
        summary.push('…');
    }
    Some(summary)
}

/// WAV-Blob zu Mono-Downmix, Abtastrate und Spitzenwert.
///
/// `None` bei allem, was `hound` nicht lesen kann — die Wiedergabe läuft dann
/// ungeregelt weiter, statt am Pegelmessen zu scheitern.
fn decode_wav(bytes: &[u8]) -> Option<(Vec<f32>, u32, f32)> {
    let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes)).ok()?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>().ok()?,
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<Result<_, _>>()
                .ok()?
        }
    };
    let peak = loudness::peak(&samples);
    let channels = spec.channels.max(1) as usize;
    let mono = if channels == 1 {
        samples
    } else {
        samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };
    Some((mono, spec.sample_rate, peak))
}

/// WAV-Blob auf `loudness::TARGET_LUFS` gezogen neu schreiben (16-bit PCM).
///
/// `None`, wenn der Blob nicht lesbar ist oder ohnehin schon passt — der
/// Aufrufer behält dann das Original, statt am Pegeln zu scheitern.
fn normalize_wav_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    let (mono, rate, peak) = decode_wav(bytes)?;
    let gain = loudness::gain_to_target(&mono, rate, peak);
    if (gain - 1.0).abs() < 0.01 {
        return None;
    }
    let reader = hound::WavReader::new(std::io::Cursor::new(bytes)).ok()?;
    let spec = reader.spec();
    let out_spec = hound::WavSpec {
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
        ..spec
    };
    // Kanäle bleiben, wie sie sind: gemessen wurde über den Downmix, der
    // Faktor gilt für alle Kanäle gleichermaßen.
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut out, out_spec).ok()?;
        let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes)).ok()?;
        let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
        match spec.sample_format {
            hound::SampleFormat::Float => {
                for sample in reader.samples::<f32>() {
                    let v = (sample.ok()? * gain).clamp(-1.0, 1.0) * i16::MAX as f32;
                    writer.write_sample(v as i16).ok()?;
                }
            }
            hound::SampleFormat::Int => {
                for sample in reader.samples::<i32>() {
                    let v = (sample.ok()? as f32 / scale * gain).clamp(-1.0, 1.0) * i16::MAX as f32;
                    writer.write_sample(v as i16).ok()?;
                }
            }
        }
        writer.finalize().ok()?;
    }
    Some(out.into_inner())
}

/// Marker, dass die Hörproben im Verzeichnis mit Pegelausgleich entstanden.
const DEMOS_NORMALIZED_MARKER: &str = ".loudness-v2";

/// Einmalig alle Hörproben löschen, die noch ohne Pegelausgleich entstanden
/// sind. Sie werden beim nächsten Anhören neu erzeugt — das kostet einmal
/// wenige Sekunden GPU-Zeit und ist der einzige Weg, sie loszuwerden, ohne
/// ihnen anzusehen, wie sie entstanden sind.
fn discard_stale_demos(dir: &std::path::Path) {
    if dir.join(DEMOS_NORMALIZED_MARKER).exists() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut removed = 0usize;
    for path in entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "wav"))
    {
        if std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    if let Err(e) = std::fs::write(dir.join(DEMOS_NORMALIZED_MARKER), b"") {
        log::warn!("could not mark demo dir: {e}");
        return;
    }
    if removed > 0 {
        log::info!("{removed} Hörprobe(n) ohne Pegelausgleich verworfen");
    }
}

fn prune_disk_cache(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::path::PathBuf, std::time::SystemTime, u64)> = entries
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some((e.path(), meta.modified().ok()?, meta.len()))
        })
        .collect();
    let total: u64 = files.iter().map(|(_, _, len)| len).sum();
    if total <= DISK_CACHE_LIMIT_BYTES {
        return;
    }
    files.sort_by_key(|(_, modified, _)| *modified);
    let mut remaining = total;
    for (path, _, len) in files {
        if remaining <= DISK_CACHE_LIMIT_BYTES {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            remaining -= len;
        }
    }
    log::info!("tts cache pruned to {} MB", remaining / (1024 * 1024));
}

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
            export_cancel: Mutex::new(Arc::new(AtomicBool::new(false))),
        });
        manager.refresh_from_settings();

        // Persistenter Audio-Cache: macht bereits Vorgelesenes offline
        // (ohne Fish-Server) abspielbar. Begrenzung siehe prune_disk_cache.
        {
            use tauri::Manager;
            let base = crate::portable::data_dir()
                .cloned()
                .or_else(|| app.path().app_local_data_dir().ok());
            if let Some(dir) = base.map(|b| b.join("tts_cache")) {
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    log::warn!("tts cache dir unavailable: {e}");
                } else {
                    *manager.core.cache_dir.lock().unwrap() = Some(dir.clone());
                    std::thread::spawn(move || prune_disk_cache(&dir));
                }
            }
        }

        // Bestandsstimmen einmalig auf das Lautheitsmaß nachziehen. Im
        // Hintergrund: der Lauf liest und schreibt Dateien und darf den
        // App-Start nicht aufhalten.
        {
            let fish_dir = manager.fish_dir();
            let demos = manager.demo_dir();
            std::thread::spawn(move || {
                let count = voices::renormalize_existing(&fish_dir);
                if count > 0 {
                    log::info!("{count} Referenzstimme(n) auf -20 LUFS nachgezogen");
                }
                // Hörproben aus der Zeit vor dem Ausgleich verwerfen. Sie
                // entstehen sonst nie neu: nachgezogen wird eine Hörprobe nur,
                // wenn die Referenz JÜNGER ist als sie — und das ist sie nach
                // diesem Lauf gerade nicht mehr.
                if let Some(dir) = demos {
                    discard_stale_demos(&dir);
                }
            });
        }

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
        // Beim Umschalten die gemessenen Faktoren verwerfen: sonst hinge der
        // Pegel an einer Messung aus der Zeit vor dem Umschalten.
        let previous = self
            .core
            .normalize
            .swap(settings.tts_normalize, Ordering::Release);
        if previous != settings.tts_normalize {
            self.core.voice_gains.lock().unwrap().clear();
        }
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
        let prepared =
            protocol::prepare_text(raw, max_chars).ok_or_else(|| "empty text".to_string())?;
        let sentences = self.utterances(&prepared.text);
        *self.speak_session.lock().unwrap() = Some(SpeakSession {
            sentences: sentences.clone(),
            position: 0,
        });
        self.run_speak_session(sentences, 0).await
    }

    /// Vorlesetext in Saetze samt Stimme zerlegen.
    ///
    /// Beginnt eine Zeile mit dem Namen einer vorhandenen Stimme und einem
    /// Doppelpunkt (`olga:`), spricht diese Stimme bis zur naechsten solchen
    /// Zeile — daraus entsteht ein Dialog. Ohne jede Markierung ist das
    /// Ergebnis Satz fuer Satz dasselbe wie vorher, nur mit `None` als Stimme.
    ///
    /// Satztrennung passiert INNERHALB eines Sprecherabschnitts: ein Satz darf
    /// nie zwei Sprecher enthalten, und die Pipeline holt den naechsten Satz
    /// bereits waehrend der vorige spielt — deshalb klingt der Wechsel fluessig.
    fn utterances(&self, text: &str) -> Vec<Utterance> {
        let known = self.list_voice_ids();
        protocol::split_voice_segments(text, &known)
            .into_iter()
            .flat_map(|segment| {
                protocol::split_sentences(&segment.text)
                    .into_iter()
                    .map(move |sentence| (sentence, segment.voice.clone()))
            })
            .collect()
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

    /// Alles im Cache → gar keinen Server anfassen (Offline-Wiedergabe);
    /// sonst normal sicherstellen. Vorher refresh, damit Stimme/Seed für die
    /// Cache-Schlüssel aktuell sind.
    pub async fn ensure_server_for(&self, sentences: &[String]) -> Result<(), String> {
        if !sentences.is_empty() && sentences.iter().all(|s| self.core.has_cached(s)) {
            log::info!("playback served entirely from cache — no server needed");
            return Ok(());
        }
        self.ensure_server().await
    }

    async fn run_speak_session(
        self: &Arc<Self>,
        sentences: Vec<Utterance>,
        start: usize,
    ) -> Result<usize, String> {
        use tauri::Emitter;
        self.refresh_from_settings();
        let texts: Vec<String> = sentences[start.min(sentences.len())..]
            .iter()
            .map(|(text, _)| text.clone())
            .collect();
        self.ensure_server_for(&texts).await?;
        // Standardstimme an ihre Referenz binden, damit sie ueber Saetze
        // hinweg dieselbe bleibt (siehe ensure_seed_reference).
        self.bind_seed_voice().await;
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
        // Live-Anzeige des Satzes, der gerade zu hören ist.
        let now_manager = Arc::clone(self);
        let now_sentences: Vec<String> = sentences.iter().map(|(text, _)| text.clone()).collect();
        let on_playing: Arc<dyn Fn(usize) + Send + Sync> = Arc::new(move |idx| {
            let _ = now_manager.app.emit(
                "tts-current-sentence",
                serde_json::json!({
                    "context": "speak",
                    "index": idx as u32,
                    "text": now_sentences.get(idx).cloned().unwrap_or_default(),
                }),
            );
        });
        self.core
            .speak_sentence_run(sentences, start, Some(on_playing), Some(on_played))
            .await
    }

    /// Server sicherstellen — mit einem zweiten Anlauf, wenn der erste an
    /// einem zerstoerten Compile-Cache gescheitert ist.
    ///
    /// Ein Systemabsturz waehrend des Kompilierens hinterlaesst im Cache von
    /// TorchInductor Dateien, die nur noch aus Nullbytes bestehen. Der Server
    /// stirbt daran beim Aufwaermen, und zwar bei JEDEM weiteren Start —
    /// heilen tut sich das nie von selbst. Bis v0.8.8 half nur, die Dateien
    /// von Hand zu suchen und zu loeschen; das ist keine Zumutung, die man
    /// einem Nutzer stellen darf, und die Bedingung dafuer (Nullbytes bei
    /// korrekter Laenge) ist maschinell pruefbar.
    ///
    /// Deshalb: EIN Versuch, bei Verdacht Reparatur, dann EIN zweiter
    /// Versuch. Nicht mehr — schlaegt auch der fehl, liegt es an etwas
    /// anderem, und eine Schleife machte es nur langsamer, nicht besser.
    pub async fn ensure_server(&self) -> Result<(), String> {
        let first = self.try_start_server().await;
        let Err(error) = first else {
            return Ok(());
        };
        let log = self
            .startup_log_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        if self.core.stop_requested.load(Ordering::Acquire) {
            // Der Nutzer hat den Start abgebrochen. Kein zweiter Anlauf.
            return Err(error);
        }
        if !compile_cache::looks_like_broken_compile_cache(&log) {
            return Err(error);
        }
        let Some(dir) = compile_cache::cache_dir() else {
            return Err(error);
        };
        let removed = match compile_cache::repair(&dir) {
            Ok(removed) => removed,
            Err(e) => {
                log::warn!("compile cache repair refused: {e}");
                return Err(error);
            }
        };
        if removed.is_empty() {
            // Der Verdacht stimmte, aber es gibt nichts zu loeschen — ein
            // zweiter Anlauf brauchte nur Zeit und endete gleich.
            return Err(error);
        }
        log::warn!(
            "{} zerstoerte Datei(en) im Compile-Cache entfernt, zweiter Startversuch",
            removed.len()
        );
        self.core.set_phase(
            TtsPhase::Starting,
            Some(format!(
                "Beschaedigter Compile-Cache bereinigt ({} Datei(en)) — starte erneut",
                removed.len()
            )),
        );
        self.try_start_server().await
    }

    /// Ein Startversuch: adoptieren, sonst spawnen und Health pollen.
    async fn try_start_server(&self) -> Result<(), String> {
        // Die Sperre wird VOR allem anderen atomar beansprucht und beim
        // Verlassen der Funktion wieder freigegeben. Vorher wurde die Phase
        // geprueft und erst nach dem Spawn gesetzt — dazwischen lagen eine
        // Gesundheitsabfrage und ein Prozessstart. Zwei Ausloeser in diesem
        // Fenster starteten beide einen Server. Der zweite belegte weitere
        // 17 GB VRAM und gehoerte niemandem; die App konnte ihn nicht mehr
        // beenden, weil sie ihn nicht als ihren kannte.
        let _claim = match self.core.start_claim.compare_exchange(
            false,
            true,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => StartClaim(&self.core.start_claim),
            Err(_) => return Err("Der Server startet bereits — bitte warten.".to_string()),
        };
        self.core.stop_requested.store(false, Ordering::Release);

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

        // Ausgabe des Kindprozesses in eine Datei statt ins Nichts. Ohne das
        // ist ein Startfehler nicht diagnostizierbar: der Nutzer sieht eine
        // Nummer, die Erklaerung steht in einem Traceback, den niemand liest.
        let startup_log = self.startup_log_path();
        let log_handles = startup_log.as_ref().and_then(|path| {
            let file = std::fs::File::create(path).ok()?;
            let clone = file.try_clone().ok()?;
            Some((file, clone))
        });

        let mut cmd = std::process::Command::new(&python);
        cmd.args([
            "tools/api_server.py",
            "--listen",
            &format!("127.0.0.1:{port}"),
        ])
        .current_dir(&fish_dir)
        .env("HF_HUB_DISABLE_TELEMETRY", "1");
        match log_handles {
            Some((out, err)) => {
                cmd.stdout(std::process::Stdio::from(out))
                    .stderr(std::process::Stdio::from(err));
            }
            None => {
                cmd.stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
            }
        }
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
            // Ein Abbruch waehrend des Startens muss sofort wirken: genau
            // dann will man ihn, weil der Start gerade den Speicher fuellt.
            if self.core.stop_requested.load(Ordering::Acquire) {
                self.kill_owned_child();
                self.core.owns_server.store(false, Ordering::Release);
                self.core.set_phase(TtsPhase::Stopped, None);
                return Err("Start abgebrochen".to_string());
            }
            if self.core.health_ok(port).await {
                self.core.set_phase(TtsPhase::Ready, None);
                *self.core.last_used.lock().unwrap() = Instant::now();
                log::info!("fish-speech ready after {} s", started.elapsed().as_secs());
                return Ok(());
            }
            // Früher Kindprozess-Tod (falscher Pfad, kaputtes venv) → klarer Fehler.
            if let Some(child) = self.child.lock().unwrap().as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    // Die Ursache steht im Protokoll des Kindprozesses; ohne
                    // sie ist "exit code 3" fuer niemanden verwertbar.
                    let detail = startup_log
                        .as_ref()
                        .and_then(|path| std::fs::read_to_string(path).ok())
                        .as_deref()
                        .and_then(startup_error_summary)
                        .map(|line| format!(" — {line}"))
                        .unwrap_or_default();
                    let where_ = startup_log
                        .as_ref()
                        .map(|p| format!(" (Protokoll: {})", p.display()))
                        .unwrap_or_default();
                    let msg =
                        format!("fish-speech exited during startup ({status}){detail}{where_}");
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
        rm.try_start_recording(
            VOICECHANGE_BINDING,
            crate::audio_toolkit::VadPolicy::Offline,
        )
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
            // speak_text sichert Server (bzw. Cache-Offline-Pfad) selbst.
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

    /// Derselbe Satz für jede Stimme — nur so vergleicht man Stimmen und nicht
    /// zwei verschiedene Aufnahmen. Bewusst kurz und vollständig: Klangfarbe,
    /// Tempo und Satzmelodie hört man an einem Satz, nicht an einem Wort.
    pub const DEMO_TEXT: &'static str = "Guten Tag. So klingt diese Stimme:         ein kurzer Satz, damit Sie Klangfarbe, Tempo und Betonung vergleichen können.";

    /// Wohin der Serverprozess seine Ausgabe schreibt. Eine Datei, bei jedem
    /// Start ueberschrieben: interessant ist immer der letzte Versuch.
    fn startup_log_path(&self) -> Option<std::path::PathBuf> {
        use tauri::Manager;
        let base = crate::portable::data_dir()
            .cloned()
            .or_else(|| self.app.path().app_local_data_dir().ok())?;
        std::fs::create_dir_all(&base).ok()?;
        Some(base.join("fish-speech-start.log"))
    }

    /// Wo die Hörproben liegen. Eigenes Verzeichnis neben `tts_cache`, damit
    /// `prune_disk_cache` sie nicht wegräumt — der Cache ist nach Größe
    /// begrenzt, die Hörproben sind wenige Dateien, die bleiben sollen.
    fn demo_dir(&self) -> Option<std::path::PathBuf> {
        use tauri::Manager;
        let base = crate::portable::data_dir()
            .cloned()
            .or_else(|| self.app.path().app_local_data_dir().ok())?;
        let dir = base.join("voice_demos");
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    /// Der Standardstimme eine echte Referenz verschaffen — einmal je Seed.
    ///
    /// Ohne Referenz wuerfelt Fish Speech die Sprecheridentitaet gemeinsam mit
    /// dem Inhalt aus. Der Seed geht zwar bei jeder Anfrage mit, aber jeder
    /// Satz ist eine eigene Anfrage mit eigenem Text — und damit klingt jede
    /// Zeile nach einer anderen Person. Fuer eine Vorlesestimme ist das
    /// unbrauchbar; genau das war die Beobachtung am 20.08.2026.
    ///
    /// Der Ausweg: den Demosatz EINMAL mit diesem Seed erzeugen und das
    /// Ergebnis als Referenz ablegen. Ab da spricht die Standardstimme so
    /// stabil wie eine geklonte — und weil Text und Seed feststehen, ergibt
    /// derselbe Seed auch nach einer Neuinstallation dieselbe Stimme.
    ///
    /// Die Referenz liegt unter `__seed_<seed>` und ist damit aus der
    /// Stimmenliste ausgenommen (siehe `voices::INTERNAL_PREFIX`).
    ///
    /// Rueckgabe: die Referenz-Kennung, oder `None`, wenn sie sich nicht
    /// anlegen liess — dann laeuft alles wie bisher weiter, nur eben
    /// wechselhaft. Ein Vorlesen daran scheitern zu lassen waere schlimmer.
    async fn ensure_seed_reference(&self, port: u16, seed: i64) -> Option<String> {
        let id = voices::seed_voice_id(seed);
        let fish_dir = self.fish_dir();
        if voices::voice_is_complete(&fish_dir, &id) {
            return Some(id);
        }
        let body = protocol::tts_request_body_in_format(Self::DEMO_TEXT, seed, None, "wav");
        let resp = self
            .core
            .http
            .post(format!("{}/v1/tts", protocol::base_url(port)))
            .json(&body)
            .timeout(TTS_TIMEOUT)
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            log::warn!("seed reference: server answered {}", resp.status());
            return None;
        }
        let audio = resp.bytes().await.ok()?.to_vec();
        if !protocol::looks_like_wav(&audio) {
            log::warn!("seed reference: answer was not a WAV");
            return None;
        }
        // Auf denselben Pegel wie jede andere Referenz: die Standardstimme
        // soll sich in einen Dialog einreihen koennen, ohne herauszustechen.
        let audio = normalize_wav_bytes(&audio).unwrap_or(audio);
        let dir = voices::voice_dir(&fish_dir, &id);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::warn!("seed reference: could not create {}: {e}", dir.display());
            return None;
        }
        if let Err(e) = std::fs::write(dir.join("sample.wav"), &audio) {
            log::warn!("seed reference: could not write sample: {e}");
            return None;
        }
        if let Err(e) = std::fs::write(dir.join("sample.lab"), Self::DEMO_TEXT.as_bytes()) {
            log::warn!("seed reference: could not write transcript: {e}");
            return None;
        }
        log::info!("Standardstimme fuer Seed {seed} als Referenz {id} festgehalten");
        Some(id)
    }

    /// `core.voice` auf die Seed-Referenz setzen, wenn keine Stimme gewaehlt
    /// ist. Vor jedem Sprechlauf aufgerufen, nachdem der Server steht.
    async fn bind_seed_voice(&self) {
        if self.core.voice.lock().unwrap().is_some() {
            return;
        }
        let port = *self.core.port.lock().unwrap();
        let seed = *self.core.seed.lock().unwrap();
        if let Some(id) = self.ensure_seed_reference(port, seed).await {
            *self.core.voice.lock().unwrap() = Some(id);
        }
    }

    /// Hörprobe einer Stimme: `DEMO_TEXT`, mit genau dieser Stimme erzeugt und
    /// als WAV zwischengespeichert.
    ///
    /// Erzeugt wird nur beim ersten Mal — und erneut, wenn die Referenzaufnahme
    /// der Stimme jünger ist als die Hörprobe: wer eine Stimme unter demselben
    /// Namen neu aufnimmt, soll nicht die alte hören.
    ///
    /// Anders als `synthesize_to_file` hängt das NICHT an der aktiven Stimme —
    /// man will ja gerade die anderen hören, ohne umzuschalten.
    pub async fn synthesize_voice_demo(
        &self,
        voice_id: &str,
    ) -> Result<std::path::PathBuf, String> {
        let dir = self
            .demo_dir()
            .ok_or_else(|| "Kein Ablageort für Hörproben".to_string())?;
        // Vor der Ablagefrage, weil der Dateiname der Standardstimme ihren
        // Seed trägt: ein anderer Seed ist eine andere Stimme.
        self.refresh_from_settings();
        let seed = *self.core.seed.lock().unwrap();
        // Leere Kennung = Standardstimme (Seed), die Stimme ohne Referenz.
        // Sie ist so anhörbar wie jede andere — man wählt sie ja gegen die
        // anderen aus, und das geht nur, wenn man sie auch hören kann.
        let reference = (!voice_id.trim().is_empty()).then_some(voice_id);
        let out = match reference {
            Some(id) => dir.join(format!("{id}.wav")),
            None => dir.join(format!("seed-{seed}.wav")),
        };

        let reference_mtime = voices::voice_sample(&self.fish_dir(), voice_id)
            .and_then(|(wav, _)| std::fs::metadata(wav).ok())
            .and_then(|meta| meta.modified().ok());
        let demo_mtime = std::fs::metadata(&out).ok().and_then(|m| m.modified().ok());
        if let (Some(demo), Some(reference)) = (demo_mtime, reference_mtime) {
            if demo >= reference {
                return Ok(out);
            }
        } else if demo_mtime.is_some() && reference_mtime.is_none() {
            return Ok(out);
        }

        self.ensure_server().await?;
        let port = *self.core.port.lock().unwrap();
        // Die Standardstimme wird ueber ihre Seed-Referenz angehoert, nicht
        // referenzlos: sonst waere die Hoerprobe eine andere Person als die,
        // die danach vorliest.
        let seed_reference = match reference {
            Some(_) => None,
            None => self.ensure_seed_reference(port, seed).await,
        };
        let reference = reference.or(seed_reference.as_deref());
        // Immer WAV, unabhängig vom Export-Format des Nutzers: die Hörprobe ist
        // ein interner Cache mit vorhersagbarem Namen, kein Liefergegenstand.
        let body = protocol::tts_request_body_in_format(Self::DEMO_TEXT, seed, reference, "wav");
        let resp = self
            .core
            .http
            .post(format!("{}/v1/tts", protocol::base_url(port)))
            .json(&body)
            .timeout(TTS_TIMEOUT)
            .send()
            .await
            .map_err(|e| format!("TTS request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("TTS server answered {}", resp.status()));
        }
        let audio = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
        if !protocol::looks_like_audio(&audio, "wav") {
            return Err("TTS response is not valid wav audio".to_string());
        }
        // Die Hörprobe ist eine eigene Datei, die ein <audio>-Element abspielt
        // — sie geht den Wiedergabe-Pfad NICHT und bekäme dessen Ausgleich
        // sonst nie. Ausgerechnet die Vorschau, an der man Stimmen
        // vergleicht, wäre damit die einzige ungeregelte Stelle.
        let audio = normalize_wav_bytes(&audio).unwrap_or(audio);
        std::fs::write(&out, &audio)
            .map_err(|e| format!("could not write {}: {e}", out.display()))?;
        *self.core.last_used.lock().unwrap() = Instant::now();
        Ok(out)
    }

    /// Den ganzen Vorlesetext — Dialog eingeschlossen — in EINE WAV-Datei
    /// schreiben, statt ihn nur zu hören.
    ///
    /// Geht bewusst durch dieselbe Zerlegung wie das Abspielen
    /// (`utterances`), damit die Datei Satz für Satz klingt wie das, was man
    /// vorher gehört hat. Zusammengefügt wird mit `hound`: die Teile kommen
    /// als eigenständige WAVs vom Server, und ein simples Aneinanderhängen der
    /// Bytes ergäbe eine Datei mit Kopfdaten mitten im Ton.
    pub async fn speak_to_file(
        self: &Arc<Self>,
        raw: &str,
        out_path: &str,
    ) -> Result<usize, String> {
        let max_chars = *self.core.max_chars.lock().unwrap();
        let prepared =
            protocol::prepare_text(raw, max_chars).ok_or_else(|| "empty text".to_string())?;
        let utterances = self.utterances(&prepared.text);
        if utterances.is_empty() {
            return Err("empty text".to_string());
        }
        self.refresh_from_settings();
        self.ensure_server().await?;
        self.bind_seed_voice().await;
        let port = *self.core.port.lock().unwrap();
        let seed = *self.core.seed.lock().unwrap();

        // Eigenes Abbruch-Flag je Lauf; ein neuer Export storniert den alten.
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut slot = self.export_cancel.lock().unwrap();
            slot.store(true, Ordering::Release);
            *slot = cancel.clone();
        }
        let total = utterances.len() as u32;
        self.emit_export_progress(0, total, false);

        let mut writer: Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>> = None;
        let mut written = 0usize;
        for (index, (sentence, voice)) in utterances.iter().enumerate() {
            if cancel.load(Ordering::Acquire) {
                // Halbe Datei ist schlimmer als keine: sie sieht fertig aus.
                drop(writer);
                let _ = std::fs::remove_file(out_path);
                self.emit_export_progress(index as u32, total, true);
                return Err("abgebrochen".to_string());
            }
            let Some(part) = protocol::prepare_text(sentence, max_chars) else {
                continue;
            };
            let bytes = self
                .core
                .fetch_wav(port, seed, &part.text, voice.as_deref())
                .await?;
            // Derselbe Ausgleich wie beim Hören: eine exportierte Datei mit
            // wechselnden Stimmen soll nicht lauter und leiser werden.
            let gain = self.core.playback_gain(voice.as_deref(), &bytes);
            let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes))
                .map_err(|e| format!("Teilstueck nicht lesbar: {e}"))?;
            let spec = reader.spec();
            if writer.is_none() {
                writer = Some(
                    hound::WavWriter::create(out_path, spec)
                        .map_err(|e| format!("could not write {out_path}: {e}"))?,
                );
            }
            let sink = writer.as_mut().expect("writer exists");
            for sample in reader.samples::<i16>() {
                let sample = sample.map_err(|e| format!("Teilstueck beschaedigt: {e}"))?;
                let sample = if gain == 1.0 {
                    sample
                } else {
                    (sample as f32 * gain).clamp(i16::MIN as f32, i16::MAX as f32) as i16
                };
                sink.write_sample(sample)
                    .map_err(|e| format!("could not write {out_path}: {e}"))?;
                written += 1;
            }
            self.emit_export_progress(index as u32 + 1, total, false);
        }
        writer
            .ok_or_else(|| "nichts zu schreiben".to_string())?
            .finalize()
            .map_err(|e| format!("could not finish {out_path}: {e}"))?;
        *self.core.last_used.lock().unwrap() = Instant::now();
        self.emit_export_progress(total, total, false);
        Ok(written)
    }

    /// Zugriff auf den AppHandle fuer Ereignisse aus Hintergrundlaeufen.
    pub fn app_handle(&self) -> tauri::AppHandle {
        self.app.clone()
    }

    /// Um `delta` Saetze springen und von dort weiterlesen.
    ///
    /// Das Vorlesen ist satzweise aufgebaut, nicht als durchgehender Strom —
    /// ein Sprung um 15 Sekunden gaebe es hier gar nicht. Der Satz ist die
    /// Einheit, in der man sich in vorgelesenem Text bewegt.
    pub async fn speak_seek(self: &Arc<Self>, delta: i32) -> Result<usize, String> {
        use tauri::Emitter;
        let (sentences, target) = {
            let mut guard = self.speak_session.lock().unwrap();
            let session = guard.as_mut().ok_or("nichts zum Springen")?;
            let len = session.sentences.len() as i32;
            let next = (session.position as i32 + delta).clamp(0, (len - 1).max(0));
            session.position = next as usize;
            (session.sentences.clone(), next as usize)
        };
        // Die neue Position melden. Ohne das erfuhr die Oberfläche vom Sprung
        // nichts: ihr "Fortsetzen möglich" hängt am Fortschritt, und der kam
        // bisher nur, wenn ein Satz VOLLSTÄNDIG gespielt wurde. Wer sprang und
        // dann pausierte, bekam beim nächsten Druck auf Play einen Neustart
        // von vorn statt der Fortsetzung an der Sprungmarke.
        let _ = self.app.emit(
            "tts-speak-progress",
            serde_json::json!({ "position": target as u32, "total": sentences.len() as u32 }),
        );
        self.run_speak_session(sentences, target).await
    }

    /// Die aktive Stimme hat sich geändert — sofort übernehmen.
    ///
    /// Während des Vorlesens genügt es NICHT, die Einstellung zu spiegeln: die
    /// Satz-Pipeline holt den nächsten Satz bereits, während der aktuelle noch
    /// spielt, ein Wechsel wäre also erst zwei Sätze später zu hören. Läuft
    /// gerade eine Wiedergabe, beginnt sie deshalb beim aktuellen Satz neu —
    /// der wird in der neuen Stimme wiederholt, und ab da gilt sie.
    ///
    /// Sätze mit ausdrücklicher Stimme (Dialogzeilen wie `olga:`) bleiben
    /// unberührt: dort hat der Text die Stimme bestimmt, nicht die Einstellung.
    pub fn apply_voice_change(self: &Arc<Self>) {
        self.refresh_from_settings();
        if self.core.phase() != TtsPhase::Speaking {
            return;
        }
        let Some((sentences, position)) = ({
            let guard = self.speak_session.lock().unwrap();
            guard
                .as_ref()
                .map(|session| (session.sentences.clone(), session.position))
        }) else {
            return;
        };
        if position >= sentences.len() {
            return;
        }
        let manager = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            if let Err(e) = manager.run_speak_session(sentences, position).await {
                log::warn!("voice change restart failed: {e}");
            }
        });
    }

    /// Laufenden Datei-Export abbrechen.
    pub fn cancel_export(&self) {
        self.export_cancel
            .lock()
            .unwrap()
            .store(true, Ordering::Release);
    }

    fn emit_export_progress(&self, position: u32, total: u32, cancelled: bool) {
        use tauri::Emitter;
        let _ = self.app.emit(
            "tts-export-progress",
            serde_json::json!({
                "position": position,
                "total": total,
                "cancelled": cancelled,
            }),
        );
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
            // Bereits synthetisierte Passagen spielen offline — der Server
            // startet nur, wenn noch Sätze fehlen.
            if let Err(e) = manager
                .ensure_server_for(&sentences[(start as usize).min(sentences.len())..])
                .await
            {
                log::error!("reading: server start failed: {e}");
                return;
            }
            // Live-Anzeige des gelesenen Satzes.
            let now_manager = Arc::clone(&manager);
            let now_sentences = sentences.clone();
            let on_playing: Arc<dyn Fn(usize) + Send + Sync> = Arc::new(move |idx| {
                use tauri::Emitter;
                let _ = now_manager.app.emit(
                    "tts-current-sentence",
                    serde_json::json!({
                        "context": "reading",
                        "index": idx as u32,
                        "text": now_sentences.get(idx).cloned().unwrap_or_default(),
                    }),
                );
            });
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
                .speak_sentence_run(
                    single_voice(sentences),
                    start as usize,
                    Some(on_playing),
                    Some(on_played),
                )
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
        let wav = self
            .core
            .fetch_wav(port, seed, &prepared.text, None)
            .await?;
        let tts_ms = tts_start.elapsed().as_millis() as u64;
        Ok((wav, server_start_ms, tts_ms))
    }

    /// Beendet AUSSCHLIESSLICH einen selbst gestarteten Serverprozess.
    /// Den Fish-Speech-Server beenden — auch einen, den die App nicht selbst
    /// gestartet hat.
    ///
    /// Frueher hat sie fremde Prozesse grundsaetzlich in Ruhe gelassen. Das ist
    /// als Regel vertretbar, in der Praxis aber unbrauchbar: der Server belegt
    /// rund 17 GB VRAM, und wer ihn einmal von Hand gestartet hat, musste zum
    /// Taskmanager greifen, um seine Grafikkarte zurueckzubekommen.
    ///
    /// Erkannt wird er ueber zwei Merkmale zugleich — er lauscht auf dem
    /// eingestellten TTS-Port UND antwortet auf `/v1/health`. Ein fremdes
    /// Programm, das zufaellig denselben Port belegt, wird damit nicht
    /// getroffen; die Gesundheitsantwort ist der Ausweis.
    pub async fn stop_server_any(&self) -> Result<(), String> {
        self.core.cancel_core();

        if self.core.owns_server() {
            self.kill_owned_child();
            self.core.owns_server.store(false, Ordering::Release);
            self.core.set_phase(TtsPhase::Stopped, None);
            return Ok(());
        }

        let port = *self.core.port.lock().unwrap();
        if !self.core.health_ok(port).await {
            // Nichts da, was zu beenden waere — Zustand nur aufraeumen.
            self.core.set_phase(TtsPhase::Stopped, None);
            return Ok(());
        }
        let pid = listening_pid(port)
            .ok_or_else(|| format!("Kein Prozess gefunden, der auf Port {port} lauscht"))?;
        kill_pid(pid)?;
        log::info!("fish-speech (fremd gestartet, PID {pid}) auf Port {port} beendet");
        self.core.set_phase(TtsPhase::Stopped, None);
        Ok(())
    }

    /// Hart beenden: alles abschießen, was auf dem TTS-Port lauscht — ohne
    /// vorher zu fragen, ob es gesund ist.
    ///
    /// `stop_server_any` prüft bei einem fremd gestarteten Server erst die
    /// Gesundheit und meldet „nichts zu beenden", wenn keine Antwort kommt.
    /// Genau dann braucht man diesen Knopf aber: ein Server, der beim Starten
    /// hängt oder nicht mehr antwortet, hält trotzdem rund 17 GB VRAM fest,
    /// und der einzige Ausweg war bisher der Taskmanager.
    ///
    /// Rückgabe: was tatsächlich passiert ist, für die Rückmeldung an den
    /// Nutzer — „nichts gefunden" ist ein Ergebnis, kein Fehler.
    pub fn kill_server_hard(&self) -> Result<String, String> {
        // Zuerst: ein laufender Startversuch soll nicht weiterlaufen und
        // hinterher auch nicht wiederholt werden.
        self.core.stop_requested.store(true, Ordering::Release);
        self.core.cancel_core();
        let owned = self.core.owns_server();
        if owned {
            self.kill_owned_child();
            self.core.owns_server.store(false, Ordering::Release);
        }
        let port = *self.core.port.lock().unwrap();
        // Auch nach dem eigenen Kind noch auf dem Port nachsehen: ein
        // Serverstart, der zweimal lief, hinterlässt einen Prozess, der uns
        // nicht mehr gehört (beobachtet am 20.08.2026, drei Startzeilen).
        let killed_foreign = match listening_pid(port) {
            Some(pid) => match kill_pid(pid) {
                Ok(()) => {
                    log::info!("fish-speech auf Port {port} (PID {pid}) hart beendet");
                    true
                }
                Err(e) => {
                    self.core.set_phase(TtsPhase::Stopped, None);
                    return Err(e);
                }
            },
            None => false,
        };
        self.core.set_phase(TtsPhase::Stopped, None);
        Ok(match (owned, killed_foreign) {
            (_, true) => format!("Prozess auf Port {port} beendet"),
            (true, false) => "Eigener Serverprozess beendet".to_string(),
            (false, false) => format!("Kein Prozess auf Port {port} gefunden"),
        })
    }

    /// Nur den selbst gestarteten Prozess beenden (Idle-Watchdog, Herunterfahren).
    pub fn stop_server(&self) {
        self.core.stop_requested.store(true, Ordering::Release);
        self.core.cancel_core();
        self.kill_owned_child();
        // Auch das, was uns nicht gehoert: beim Beenden der Anwendung darf
        // kein Serverprozess ueberleben, egal wer ihn gestartet hat. Ein
        // verwaister Prozess haelt 17 GB VRAM, die niemand mehr freigibt —
        // die App kann ihn danach nicht einmal mehr finden.
        let port = *self.core.port.lock().unwrap();
        if let Some(pid) = listening_pid(port) {
            if let Err(e) = kill_pid(pid) {
                log::warn!("Could not stop server on port {port}: {e}");
            }
        }
        self.core.owns_server.store(false, Ordering::Release);
        self.core.set_phase(TtsPhase::Stopped, None);
    }

    /// Den eigenen Serverprozess beenden — samt seiner Kinder.
    ///
    /// `Child::kill` beendet unter Windows NUR den direkten Prozess. Der
    /// Fish-API-Server startet aber einen Arbeitsprozess, und der haelt das
    /// Modell: gemessen am 21.08.2026 7,92 GB, die nach einem vermeintlich
    /// erfolgreichen Stopp weiterliefen. Deshalb erst den Baum ueber
    /// `taskkill /T`, danach der uebliche Weg als Rueckfallebene.
    fn kill_owned_child(&self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            #[cfg(windows)]
            if let Err(e) = kill_pid(child.id()) {
                log::warn!("Could not kill fish-speech process tree: {e}");
            }
            if let Err(e) = child.kill() {
                log::debug!("fish-speech child already gone: {e}");
            }
            let _ = child.wait();
        }
    }
}

/// PID des Prozesses, der auf `127.0.0.1:port` lauscht.
///
/// Ueber `netstat -ano` statt einer Crate: das Werkzeug gehoert zu Windows, die
/// Ausgabe ist seit Jahrzehnten stabil, und der Alternativweg (IP Helper API)
/// waere fuer eine einzige Abfrage viel unsafe-Code.
/// PID des Prozesses, der auf `port` lauscht.
fn listening_pid(port: u16) -> Option<u32> {
    let output = std::process::Command::new("netstat")
        .args(["-ano", "-p", "TCP"])
        .output()
        .ok()?;
    parse_listening_pid(&String::from_utf8_lossy(&output.stdout), port)
}

/// Die Zeile eines lauschenden Sockets aus einer netstat-Ausgabe heraussuchen.
///
/// Erkannt wird an der STRUKTUR, nicht am Statuswort: `netstat` uebersetzt es
/// (deutsch "ABHOEREN", englisch "LISTENING"). Der Vergleich mit "LISTENING"
/// lief auf einem deutschen Windows deshalb immer ins Leere — beide
/// Stopp-Wege der App taten schlicht nichts, und der Serverprozess hielt
/// seine 17 GB VRAM weiter fest (beobachtet 20.08.2026).
///
/// Ein lauschender Socket hat keine Gegenstelle; seine Remoteadresse ist
/// `0.0.0.0:0` bzw. `[::]:0`. Eine ausgehende Verbindung von demselben Port
/// hat dort eine echte Adresse und wird dadurch ausgeschlossen. Das gilt in
/// jeder Sprache, weil dort nur Zahlen stehen.
fn parse_listening_pid(netstat_output: &str, port: u16) -> Option<u32> {
    let wanted = format!(":{port}");
    for line in netstat_output.lines() {
        let mut fields = line.split_whitespace();
        let (Some(proto), Some(local), Some(remote), Some(_state), Some(pid)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            continue;
        };
        if !proto.eq_ignore_ascii_case("TCP") {
            continue;
        }
        // Nur die Loopback-Adresse: der Server der App laeuft auf 127.0.0.1,
        // und ein fremder Dienst auf 0.0.0.0 desselben Ports geht uns nichts an.
        let local_matches = local.ends_with(&wanted) && local.contains("127.0.0.1");
        let is_listening = remote.ends_with(":0");
        if local_matches && is_listening {
            return pid.parse().ok();
        }
    }
    None
}

fn kill_pid(pid: u32) -> Result<(), String> {
    let output = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output()
        .map_err(|e| format!("taskkill nicht ausfuehrbar: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "Prozess {pid} liess sich nicht beenden: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Der Statustext ist uebersetzt — die Erkennung darf nicht daran haengen.
    /// Beide Ausgaben stammen von echten Systemen (de-DE und en-US).
    #[test]
    fn der_lauschende_prozess_wird_in_jeder_sprache_gefunden() {
        let deutsch = concat!(
            "Aktive Verbindungen\r\n\r\n",
            "  Proto  Lokale Adresse         Remoteadresse          Status           PID\r\n",
            "  TCP    0.0.0.0:135            0.0.0.0:0              ABHÖREN         2284\r\n",
            "  TCP    127.0.0.1:8080         0.0.0.0:0              ABHÖREN         87820\r\n"
        );
        let englisch = concat!(
            "Active Connections\r\n\r\n",
            "  Proto  Local Address          Foreign Address        State           PID\r\n",
            "  TCP    127.0.0.1:8080         0.0.0.0:0              LISTENING       4711\r\n"
        );
        assert_eq!(parse_listening_pid(deutsch, 8080), Some(87820));
        assert_eq!(parse_listening_pid(englisch, 8080), Some(4711));
    }

    /// Eine ausgehende Verbindung VON diesem Port ist kein Server.
    #[test]
    fn eine_bestehende_verbindung_wird_nicht_fuer_den_server_gehalten() {
        let text = "  TCP    127.0.0.1:8080         127.0.0.1:53318        HERGESTELLT     999\r\n";
        assert_eq!(parse_listening_pid(text, 8080), None);
    }

    /// Ein anderer Port und ein Dienst auf allen Adressen gehen uns nichts an.
    #[test]
    fn fremde_ports_und_fremde_adressen_werden_uebergangen() {
        let text = concat!(
            "  TCP    127.0.0.1:8081         0.0.0.0:0              ABHÖREN         111\r\n",
            "  TCP    0.0.0.0:8080           0.0.0.0:0              ABHÖREN         222\r\n"
        );
        assert_eq!(parse_listening_pid(text, 8080), None);
    }

    /// Der echte Fall vom 21.08.2026: Startprotokoll eines Servers, den ein
    /// zerstoerter Compile-Cache umgebracht hat. Die Meldung muss die
    /// Ursachenzeile tragen, nicht die Rahmenzeilen des Tracebacks.
    #[test]
    fn die_ursachenzeile_wird_aus_dem_startprotokoll_gezogen() {
        let log = concat!(
            "Traceback (most recent call last):\r\n",
            r#"  File "C:\AI\fish-speech\tools\api_server.py", line 89, in initialize_app"#,
            "\r\n",
            "    app.state.model_manager = ModelManager(\r\n",
            "torch._dynamo.exc.BackendCompilerFailed: backend='inductor' raised:\r\n",
            "JSONDecodeError: Expecting value: line 1 column 1 (char 0)\r\n",
            "\r\n",
            "ERROR:    Application startup failed. Exiting.\r\n"
        );
        let summary = startup_error_summary(log).expect("Zusammenfassung");
        assert!(summary.contains("Application startup failed"), "{summary}");
        assert!(
            !summary.starts_with("File \""),
            "Rahmenzeile gewaehlt: {summary}"
        );
    }

    /// Ohne Fehlerwort bleibt die letzte nicht leere Zeile — irgendetwas ist
    /// immer besser als eine nackte Nummer.
    #[test]
    fn ohne_fehlerwort_bleibt_die_letzte_zeile() {
        let log = "lade Modell\r\n\r\nfertig\r\n\r\n";
        assert_eq!(startup_error_summary(log).as_deref(), Some("fertig"));
    }

    #[test]
    fn ein_leeres_protokoll_ergibt_keine_zusammenfassung() {
        assert_eq!(startup_error_summary(""), None);
        assert_eq!(startup_error_summary("   \r\n\r\n  "), None);
    }

    /// Eine Fehlermeldung ist kein Protokollfenster: sehr lange Zeilen werden
    /// gekappt, damit sie im Fehlerband der Oberflaeche noch lesbar sind.
    #[test]
    fn sehr_lange_zeilen_werden_gekappt() {
        let log = format!("Error: {}", "x".repeat(500));
        let summary = startup_error_summary(&log).expect("Zusammenfassung");
        assert!(
            summary.chars().count() <= 301,
            "{} Zeichen",
            summary.chars().count()
        );
        assert!(summary.ends_with('…'));
    }

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
        assert!(
            !core.owns_server(),
            "extern erkannt → kein Besitz, kein Kill"
        );
    }

    #[tokio::test]
    async fn ein_selbst_gestarteter_server_bleibt_nach_der_gesundheitspruefung_eigener() {
        let calls = Arc::new(AtomicUsize::new(0));
        let port = spawn_mock(calls, Arc::new(std::sync::Mutex::new(Vec::new()))).await;
        let core = TtsCore::for_test(port);
        // So sieht es aus, nachdem die App selbst gespawnt hat.
        core.owns_server.store(true, Ordering::Release);

        core.ensure_server_core().await.unwrap();

        assert!(
            core.owns_server(),
            "die Gesundheitspruefung hat den eigenen Server enteignet — danach              war 'Server stoppen' ausgegraut und der Prozess blieb mit seinem              VRAM stehen"
        );
    }

    #[tokio::test]
    async fn eine_laufende_wiedergabe_wird_von_der_gesundheitspruefung_nicht_beendet() {
        let calls = Arc::new(AtomicUsize::new(0));
        let port = spawn_mock(calls, Arc::new(std::sync::Mutex::new(Vec::new()))).await;
        let core = TtsCore::for_test(port);
        core.set_phase(TtsPhase::Speaking, None);

        core.ensure_server_core().await.unwrap();

        assert_eq!(
            core.phase(),
            TtsPhase::Speaking,
            "die Phase ist zugleich die Anzeige 'spricht gerade'; wird sie              mitten im Vorlesen auf 'Bereit' gesetzt, graut die Oberflaeche              ihren einzigen Stopp-Knopf aus"
        );
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
        assert_eq!(
            core.phase(),
            TtsPhase::Ready,
            "nach dem Sprechen wieder Ready"
        );
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
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "neuer Text → neuer Request"
        );
    }

    /// Bereits Vorgelesenes muss OHNE Server abspielbar sein: Der zweite
    /// Kern (leerer RAM-Cache, unerreichbarer Port) bedient sich vom
    /// Platten-Cache des ersten.
    #[tokio::test]
    async fn cached_audio_plays_without_any_server() {
        let calls = Arc::new(AtomicUsize::new(0));
        let bodies = Arc::new(std::sync::Mutex::new(Vec::new()));
        let port = spawn_mock(calls.clone(), bodies).await;
        let cache_dir = tempfile::tempdir().unwrap();

        let text = "Dieser Satz landet im persistenten Plattencache.";
        let online = TtsCore::for_test(port);
        *online.cache_dir.lock().unwrap() = Some(cache_dir.path().to_path_buf());
        online.ensure_server_core().await.unwrap();
        online.speak_core(text).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let offline = TtsCore::for_test(1); // Port 1: kein Server erreichbar
        *offline.cache_dir.lock().unwrap() = Some(cache_dir.path().to_path_buf());
        assert!(
            offline.has_cached(text),
            "Platten-Cache muss erkannt werden"
        );
        let played = offline.speak_core(text).await.unwrap();
        assert!(played > 1024, "Wiedergabe kam vollständig von der Platte");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "kein weiterer Server-Request"
        );
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
        assert!(
            flag.load(Ordering::Acquire),
            "cancel muss den laufenden Auftrag treffen"
        );
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
