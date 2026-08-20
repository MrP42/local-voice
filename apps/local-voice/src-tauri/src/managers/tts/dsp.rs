//! Der eine Filterbaustein, den alles hier benutzt.
//!
//! Ein Biquad ist ein Filter zweiter Ordnung: zwei Verzögerungen im Vorwärts-
//! und zwei im Rückwärtszweig. Damit lassen sich Hoch-, Tief- und
//! Kuhschwanzfilter bauen — die Bauform ist dieselbe, nur die fünf
//! Koeffizienten unterscheiden sich.
//!
//! Zwei Nutzer: die K-Gewichtung der Lautheitsmessung ([`super::loudness`])
//! bringt ihre Koeffizienten aus der Norm mit, die Klangbearbeitung
//! ([`super::enhance`]) rechnet sie sich aus Frequenz und Güte aus.

/// Transponierte Direktform II — die numerisch gutmütige Bauform und die
/// übliche Wahl für f64-Verarbeitung.
#[derive(Debug, Clone)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// Aus fertigen, bereits auf `a0` normierten Koeffizienten.
    pub fn new(b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Hochpass zweiter Ordnung nach der Audio-EQ-Cookbook-Formel.
    ///
    /// `q = 0.7071` ergibt den Butterworth-Verlauf: so flach wie möglich im
    /// Durchlassbereich, ohne Überhöhung an der Grenzfrequenz.
    pub fn highpass(freq_hz: f64, q: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * std::f64::consts::PI * freq_hz / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self::new(
            ((1.0 + cos_w0) / 2.0) / a0,
            (-(1.0 + cos_w0)) / a0,
            ((1.0 + cos_w0) / 2.0) / a0,
            (-2.0 * cos_w0) / a0,
            (1.0 - alpha) / a0,
        )
    }

    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Effektivwert eines Sinus nach dem Filter, bezogen auf den davor —
    /// misst die Dämpfung des Filters bei dieser Frequenz.
    fn attenuation_db(freq: f32, cutoff: f64, rate: f64) -> f64 {
        let n = (rate * 2.0) as usize;
        let mut filter = Biquad::highpass(cutoff, 0.7071, rate);
        let mut sum_in = 0.0;
        let mut sum_out = 0.0;
        for i in 0..n {
            let t = i as f64 / rate;
            let x = (2.0 * std::f64::consts::PI * freq as f64 * t).sin();
            let y = filter.process(x);
            // Die erste Zehntelsekunde ist Einschwingen und zaehlt nicht mit.
            if i > (rate / 10.0) as usize {
                sum_in += x * x;
                sum_out += y * y;
            }
        }
        10.0 * (sum_out / sum_in).log10()
    }

    /// An der Grenzfrequenz daempft ein Butterworth-Hochpass um 3 dB — der
    /// Pruefstein dafuer, dass die Koeffizienten stimmen.
    #[test]
    fn der_hochpass_daempft_an_der_grenzfrequenz_um_drei_dezibel() {
        let db = attenuation_db(80.0, 80.0, 48_000.0);
        assert!((db - (-3.01)).abs() < 0.3, "gemessen {db} dB");
    }

    /// Weit unterhalb wird stark gedaempft, weit oberhalb bleibt alles.
    #[test]
    fn tiefes_wird_gedaempft_hohes_bleibt() {
        let tief = attenuation_db(20.0, 80.0, 48_000.0);
        let hoch = attenuation_db(1000.0, 80.0, 48_000.0);
        assert!(tief < -20.0, "20 Hz nur {tief} dB gedaempft");
        assert!(hoch.abs() < 0.2, "1 kHz um {hoch} dB veraendert");
    }
}
