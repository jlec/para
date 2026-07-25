pub mod segments;
pub mod swift_bridge;

/// The result of transcribing one input file.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Transcript {
    pub text: String,
    pub segments: Vec<Segment>,
    pub model: String,
    pub duration_secs: f64,
}

/// One timed unit of a transcript. `start`/`end` are seconds; `end` must be > `start`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}
