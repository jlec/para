use crate::inference::Transcript;
use std::io::Write;

/// Writes only the transcript text plus a trailing newline — nothing else
/// (FR-004, FR-012; contracts/cli-interface.md's stdout contract).
pub fn write(transcript: &Transcript, writer: &mut dyn Write) -> anyhow::Result<()> {
    writeln!(writer, "{}", transcript.text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_text_and_trailing_newline_only() {
        let transcript = Transcript {
            text: "hello world".to_string(),
            segments: vec![],
            model: "test-model".to_string(),
            duration_secs: 1.0,
        };
        let mut out = Vec::new();
        write(&transcript, &mut out).unwrap();
        assert_eq!(out, b"hello world\n");
    }
}
