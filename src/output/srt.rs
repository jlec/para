use crate::inference::Transcript;
use std::io::Write;

/// Formats seconds as `HH:MM:SS,mmm` — the SRT spec's timestamp format. The
/// millisecond separator is a comma, not a period (contracts/output-srt.md);
/// getting this wrong causes some subtitle tools to reject the file silently.
fn fmt_srt_time(secs: f64) -> String {
    let total_ms = (secs * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let s = (total_ms / 1000) % 60;
    let m = (total_ms / 60_000) % 60;
    let h = total_ms / 3_600_000;
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

/// Writes `transcript` as a sequence of numbered, comma-timestamped SRT
/// blocks (FR-006; contracts/output-srt.md). A transcript with a single
/// segment (e.g. a short clip with no internal pauses) produces exactly one
/// block.
pub fn write(transcript: &Transcript, writer: &mut dyn Write) -> anyhow::Result<()> {
    for (i, segment) in transcript.segments.iter().enumerate() {
        writeln!(writer, "{}", i + 1)?;
        writeln!(
            writer,
            "{} --> {}",
            fmt_srt_time(segment.start),
            fmt_srt_time(segment.end)
        )?;
        writeln!(writer, "{}", segment.text)?;
        writeln!(writer)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::Segment;

    #[test]
    fn formats_timestamp_with_comma_separator() {
        assert_eq!(fmt_srt_time(3661.5), "01:01:01,500");
    }

    #[test]
    fn formats_zero() {
        assert_eq!(fmt_srt_time(0.0), "00:00:00,000");
    }

    #[test]
    fn writes_numbered_blocks_with_blank_line_separator() {
        let transcript = Transcript {
            text: "hello world".to_string(),
            segments: vec![
                Segment {
                    start: 0.0,
                    end: 2.4,
                    text: "First segment text".to_string(),
                },
                Segment {
                    start: 2.4,
                    end: 4.1,
                    text: "Second segment text".to_string(),
                },
            ],
            model: "parakeet-tdt-0.6b-v3".to_string(),
            duration_secs: 4.1,
        };
        let mut out = Vec::new();
        write(&transcript, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(
            text,
            "1\n00:00:00,000 --> 00:00:02,400\nFirst segment text\n\n\
             2\n00:00:02,400 --> 00:00:04,100\nSecond segment text\n\n"
        );
    }

    #[test]
    fn single_segment_transcript_produces_one_block() {
        let transcript = Transcript {
            text: "whole file text".to_string(),
            segments: vec![Segment {
                start: 0.0,
                end: 5.0,
                text: "whole file text".to_string(),
            }],
            model: "parakeet-tdt-0.6b-v3".to_string(),
            duration_secs: 5.0,
        };
        let mut out = Vec::new();
        write(&transcript, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.matches("-->").count(), 1);
        assert!(text.starts_with('1'));
    }
}
