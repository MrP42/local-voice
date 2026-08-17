//! Self-test facility: run a dictation end to end and score it, without a
//! microphone and without a human.
//!
//! The point is that an automated caller and the in-app test page reach the
//! same verdict through the same code. The comparison lives here rather than
//! in the frontend so `--transcribe-file --reference` and the Diktattest page
//! cannot drift apart.
//!
//! Audio is fed straight into the transcription pipeline. Playing a WAV
//! through the speakers and re-capturing it acoustically — which is how the
//! PowerShell harness works — measures the room as much as the software, and
//! cannot produce a repeatable latency number.

use serde::Serialize;
use specta::Type;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set for the whole process while a headless run is in flight.
///
/// A self-test must never type into whatever window happens to be focused.
/// The streaming path honours `stream_injection` from the persisted settings,
/// and those settings belong to the user's normal session — a measurement run
/// would otherwise inherit "yes, paste while I speak" and scribble into their
/// document. It only stayed harmless in testing because Enigo happens to be
/// uninitialised headlessly, which is luck, not a guarantee.
static HEADLESS_RUN: AtomicBool = AtomicBool::new(false);

pub fn begin_headless_run() {
    HEADLESS_RUN.store(true, Ordering::Release);
}

pub fn is_headless_run() -> bool {
    HEADLESS_RUN.load(Ordering::Acquire)
}

/// One run's verdict, shared by the CLI and the UI.
#[derive(Debug, Clone, Serialize, Type)]
pub struct SelfTestResult {
    /// What was supposed to be said. Empty when no reference was given, in
    /// which case the accuracy fields are meaningless and set to zero.
    pub reference: String,
    /// What the recogniser produced.
    pub recognised: String,
    /// 1.0 minus word error rate, floored at 0.
    pub accuracy: f64,
    pub reference_words: usize,
    pub correct: usize,
    pub substitutions: usize,
    pub deletions: usize,
    pub insertions: usize,
    /// Word-level alignment, so a caller can show or log what differed.
    pub diff: Vec<WordDiff>,
    /// Wall-clock milliseconds from the start of the run to each committed
    /// growth of the text. Empty for a non-streaming run.
    pub commit_times_ms: Vec<u64>,
    /// Milliseconds until the first committed text existed. `None` if none did.
    pub first_text_ms: Option<u64>,
    /// Median gap between commits, which is the number that answers "how
    /// often does text appear while I speak".
    pub median_gap_ms: Option<u64>,
    /// Total wall-clock time of the run.
    pub total_ms: u64,
    /// Seconds of audio fed in, so a speed factor can be computed.
    pub audio_secs: f64,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct WordDiff {
    pub kind: DiffKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    Same,
    /// In the reference but not recognised.
    Missing,
    /// Recognised but not in the reference.
    Extra,
    /// Recognised in place of a different reference word.
    Different,
}

/// Fold away the differences a speech model may legitimately render either
/// way: punctuation, capitalisation, and the German ß/umlaut spellings.
///
/// Number words are deliberately NOT folded onto digits. Whether "dritten" or
/// "3." comes back is a real difference between models — Parakeet normalises,
/// Nemotron does not — and hiding it would defeat the purpose of the bench.
pub fn normalize_word(word: &str) -> String {
    word.to_lowercase()
        .chars()
        .filter(|c| !matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '"' | '\'' | '„' | '“' | '»' | '«' | '(' | ')' | '[' | ']'))
        .collect::<String>()
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
        .replace('ß', "ss")
}

fn words(text: &str) -> Vec<&str> {
    text.split_whitespace().filter(|w| !w.is_empty()).collect()
}

/// Word-level alignment by edit distance, which is what word error rate is
/// defined on. A greedy diff would report one inserted word as "everything
/// after it is wrong".
pub fn compare(reference: &str, recognised: &str) -> (f64, usize, usize, usize, usize, Vec<WordDiff>) {
    let reference_words = words(reference);
    let hypothesis_words = words(recognised);
    let reference_norm: Vec<String> = reference_words.iter().map(|w| normalize_word(w)).collect();
    let hypothesis_norm: Vec<String> = hypothesis_words.iter().map(|w| normalize_word(w)).collect();

    let n = reference_words.len();
    let m = hypothesis_words.len();

    if n == 0 {
        let diff = hypothesis_words
            .iter()
            .map(|w| WordDiff {
                kind: DiffKind::Extra,
                text: (*w).to_string(),
            })
            .collect();
        return (if m == 0 { 1.0 } else { 0.0 }, 0, 0, 0, m, diff);
    }

    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=m {
        d[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(reference_norm[i - 1] != hypothesis_norm[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
        }
    }

    let mut diff = Vec::new();
    let (mut correct, mut substitutions, mut deletions, mut insertions) = (0, 0, 0, 0);
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 {
            let cost = usize::from(reference_norm[i - 1] != hypothesis_norm[j - 1]);
            if d[i][j] == d[i - 1][j - 1] + cost {
                if cost == 0 {
                    correct += 1;
                    diff.push(WordDiff {
                        kind: DiffKind::Same,
                        text: hypothesis_words[j - 1].to_string(),
                    });
                } else {
                    substitutions += 1;
                    diff.push(WordDiff {
                        kind: DiffKind::Different,
                        text: hypothesis_words[j - 1].to_string(),
                    });
                }
                i -= 1;
                j -= 1;
                continue;
            }
        }
        if i > 0 && d[i][j] == d[i - 1][j] + 1 {
            deletions += 1;
            diff.push(WordDiff {
                kind: DiffKind::Missing,
                text: reference_words[i - 1].to_string(),
            });
            i -= 1;
            continue;
        }
        insertions += 1;
        diff.push(WordDiff {
            kind: DiffKind::Extra,
            text: hypothesis_words[j - 1].to_string(),
        });
        j -= 1;
    }
    diff.reverse();

    // Insertions alone can push the error rate above 1, hence the floor.
    let errors = substitutions + deletions + insertions;
    let accuracy = (1.0 - errors as f64 / n as f64).max(0.0);
    (accuracy, correct, substitutions, deletions, insertions, diff)
}

/// Median gap between successive commit timestamps — the honest answer to
/// "how often does text appear while I speak". The mean would be dragged
/// around by the long first gap while the model warms up.
pub fn median_gap(times_ms: &[u64]) -> Option<u64> {
    if times_ms.len() < 2 {
        return None;
    }
    let mut gaps: Vec<u64> = times_ms.windows(2).map(|w| w[1].saturating_sub(w[0])).collect();
    gaps.sort_unstable();
    Some(gaps[gaps.len() / 2])
}

impl SelfTestResult {
    pub fn build(
        reference: &str,
        recognised: &str,
        commit_times_ms: Vec<u64>,
        total_ms: u64,
        audio_secs: f64,
    ) -> Self {
        let (accuracy, correct, substitutions, deletions, insertions, diff) =
            compare(reference, recognised);
        Self {
            reference: reference.to_string(),
            recognised: recognised.to_string(),
            accuracy,
            reference_words: words(reference).len(),
            correct,
            substitutions,
            deletions,
            insertions,
            diff,
            first_text_ms: commit_times_ms.first().copied(),
            median_gap_ms: median_gap(&commit_times_ms),
            commit_times_ms,
            total_ms,
            audio_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_scores_perfectly() {
        let (accuracy, correct, subs, dels, ins, _) =
            compare("Der Termin ist am dritten Februar.", "Der Termin ist am dritten Februar.");
        assert_eq!(accuracy, 1.0);
        assert_eq!(correct, 6);
        assert_eq!((subs, dels, ins), (0, 0, 0));
    }

    /// Punctuation, case and umlaut spelling are not recognition errors.
    #[test]
    fn cosmetic_differences_do_not_count_as_errors() {
        let (accuracy, ..) = compare("Straße, Köln!", "strasse koeln");
        assert_eq!(accuracy, 1.0);
    }

    /// The failure this bench exists to catch: streaming with too little
    /// look-ahead produced "Spracherken Termin" instead of
    /// "Spracherkennung. Der Termin".
    #[test]
    fn damaged_streaming_output_is_scored_as_damaged() {
        let (accuracy, _, subs, dels, _, _) =
            compare("der lokalen Spracherkennung. Der Termin ist", "der lokalen Spracherken Termin ist");
        assert!(accuracy < 1.0, "damage must not score as perfect");
        assert!(subs + dels > 0);
    }

    #[test]
    fn a_missing_word_is_a_deletion_not_a_cascade() {
        let (_, correct, subs, dels, ins, _) = compare("eins zwei drei vier", "eins drei vier");
        assert_eq!((correct, subs, dels, ins), (3, 0, 1, 0));
    }

    #[test]
    fn an_extra_word_is_an_insertion() {
        let (_, correct, subs, dels, ins, _) = compare("eins zwei", "eins und zwei");
        assert_eq!((correct, subs, dels, ins), (2, 0, 0, 1));
    }

    #[test]
    fn empty_reference_reports_everything_as_extra() {
        let (accuracy, _, _, _, ins, diff) = compare("", "unerwarteter Text");
        assert_eq!(accuracy, 0.0);
        assert_eq!(ins, 2);
        assert!(diff.iter().all(|d| d.kind == DiffKind::Extra));
    }

    #[test]
    fn median_gap_ignores_the_long_warm_up_gap() {
        // A slow first commit followed by steady ones must not dominate.
        assert_eq!(median_gap(&[3000, 3500, 4000, 4500]), Some(500));
        assert_eq!(median_gap(&[100]), None);
        assert_eq!(median_gap(&[]), None);
    }
}
