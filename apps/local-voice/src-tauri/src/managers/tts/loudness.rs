//! Lautheitsmessung nach ITU-R BS.1770-4 (EBU R 128) — die Grundlage dafür,
//! dass alle Stimmen gleich laut klingen.
//!
//! Warum nicht einfach RMS über die Datei: RMS zählt Pausen mit. Eine
//! Aufnahme mit viel Stille misst dadurch leiser, als sie klingt, und wird
//! beim Normalisieren zu weit hochgezogen — genau der Effekt, der zwei
//! Sprecher unterschiedlich laut wirken lässt. BS.1770 filtert erst
//! gehörrichtig (K-Gewichtung) und misst dann blockweise mit zwei Toren:
//! absolut bei -70 LUFS (Stille fällt raus) und relativ 10 LU unter dem
//! Mittel (leise Passagen zwischen den Sätzen fallen raus). Gemessen wird
//! damit die Lautheit des *Gesprochenen*, nicht die der Datei.
//!
//! Reines Rechnen, keine Fremdbibliothek, kein I/O — dadurch prüfbar.

/// Zielpegel aller Stimmen: -20 LUFS.
///
/// Rundfunk normiert auf -23 LUFS; hier liegt der Wert bewusst höher, weil
/// die Wiedergabe eine einzelne Sprachspur ohne Musik ist und die
/// Nutzerlautstärke (`tts_volume`) obendrauf noch nach unten regelt.
pub const TARGET_LUFS: f32 = -20.0;

/// Aussteuerungsgrenze -1 dBFS. Deckelt die Verstärkung, damit eine leise
/// Aufnahme mit einem einzelnen lauten Einsatz nicht ins Clipping gerät.
pub const PEAK_CEILING: f32 = 0.891;

/// Höchste Anhebung: +24 dB.
///
/// Der Deckel schützt davor, aus einer fast stillen Aufnahme lautes Rauschen
/// zu machen. Er lag zuerst bei +12 dB — zu eng: gemessen am 20.08.2026 lagen
/// zwei von acht echten Referenzaufnahmen bei RMS -40 und -45 dBFS und
/// brauchten +18 bzw. +21 dB. Sie blieben dadurch 6 bis 9 dB zu leise, also
/// genau der Fehler, den das Pegeln beseitigen soll. Gegen Clipping schützt
/// ohnehin die Aussteuerungsgrenze, nicht dieser Wert.
const MAX_BOOST_DB: f32 = 24.0;

/// Blocklänge der Messung (400 ms) und Vorschub (100 ms = 75 % Überlappung).
const BLOCK_SECS: f64 = 0.4;
const OVERLAP: usize = 4;

/// Absolutes Tor: alles darunter ist Stille, nicht leise Sprache.
const ABSOLUTE_GATE_LUFS: f64 = -70.0;
/// Relatives Tor: 10 LU unter dem Mittel der nicht-stillen Blöcke.
const RELATIVE_GATE_LU: f64 = 10.0;

/// Offset aus BS.1770 — bringt die Skala auf LUFS.
const LUFS_OFFSET: f64 = -0.691;

/// Biquad in transponierter Direktform II.
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Erste Stufe der K-Gewichtung: Kugelkopf-Filter (High-Shelf, +4 dB).
///
/// Die Koeffizienten sind in BS.1770 für 48 kHz tabelliert; hier werden sie
/// aus demselben analogen Prototyp für die tatsächliche Abtastrate erzeugt,
/// damit 16-kHz-Referenzen und 44,1-kHz-Importe gleich gemessen werden.
fn shelving_filter(sample_rate: f64) -> Biquad {
    let f0 = 1681.974450955533;
    let gain_db = 3.999_843_853_973_347;
    let q = 0.7071752369554196;

    let k = (std::f64::consts::PI * f0 / sample_rate).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.499_666_774_154_641_6);
    let a0 = 1.0 + k / q + k * k;
    Biquad {
        b0: (vh + vb * k / q + k * k) / a0,
        b1: 2.0 * (k * k - vh) / a0,
        b2: (vh - vb * k / q + k * k) / a0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / q + k * k) / a0,
        z1: 0.0,
        z2: 0.0,
    }
}

/// Zweite Stufe: RLB-Hochpass, nimmt den Tiefbass aus der Messung.
fn highpass_filter(sample_rate: f64) -> Biquad {
    let f0 = 38.13547087602444;
    let q = 0.5003270373238773;
    let k = (std::f64::consts::PI * f0 / sample_rate).tan();
    let denom = 1.0 + k / q + k * k;
    Biquad {
        b0: 1.0,
        b1: -2.0,
        b2: 1.0,
        a1: 2.0 * (k * k - 1.0) / denom,
        a2: (1.0 - k / q + k * k) / denom,
        z1: 0.0,
        z2: 0.0,
    }
}

/// Integrierte Lautheit in LUFS eines Mono-Signals.
///
/// `None`, wenn nichts Messbares da ist: zu kurz für einen 400-ms-Block oder
/// durchgehend unter dem absoluten Tor (Stille). Aufrufer lassen solche
/// Signale unverändert, statt Rauschen hochzuziehen.
pub fn loudness_lufs(mono: &[f32], sample_rate: u32) -> Option<f32> {
    if sample_rate == 0 {
        return None;
    }
    let rate = sample_rate as f64;
    let block_len = (BLOCK_SECS * rate).round() as usize;
    if block_len == 0 || mono.len() < block_len {
        return None;
    }
    let step = block_len / OVERLAP;
    if step == 0 {
        return None;
    }

    // K-Gewichtung einmal über das ganze Signal; die Blöcke überlappen und
    // dürfen den Filterzustand nicht jeweils neu anfangen lassen.
    let mut shelf = shelving_filter(rate);
    let mut hp = highpass_filter(rate);
    let filtered: Vec<f64> = mono
        .iter()
        .map(|s| hp.process(shelf.process(*s as f64)))
        .collect();

    // Mittlere Leistung je Block (z_j in der Norm).
    let mut powers: Vec<f64> = Vec::new();
    let mut start = 0usize;
    while start + block_len <= filtered.len() {
        let sum: f64 = filtered[start..start + block_len]
            .iter()
            .map(|v| v * v)
            .sum();
        powers.push(sum / block_len as f64);
        start += step;
    }
    if powers.is_empty() {
        return None;
    }

    let loudness_of = |z: f64| LUFS_OFFSET + 10.0 * z.log10();

    // Tor 1 (absolut): Stille zählt nicht mit.
    let above_absolute: Vec<f64> = powers
        .iter()
        .copied()
        .filter(|z| *z > 0.0 && loudness_of(*z) > ABSOLUTE_GATE_LUFS)
        .collect();
    if above_absolute.is_empty() {
        return None;
    }

    // Tor 2 (relativ): Pausen zwischen den Sätzen zählen nicht mit.
    let mean_above = above_absolute.iter().sum::<f64>() / above_absolute.len() as f64;
    let relative_gate = loudness_of(mean_above) - RELATIVE_GATE_LU;
    let gated: Vec<f64> = above_absolute
        .into_iter()
        .filter(|z| loudness_of(*z) > relative_gate)
        .collect();
    if gated.is_empty() {
        return None;
    }

    let mean = gated.iter().sum::<f64>() / gated.len() as f64;
    Some(loudness_of(mean) as f32)
}

/// Spitzenwert (Betrag) eines Signals.
pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |acc, s| acc.max(s.abs()))
}

/// Verstärkungsfaktor, der `mono` auf `TARGET_LUFS` bringt — begrenzt durch
/// die Aussteuerungsgrenze und durch `MAX_BOOST_DB`.
///
/// `peak_of` ist der Spitzenwert des Signals, das der Faktor treffen wird.
/// Bei mehrkanaligen Dateien wird über den Mono-Downmix gemessen, aber gegen
/// die Spitze *aller* Kanäle gedeckelt — sonst clippt ein einzelner Kanal.
pub fn gain_to_target(mono: &[f32], sample_rate: u32, peak_of: f32) -> f32 {
    let Some(lufs) = loudness_lufs(mono, sample_rate) else {
        return 1.0;
    };
    let wanted = 10f32.powf((TARGET_LUFS - lufs) / 20.0);
    let max_boost = 10f32.powf(MAX_BOOST_DB / 20.0);
    // Absenken ist unbegrenzt erlaubt, Anheben nicht: aus zu leise wird sonst
    // lautes Rauschen statt einer lauten Stimme.
    let bounded = wanted.min(max_boost);
    if peak_of <= f32::EPSILON {
        return 1.0;
    }
    bounded.min(PEAK_CEILING / peak_of)
}

/// Bequemlichkeit für einkanalige Signale.
pub fn gain_for_mono(mono: &[f32], sample_rate: u32) -> f32 {
    gain_to_target(mono, sample_rate, peak(mono))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, amplitude: f32, secs: f32, rate: u32) -> Vec<f32> {
        let n = (secs * rate as f32) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / rate as f32;
                amplitude * (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect()
    }

    /// Ein Vollpegel-Sinus bei 1 kHz muss -3,01 LUFS ergeben — der Prüfstein
    /// jeder BS.1770-Umsetzung. Die Leistung ist 0,5 (10·log10(0,5) =
    /// -3,01 dB); der Offset -0,691 hebt sich gegen die K-Gewichtung auf, die
    /// bei 1 kHz genau +0,691 dB beträgt. Genau darauf ist der Offset in der
    /// Norm ausgelegt: 1 kHz bei -23 dBFS ergibt -23 LUFS.
    #[test]
    fn ein_1khz_vollpegel_sinus_misst_den_erwarteten_normwert() {
        let l = loudness_lufs(&sine(1000.0, 1.0, 3.0, 48_000), 48_000).unwrap();
        assert!((l - (-3.01)).abs() < 0.2, "gemessen {l} LUFS");
    }

    /// Die Eichung der Norm: 1 kHz bei -23 dBFS ergibt -23 LUFS.
    #[test]
    fn die_eichung_der_norm_stimmt() {
        let amplitude = 10f32.powf(-23.0 / 20.0) * 2f32.sqrt();
        let l = loudness_lufs(&sine(1000.0, amplitude, 3.0, 48_000), 48_000).unwrap();
        assert!((l - (-23.0)).abs() < 0.2, "gemessen {l} LUFS");
    }

    /// Die Messung darf nicht von der Abtastrate abhängen — sonst misst eine
    /// 16-kHz-Aufnahme anders als ein 44,1-kHz-Import.
    #[test]
    fn die_messung_haengt_nicht_an_der_abtastrate() {
        let a = loudness_lufs(&sine(440.0, 0.5, 3.0, 16_000), 16_000).unwrap();
        let b = loudness_lufs(&sine(440.0, 0.5, 3.0, 44_100), 44_100).unwrap();
        assert!((a - b).abs() < 0.3, "16k: {a}, 44k: {b}");
    }

    /// Der Kern der Sache: Pausen dürfen die Lautheit nicht verfälschen.
    /// Genau hier scheitert das alte RMS über die ganze Datei.
    #[test]
    fn pausen_veraendern_die_gemessene_lautheit_nicht() {
        let rate = 16_000;
        let dicht = sine(300.0, 0.4, 4.0, rate);
        // Dasselbe Signal, aber jede Sekunde Ton gefolgt von einer Sekunde Stille.
        let mut mit_pausen = Vec::new();
        for chunk in dicht.chunks(rate as usize) {
            mit_pausen.extend_from_slice(chunk);
            mit_pausen.extend(std::iter::repeat_n(0.0f32, rate as usize));
        }
        let a = loudness_lufs(&dicht, rate).unwrap();
        let b = loudness_lufs(&mit_pausen, rate).unwrap();
        // Nicht null: die Blöcke an den Übergängen enthalten Ton UND Stille
        // und liegen noch über dem relativen Tor. Das ist so gewollt — der
        // Rest ist der Fehler, den das alte Verfahren machte.
        assert!((a - b).abs() < 1.5, "dicht: {a}, mit Pausen: {b}");

        // Zum Vergleich: RMS über die ganze Datei liegt um ~3 dB daneben,
        // also doppelt so weit — und der Fehler wächst mit der Pausenmenge.
        let rms =
            |s: &[f32]| (s.iter().map(|v| (v * v) as f64).sum::<f64>() / s.len() as f64).sqrt();
        let rms_diff = 20.0 * (rms(&dicht) / rms(&mit_pausen)).log10();
        assert!(
            rms_diff > (a - b).abs() as f64 * 2.0,
            "RMS-Abweichung {rms_diff} dB, Lautheitsabweichung {} dB",
            (a - b).abs()
        );
    }

    /// Nach dem Anwenden des Faktors muss der Zielpegel auch erreicht sein.
    #[test]
    fn der_faktor_bringt_das_signal_auf_den_zielpegel() {
        let rate = 16_000;
        for amplitude in [0.05f32, 0.2, 0.6] {
            let s = sine(300.0, amplitude, 4.0, rate);
            let g = gain_for_mono(&s, rate);
            let scaled: Vec<f32> = s.iter().map(|v| v * g).collect();
            let l = loudness_lufs(&scaled, rate).unwrap();
            assert!(
                (l - TARGET_LUFS).abs() < 0.5,
                "Amplitude {amplitude}: {l} LUFS statt {TARGET_LUFS}"
            );
        }
    }

    /// Zwei verschieden laute Stimmen müssen hinterher gleich laut sein —
    /// das ist die eigentliche Anforderung.
    #[test]
    fn zwei_verschieden_laute_signale_werden_gleich_laut() {
        let rate = 16_000;
        // Beide innerhalb der zulässigen +12 dB Anhebung: der Deckel ist
        // Absicht (siehe `die_anhebung_ist_nach_oben_begrenzt`) und würde die
        // Aussage dieses Tests sonst verfälschen.
        let leise = sine(220.0, 0.08, 4.0, rate);
        let laut = sine(220.0, 0.8, 4.0, rate);
        let scale = |s: &[f32]| -> Vec<f32> {
            let g = gain_for_mono(s, rate);
            s.iter().map(|v| v * g).collect()
        };
        let a = loudness_lufs(&scale(&leise), rate).unwrap();
        let b = loudness_lufs(&scale(&laut), rate).unwrap();
        assert!((a - b).abs() < 0.5, "leise: {a}, laut: {b}");
    }

    #[test]
    fn stille_und_zu_kurze_signale_bleiben_unveraendert() {
        assert_eq!(gain_for_mono(&[0.0; 16_000], 16_000), 1.0);
        assert_eq!(gain_for_mono(&[], 16_000), 1.0);
        // 200 ms — kürzer als ein Messblock.
        assert_eq!(gain_for_mono(&sine(300.0, 0.5, 0.2, 16_000), 16_000), 1.0);
    }

    /// Ein einzelner Knacks darf die Aufnahme nicht ins Clipping heben.
    #[test]
    fn die_aussteuerungsgrenze_deckelt_die_anhebung() {
        let rate = 16_000;
        let mut s = sine(300.0, 0.02, 4.0, rate);
        s[100] = 0.95; // Knacks bei nahezu Vollpegel
        let g = gain_for_mono(&s, rate);
        let peak_after = peak(&s) * g;
        assert!(peak_after <= PEAK_CEILING + 1e-6, "Spitze {peak_after}");
    }

    /// Aus Rauschen darf kein Gebrüll werden: die Anhebung ist gedeckelt.
    #[test]
    fn die_anhebung_ist_nach_oben_begrenzt() {
        let rate = 16_000;
        let sehr_leise = sine(300.0, 0.0005, 4.0, rate);
        let g = gain_for_mono(&sehr_leise, rate);
        assert!(g <= 10f32.powf(MAX_BOOST_DB / 20.0) + 1e-6, "Faktor {g}");
    }
}
