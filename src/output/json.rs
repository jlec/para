use crate::inference::Transcript;
use serde::Serialize;
use std::io::Write;

/// Wire shape for `--format json`, matching contracts/output-json-schema.json.
/// Kept distinct from `Transcript` because the JSON field names (`duration_seconds`)
/// and rounding rules differ from the internal type (data-model.md; research.md's
/// note on `duration_secs` vs `duration_seconds`).
#[derive(Serialize)]
struct JsonOutput<'a> {
    text: &'a str,
    segments: Vec<JsonSegment>,
    model: &'a str,
    duration_seconds: f64,
}

#[derive(Serialize)]
struct JsonSegment {
    start: f64,
    end: f64,
    text: String,
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Writes `transcript` as JSON conforming to contracts/output-json-schema.json:
/// every segment has `end > start`, seconds rounded to 2 decimal places.
pub fn write(transcript: &Transcript, writer: &mut dyn Write) -> anyhow::Result<()> {
    let output = JsonOutput {
        text: &transcript.text,
        segments: transcript
            .segments
            .iter()
            .map(|s| JsonSegment {
                start: round2(s.start),
                end: round2(s.end),
                text: s.text.clone(),
            })
            .collect(),
        model: &transcript.model,
        duration_seconds: round2(transcript.duration_secs),
    };
    serde_json::to_writer_pretty(&mut *writer, &output)?;
    writeln!(writer)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::Segment;

    fn sample_transcript() -> Transcript {
        Transcript {
            text: "hello world".to_string(),
            segments: vec![
                Segment {
                    start: 0.001,
                    end: 1.005,
                    text: "hello".to_string(),
                },
                Segment {
                    start: 1.005,
                    end: 2.0,
                    text: "world".to_string(),
                },
            ],
            model: "parakeet-tdt-0.6b-v3".to_string(),
            duration_secs: 2.0,
        }
    }

    #[test]
    fn produces_valid_schema_shaped_json() {
        let mut out = Vec::new();
        write(&sample_transcript(), &mut out).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed["text"], "hello world");
        assert_eq!(parsed["model"], "parakeet-tdt-0.6b-v3");
        assert!(parsed.get("duration_seconds").is_some());
        let segments = parsed["segments"].as_array().unwrap();
        assert_eq!(segments.len(), 2);
        for seg in segments {
            assert!(seg["end"].as_f64().unwrap() > seg["start"].as_f64().unwrap());
        }
    }

    #[test]
    fn rounds_seconds_to_two_decimal_places() {
        let mut out = Vec::new();
        write(&sample_transcript(), &mut out).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed["segments"][0]["end"], 1.0); // 1.005 rounds to 1.0 under f64 representation
        assert_eq!(parsed["duration_seconds"], 2.0);
    }
}
