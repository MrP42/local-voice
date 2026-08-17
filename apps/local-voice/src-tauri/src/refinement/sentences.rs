use std::ops::Range;

const COMMON_ABBREVIATIONS: &[&str] = &[
    "bzw", "ca", "d", "dr", "etc", "ggf", "nr", "prof", "s", "u", "usw", "vgl", "z",
];

pub(crate) fn complete_sentence_ranges(text: &str, after: usize) -> Vec<Range<usize>> {
    if after > text.len() || !text.is_char_boundary(after) {
        return Vec::new();
    }

    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut ranges = Vec::new();
    let mut sentence_start = skip_whitespace(text, after);

    for (position, &(byte_index, ch)) in chars.iter().enumerate() {
        if byte_index < sentence_start || !matches!(ch, '.' | '!' | '?') {
            continue;
        }
        if ch == '.' && (is_decimal_point(&chars, position) || is_abbreviation(text, byte_index)) {
            continue;
        }

        let mut end = byte_index + ch.len_utf8();
        while let Some(closing) = text[end..].chars().next() {
            if matches!(closing, '"' | '\'' | '”' | ')' | ']' | '}') {
                end += closing.len_utf8();
            } else {
                break;
            }
        }
        if text[end..]
            .chars()
            .next()
            .is_some_and(|next| !next.is_whitespace())
        {
            continue;
        }
        if sentence_start < end {
            ranges.push(sentence_start..end);
        }
        sentence_start = skip_whitespace(text, end);
    }

    ranges
}

fn skip_whitespace(text: &str, mut index: usize) -> usize {
    while let Some(ch) = text[index..].chars().next() {
        if !ch.is_whitespace() {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

fn is_decimal_point(chars: &[(usize, char)], position: usize) -> bool {
    position
        .checked_sub(1)
        .and_then(|index| chars.get(index))
        .is_some_and(|(_, ch)| ch.is_ascii_digit())
        && chars
            .get(position + 1)
            .is_some_and(|(_, ch)| ch.is_ascii_digit())
}

fn is_abbreviation(text: &str, period_index: usize) -> bool {
    let prefix = &text[..period_index];
    let word_start = prefix
        .char_indices()
        .rev()
        .find_map(|(index, ch)| (!ch.is_alphabetic()).then_some(index + ch.len_utf8()))
        .unwrap_or(0);
    let word = prefix[word_start..].to_lowercase();
    word.chars().count() == 1 || COMMON_ABBREVIATIONS.contains(&word.as_str())
}

#[cfg(test)]
mod tests {
    use super::complete_sentence_ranges;

    fn sentences(text: &str) -> Vec<&str> {
        complete_sentence_ranges(text, 0)
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }

    #[test]
    fn finds_complete_german_sentences_but_not_incomplete_tail() {
        assert_eq!(sentences("Das geht. Weiter"), vec!["Das geht."]);
        assert_eq!(
            sentences("Erstens! Zweitens? Noch offen"),
            vec!["Erstens!", "Zweitens?"]
        );
        assert!(sentences("Noch nicht fertig").is_empty());
    }

    #[test]
    fn decimal_points_are_not_sentence_boundaries() {
        assert_eq!(
            sentences("Der Wert ist 3.14. Weiter"),
            vec!["Der Wert ist 3.14."]
        );
    }

    #[test]
    fn common_abbreviations_are_not_sentence_boundaries() {
        assert_eq!(
            sentences("Das ist z. B. korrekt. Weiter"),
            vec!["Das ist z. B. korrekt."]
        );
        assert_eq!(
            sentences("Dr. Müller kommt. Weiter"),
            vec!["Dr. Müller kommt."]
        );
    }

    #[test]
    fn scan_offset_prevents_duplicate_sentence_jobs() {
        let text = "Eins. Zwei.";
        assert_eq!(
            complete_sentence_ranges(text, "Eins.".len()),
            vec!["Eins. ".len().."Eins. Zwei.".len()]
        );
    }
}
