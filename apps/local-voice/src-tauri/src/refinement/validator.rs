use std::collections::{BTreeMap, HashMap, HashSet};

const MIN_LCS_COVERAGE: f64 = 0.90;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValidationFailure {
    Numbers,
    Negations,
    RareTerms,
    InventedContent,
    Order,
}

#[derive(Debug, Clone)]
struct Token {
    raw: String,
    normalized: String,
}

pub(crate) fn validate(original: &str, candidate: &str) -> Result<(), ValidationFailure> {
    let original_tokens = tokenize(original);
    let candidate_tokens = tokenize(candidate);

    if number_multiset(&original_tokens) != number_multiset(&candidate_tokens) {
        return Err(ValidationFailure::Numbers);
    }
    if negation_multiset(&original_tokens) != negation_multiset(&candidate_tokens) {
        return Err(ValidationFailure::Negations);
    }
    if !rare_terms(original, &original_tokens).is_subset(&rare_terms(candidate, &candidate_tokens))
    {
        return Err(ValidationFailure::RareTerms);
    }
    if !candidate_content_is_contained(&original_tokens, &candidate_tokens) {
        return Err(ValidationFailure::InventedContent);
    }
    if lcs_coverage(&original_tokens, &candidate_tokens) < MIN_LCS_COVERAGE {
        return Err(ValidationFailure::Order);
    }

    Ok(())
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            current.push(ch);
        } else if !current.is_empty() {
            push_token(&mut tokens, &mut current);
        }
    }
    if !current.is_empty() {
        push_token(&mut tokens, &mut current);
    }

    tokens
}

fn push_token(tokens: &mut Vec<Token>, current: &mut String) {
    let raw = std::mem::take(current);
    let normalized = normalize_word(&raw);
    tokens.push(Token { raw, normalized });
}

fn normalize_word(word: &str) -> String {
    word.to_lowercase()
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
        .replace('ß', "ss")
}

fn number_multiset(tokens: &[Token]) -> BTreeMap<String, usize> {
    let mut numbers = BTreeMap::new();
    for token in tokens {
        if let Some(value) = canonical_number(&token.normalized) {
            *numbers.entry(value).or_default() += 1;
        }
    }
    numbers
}

fn canonical_number(token: &str) -> Option<String> {
    if !token.is_empty() && token.chars().all(|ch| ch.is_ascii_digit()) {
        let trimmed = token.trim_start_matches('0');
        return Some(if trimmed.is_empty() { "0" } else { trimmed }.to_string());
    }

    parse_german_integer(token).map(|value| value.to_string())
}

fn parse_german_integer(word: &str) -> Option<u64> {
    let direct = match word {
        "null" => Some(0),
        "ein" | "eins" | "eine" | "einen" | "einem" | "einer" | "eines" => Some(1),
        "zwei" => Some(2),
        "drei" => Some(3),
        "vier" => Some(4),
        "fuenf" => Some(5),
        "sechs" => Some(6),
        "sieben" => Some(7),
        "acht" => Some(8),
        "neun" => Some(9),
        "zehn" => Some(10),
        "elf" => Some(11),
        "zwoelf" => Some(12),
        "dreizehn" => Some(13),
        "vierzehn" => Some(14),
        "fuenfzehn" => Some(15),
        "sechzehn" => Some(16),
        "siebzehn" => Some(17),
        "achtzehn" => Some(18),
        "neunzehn" => Some(19),
        "zwanzig" => Some(20),
        "dreissig" => Some(30),
        "vierzig" => Some(40),
        "fuenfzig" => Some(50),
        "sechzig" => Some(60),
        "siebzig" => Some(70),
        "achtzig" => Some(80),
        "neunzig" => Some(90),
        _ => None,
    };
    if direct.is_some() {
        return direct;
    }

    if let Some((left, right)) = word.split_once("tausend") {
        let thousands = if left.is_empty() {
            1
        } else {
            parse_german_integer(left)?
        };
        if thousands > 999 {
            return None;
        }
        let remainder = if right.is_empty() {
            0
        } else {
            parse_german_integer(right)?
        };
        if remainder > 999 {
            return None;
        }
        return thousands.checked_mul(1000)?.checked_add(remainder);
    }

    if let Some((left, right)) = word.split_once("hundert") {
        let hundreds = if left.is_empty() {
            1
        } else {
            parse_german_integer(left)?
        };
        if !(1..=9).contains(&hundreds) {
            return None;
        }
        let remainder = if right.is_empty() {
            0
        } else {
            parse_german_integer(right)?
        };
        if remainder > 99 {
            return None;
        }
        return hundreds.checked_mul(100)?.checked_add(remainder);
    }

    if let Some((left, right)) = word.split_once("und") {
        let unit = parse_german_integer(left)?;
        let tens = parse_german_integer(right)?;
        if (1..=9).contains(&unit) && (20..=90).contains(&tens) && tens % 10 == 0 {
            return Some(tens + unit);
        }
    }

    None
}

fn negation_multiset(tokens: &[Token]) -> BTreeMap<String, usize> {
    let mut negations = BTreeMap::new();
    for token in tokens {
        let key = if token.normalized.starts_with("kein") {
            Some("kein")
        } else {
            match token.normalized.as_str() {
                "nicht" => Some("nicht"),
                "nichts" => Some("nichts"),
                "nie" => Some("nie"),
                "niemals" => Some("niemals"),
                "ohne" => Some("ohne"),
                "weder" => Some("weder"),
                _ => None,
            }
        };
        if let Some(key) = key {
            *negations.entry(key.to_string()).or_default() += 1;
        }
    }
    negations
}

fn rare_terms(text: &str, tokens: &[Token]) -> HashSet<String> {
    let mut counts = HashMap::<&str, usize>::new();
    for token in tokens {
        *counts.entry(&token.normalized).or_default() += 1;
    }

    let mut rare = HashSet::new();
    for chunk in text.split_whitespace().map(trim_structured_token) {
        if is_structured_term(chunk) {
            rare.insert(format!("exact:{chunk}"));
        }
    }

    for token in tokens {
        if canonical_number(&token.normalized).is_some()
            || counts.get(token.normalized.as_str()).copied() != Some(1)
        {
            continue;
        }

        let letters: Vec<char> = token.raw.chars().filter(|ch| ch.is_alphabetic()).collect();
        let is_acronym = letters.len() >= 2 && letters.iter().all(|ch| ch.is_uppercase());
        let has_inner_uppercase = token
            .raw
            .chars()
            .scan(false, |seen_lowercase, ch| {
                let inner_upper = *seen_lowercase && ch.is_uppercase();
                *seen_lowercase |= ch.is_lowercase();
                Some(inner_upper)
            })
            .any(|inner_upper| inner_upper);
        let has_letters_and_digits = token.raw.chars().any(|ch| ch.is_alphabetic())
            && token.raw.chars().any(|ch| ch.is_ascii_digit());
        let is_long_hapax = token.raw.chars().count() >= 16;
        let is_hyphenated = token.raw.contains('-') && token.raw.chars().count() >= 8;

        if is_acronym
            || has_inner_uppercase
            || has_letters_and_digits
            || is_long_hapax
            || is_hyphenated
        {
            rare.insert(format!("word:{}", token.normalized));
        }
    }

    rare
}

fn trim_structured_token(token: &str) -> &str {
    token.trim_matches(|ch: char| {
        matches!(
            ch,
            '"' | '\''
                | '“'
                | '”'
                | '„'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | ','
                | ';'
                | ':'
                | '!'
                | '?'
                | '.'
        )
    })
}

fn is_structured_term(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let is_url = token.contains("://");
    let is_email = token
        .split_once('@')
        .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'));
    let is_windows_path =
        token.as_bytes().get(1) == Some(&b':') && (token.contains('\\') || token.contains('/'));
    let is_unc_path = token.starts_with("\\\\");
    let is_posix_path = token.starts_with('/') && token.len() > 1;
    is_url || is_email || is_windows_path || is_unc_path || is_posix_path
}

fn semantic_token(token: &Token) -> String {
    if token.normalized.starts_with("kein") {
        return "#negation:kein".to_string();
    }
    canonical_number(&token.normalized)
        .map(|value| format!("#number:{value}"))
        .unwrap_or_else(|| token.normalized.clone())
}

fn candidate_content_is_contained(original: &[Token], candidate: &[Token]) -> bool {
    let mut available = HashMap::<String, usize>::new();
    for token in original {
        let semantic = semantic_token(token);
        if is_content_token(&semantic) {
            *available.entry(semantic).or_default() += 1;
        }
    }

    for token in candidate {
        let semantic = semantic_token(token);
        if !is_content_token(&semantic) {
            continue;
        }
        let Some(count) = available.get_mut(&semantic) else {
            return false;
        };
        if *count == 0 {
            return false;
        }
        *count -= 1;
    }
    true
}

fn is_content_token(token: &str) -> bool {
    if token.starts_with("#number:") {
        return true;
    }
    !matches!(
        token,
        "aber"
            | "als"
            | "am"
            | "an"
            | "auch"
            | "auf"
            | "aus"
            | "bei"
            | "bis"
            | "da"
            | "das"
            | "dass"
            | "dem"
            | "den"
            | "der"
            | "des"
            | "die"
            | "doch"
            | "durch"
            | "ein"
            | "eine"
            | "einem"
            | "einen"
            | "einer"
            | "eines"
            | "er"
            | "es"
            | "fuer"
            | "hat"
            | "ich"
            | "im"
            | "in"
            | "ist"
            | "ja"
            | "mit"
            | "nach"
            | "oder"
            | "sie"
            | "so"
            | "ueber"
            | "um"
            | "und"
            | "vom"
            | "von"
            | "vor"
            | "war"
            | "wie"
            | "wir"
            | "zu"
            | "zum"
            | "zur"
    )
}

fn lcs_coverage(original: &[Token], candidate: &[Token]) -> f64 {
    if original.is_empty() {
        return if candidate.is_empty() { 1.0 } else { 0.0 };
    }

    let original: Vec<String> = original.iter().map(semantic_token).collect();
    let candidate: Vec<String> = candidate.iter().map(semantic_token).collect();
    let mut previous = vec![0usize; candidate.len() + 1];

    for original_token in &original {
        let mut current = vec![0usize; candidate.len() + 1];
        for (index, candidate_token) in candidate.iter().enumerate() {
            current[index + 1] = if original_token == candidate_token {
                previous[index] + 1
            } else {
                current[index].max(previous[index + 1])
            };
        }
        previous = current;
    }

    previous[candidate.len()] as f64 / original.len() as f64
}

#[cfg(test)]
mod tests {
    use super::{validate, ValidationFailure};

    #[test]
    fn numbers_compare_canonical_values() {
        assert!(validate("Ich brauche zweiundzwanzig Teile.", "Ich brauche 22 Teile.").is_ok());
        assert_eq!(
            validate("Ich brauche 22 Teile.", "Ich brauche 23 Teile."),
            Err(ValidationFailure::Numbers)
        );
    }

    #[test]
    fn distinct_negations_cannot_replace_each_other() {
        assert_eq!(
            validate("Das ist nicht nichts.", "Das ist nichts nichts."),
            Err(ValidationFailure::Negations)
        );
    }

    #[test]
    fn kein_flexions_share_one_strict_key() {
        assert!(validate("Keinen Fehler machen.", "Kein Fehler machen.").is_ok());
        assert_eq!(
            validate("Kein Fehler.", "Ohne Fehler."),
            Err(ValidationFailure::Negations)
        );
    }

    #[test]
    fn rare_terms_and_structured_tokens_must_survive() {
        for (original, candidate) in [
            ("OpenAI bleibt.", "Die Plattform bleibt."),
            ("RTX4090 bleibt.", "Hardware bleibt."),
            ("API bleibt.", "Schnittstelle bleibt."),
            ("Mail an test@example.com.", "Mail an jemand."),
            ("Öffne C:\\Temp\\Modell.gguf.", "Öffne die Datei."),
            ("Besuche https://example.com/Modell.", "Besuche die Seite."),
        ] {
            assert_eq!(
                validate(original, candidate),
                Err(ValidationFailure::RareTerms),
                "{original}"
            );
        }
    }

    #[test]
    fn ordinary_german_capitalized_nouns_are_not_names_by_case_alone() {
        assert_ne!(
            validate(
                "Der Server verarbeitet Daten.",
                "Der Rechner verarbeitet Daten."
            ),
            Err(ValidationFailure::RareTerms)
        );
    }

    #[test]
    fn candidate_cannot_invent_content_tokens() {
        assert_eq!(
            validate(
                "Der Server verarbeitet Daten.",
                "Der Server analysiert Daten."
            ),
            Err(ValidationFailure::InventedContent)
        );
    }

    #[test]
    fn word_to_digit_normalization_is_allowed_by_content_gate() {
        assert!(validate("Ich brauche zweiundzwanzig Teile.", "Ich brauche 22 Teile.").is_ok());
    }

    #[test]
    fn token_order_requires_ninety_percent_lcs_coverage() {
        assert_eq!(
            validate(
                "eins zwei drei vier fünf sechs sieben acht neun zehn",
                "zehn neun acht sieben sechs fünf vier drei zwei eins"
            ),
            Err(ValidationFailure::Order)
        );
    }

    #[test]
    fn punctuation_case_and_light_smoothing_are_accepted() {
        assert!(validate("das ist also ein test", "Das ist also ein Test.").is_ok());
    }
}
