//! Turns FluidAudio's real, word-level timestamps
//! (`swift_bridge::WordTiming`) into para's own `Segment`/`Transcript`
//! shapes — the native CoreML backend produces one flat word sequence per
//! file, not pre-grouped phrases, so this project still owns segment
//! (pause-based paragraph) grouping and filler-word removal
//! (004-native-coreml-backend spec.md FR-006/FR-007).

use crate::inference::Segment;
use crate::inference::swift_bridge::WordTiming;

/// Silence gap (seconds) between two consecutive words' timestamps past
/// which a new segment/paragraph starts (same threshold the ONNX-era
/// decoder used for its own segment grouping, kept for continuity of
/// output "feel").
const SEGMENT_GAP_SECONDS: f64 = 1.5;

/// Non-lexical filler words removed from the output (FR-006) — the same
/// small, deliberately narrow set FluidAudio's own CLI utility uses
/// (`FluidAudioCLI/Utils/TextNormalizer.swift`), not a broader disfluency
/// model. Matched whole-word, case-insensitively.
const FILLER_WORDS: &[&str] = &["um", "uh", "hmm", "mm", "mhm", "mmm"];

fn is_filler(word: &str) -> bool {
    let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric());
    FILLER_WORDS.iter().any(|f| trimmed.eq_ignore_ascii_case(f))
}

/// Groups a flat word sequence into paragraph-level `Segment`s wherever the
/// gap to the previous word exceeds `SEGMENT_GAP_SECONDS`, dropping filler
/// words from the segment text (FR-006/FR-007). Segments are contiguous and
/// ordered; a `words` slice with no large gaps produces exactly one segment.
pub fn build_segments(words: &[WordTiming]) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current: Vec<&WordTiming> = Vec::new();

    for word in words {
        if let Some(last) = current.last() {
            if word.start_secs - last.end_secs > SEGMENT_GAP_SECONDS {
                if let Some(seg) = finish_segment(&current) {
                    segments.push(seg);
                }
                current.clear();
            }
        }
        current.push(word);
    }
    if let Some(seg) = finish_segment(&current) {
        segments.push(seg);
    }
    segments
}

fn finish_segment(words: &[&WordTiming]) -> Option<Segment> {
    let non_filler: Vec<&&WordTiming> = words.iter().filter(|w| !is_filler(&w.word)).collect();
    if non_filler.is_empty() {
        return None;
    }
    let start = non_filler.first().unwrap().start_secs;
    let end = non_filler.last().unwrap().end_secs;
    let text = non_filler
        .iter()
        .map(|w| w.word.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    Some(Segment { start, end, text })
}

/// Joins segments into the final plain-text `Transcript.text`: a blank line
/// between segments whose gap triggered a new paragraph, a single space
/// otherwise. In practice every `Segment` from `build_segments` already
/// represents one paragraph, so this always inserts a paragraph break
/// between segments (FR-007).
pub fn join_as_paragraphs(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(word: &str, start: f64, end: f64) -> WordTiming {
        WordTiming {
            word: word.to_string(),
            start_secs: start,
            end_secs: end,
        }
    }

    #[test]
    fn single_segment_when_no_large_gaps() {
        let words = vec![w("hello", 0.0, 0.5), w("world", 0.6, 1.0)];
        let segs = build_segments(&words);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "hello world");
        assert_eq!(segs[0].start, 0.0);
        assert_eq!(segs[0].end, 1.0);
    }

    #[test]
    fn large_gap_starts_a_new_segment() {
        let words = vec![w("hello", 0.0, 0.5), w("world", 5.0, 5.5)];
        let segs = build_segments(&words);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "hello");
        assert_eq!(segs[1].text, "world");
    }

    #[test]
    fn filler_words_are_removed_from_segment_text() {
        let words = vec![
            w("um", 0.0, 0.2),
            w("hello", 0.3, 0.6),
            w("uh", 0.7, 0.8),
            w("world", 0.9, 1.2),
        ];
        let segs = build_segments(&words);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "hello world");
    }

    #[test]
    fn ordinary_word_resembling_a_filler_is_not_removed() {
        // "Um" alone as a filler is removed, but this guards against a
        // substring match accidentally eating part of a real word.
        let words = vec![w("Hummus", 0.0, 0.5)];
        let segs = build_segments(&words);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "Hummus");
    }

    #[test]
    fn segment_of_only_filler_words_is_dropped_entirely() {
        let words = vec![w("um", 0.0, 0.2), w("uh", 5.0, 5.2)];
        let segs = build_segments(&words);
        assert!(segs.is_empty());
    }

    #[test]
    fn join_as_paragraphs_separates_with_blank_line() {
        let segs = vec![
            Segment {
                start: 0.0,
                end: 1.0,
                text: "first".to_string(),
            },
            Segment {
                start: 5.0,
                end: 6.0,
                text: "second".to_string(),
            },
        ];
        assert_eq!(join_as_paragraphs(&segs), "first\n\nsecond");
    }
}
