//! Converts spoken cardinal-number word phrases (e.g. "one hundred forty")
//! into digit form ("140") — FR-008's rule-based normalizer. Only converts
//! multi-word number phrases (a bare "one" or "twenty" is left as a word,
//! since a lone number word is usually not being used as a number). Model
//! numbers/codes read as separate digits (e.g. "four eighty" for "480") are
//! deliberately not handled here — that pattern is ambiguous with ordinary
//! cardinal speech and isn't part of this heuristic's scope.

fn normalize(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

fn ones_value(word: &str) -> Option<u32> {
    Some(match word {
        "zero" => 0,
        "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        _ => return None,
    })
}

fn teens_value(word: &str) -> Option<u32> {
    Some(match word {
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        "thirteen" => 13,
        "fourteen" => 14,
        "fifteen" => 15,
        "sixteen" => 16,
        "seventeen" => 17,
        "eighteen" => 18,
        "nineteen" => 19,
        _ => return None,
    })
}

fn tens_value(word: &str) -> Option<u32> {
    Some(match word {
        "twenty" => 20,
        "thirty" => 30,
        "forty" => 40,
        "fifty" => 50,
        "sixty" => 60,
        "seventy" => 70,
        "eighty" => 80,
        "ninety" => 90,
        _ => return None,
    })
}

/// Tries to parse a cardinal-number phrase starting at `tokens[start]`.
/// Returns `(value, words_consumed)` only for phrases of 2+ words — a
/// "<ones> hundred [and] [<tens>] [<ones>]" phrase, or a "<tens> <ones>"
/// phrase (e.g. "twenty one"). Returns `None` for anything shorter or that
/// doesn't match, leaving the original words untouched.
fn try_parse_number_phrase(tokens: &[String], start: usize) -> Option<(u32, usize)> {
    let get = |idx: usize| tokens.get(idx).map(|s| s.as_str());

    if let Some(w0) = get(start) {
        if let Some(ones) = ones_value(w0) {
            if ones >= 1 && get(start + 1) == Some("hundred") {
                let mut value = ones * 100;
                let mut consumed = 2;
                let mut j = start + 2;
                if get(j) == Some("and") {
                    j += 1;
                }
                if let Some(w) = get(j) {
                    if let Some(t) = tens_value(w) {
                        value += t;
                        consumed = j + 1 - start;
                        j += 1;
                        if let Some(w2) = get(j) {
                            if let Some(o) = ones_value(w2) {
                                if o >= 1 {
                                    value += o;
                                    consumed = j + 1 - start;
                                }
                            }
                        }
                    } else if let Some(te) = teens_value(w) {
                        value += te;
                        consumed = j + 1 - start;
                    } else if let Some(o) = ones_value(w) {
                        if o >= 1 {
                            value += o;
                            consumed = j + 1 - start;
                        }
                    }
                }
                return Some((value, consumed));
            }
        }

        if let Some(tens) = tens_value(w0) {
            if let Some(w1) = get(start + 1) {
                if let Some(ones) = ones_value(w1) {
                    if ones >= 1 {
                        return Some((tens + ones, 2));
                    }
                }
            }
        }
    }

    None
}

/// Replaces every recognized cardinal-number word phrase in `words` with its
/// digit form, leaving everything else (including lone number words)
/// unchanged.
pub fn normalize_numbers(words: &[&str]) -> Vec<String> {
    let normalized: Vec<String> = words.iter().map(|w| normalize(w)).collect();
    let mut result = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        match try_parse_number_phrase(&normalized, i) {
            Some((value, consumed)) => {
                result.push(value.to_string());
                i += consumed;
            }
            None => {
                result.push(words[i].to_string());
                i += 1;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(words: &[&str]) -> Vec<String> {
        normalize_numbers(words)
    }

    #[test]
    fn converts_simple_hundred_phrase() {
        assert_eq!(run(&["one", "hundred"]), vec!["100"]);
    }

    #[test]
    fn converts_hundred_plus_tens_phrase() {
        assert_eq!(run(&["two", "hundred", "forty"]), vec!["240"]);
    }

    #[test]
    fn converts_hundred_and_tens_phrase_with_and() {
        assert_eq!(run(&["one", "hundred", "and", "forty"]), vec!["140"]);
    }

    #[test]
    fn converts_hundred_tens_ones_phrase() {
        assert_eq!(run(&["nine", "hundred", "ninety", "nine"]), vec!["999"]);
    }

    #[test]
    fn converts_hundred_plus_teens_phrase() {
        assert_eq!(run(&["one", "hundred", "thirteen"]), vec!["113"]);
    }

    #[test]
    fn converts_tens_ones_phrase() {
        assert_eq!(run(&["twenty", "one"]), vec!["21"]);
    }

    #[test]
    fn leaves_trailing_words_after_hundred_phrase_untouched() {
        assert_eq!(
            run(&["one", "hundred", "instances"]),
            vec!["100", "instances"]
        );
    }

    #[test]
    fn leaves_lone_number_word_unchanged() {
        assert_eq!(run(&["one", "thing"]), vec!["one", "thing"]);
        assert_eq!(run(&["twenty"]), vec!["twenty"]);
    }

    #[test]
    fn does_not_convert_reversed_ones_tens_pattern() {
        // "four eighty" (a GPU-model-style digit sequence) is out of scope
        // for this cardinal-number grammar — left untouched.
        assert_eq!(run(&["four", "eighty"]), vec!["four", "eighty"]);
    }

    #[test]
    fn preserves_words_around_a_converted_phrase() {
        assert_eq!(
            run(&["about", "one", "hundred", "forty", "servers"]),
            vec!["about", "140", "servers"]
        );
    }
}
