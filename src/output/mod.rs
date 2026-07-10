pub mod json;
pub mod srt;
pub mod text;

use crate::inference::Transcript;
use std::io::Write;

/// The three output forms a run can produce (spec.md FR-007).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Srt,
}

/// Writes `transcript` to `writer` in the requested `format`. Callers pass
/// either stdout or an output file (contracts/cli-interface.md).
pub fn write_transcript(
    format: OutputFormat,
    transcript: &Transcript,
    writer: &mut dyn Write,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Text => text::write(transcript, writer),
        OutputFormat::Json => json::write(transcript, writer),
        OutputFormat::Srt => srt::write(transcript, writer),
    }
}
