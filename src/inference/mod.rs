pub mod decoder;
pub mod engine;
pub mod mel;

/// The result of transcribing one input file.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Transcript {
    pub text: String,
    pub segments: Vec<Segment>,
    pub model: String,
    pub duration_secs: f64,
}

/// One timed unit of a transcript. `start`/`end` are seconds; `end` must be > `start`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Which decode path a model uses. TDT models emit segment-level timestamps;
/// CTC models emit a single segment spanning the whole input (data-model.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Tdt,
    Ctc,
}

/// Execution provider selection for the ONNX Runtime session (contracts/cli-interface.md `--device`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Device {
    #[default]
    Auto,
    Coreml,
    Cpu,
}
