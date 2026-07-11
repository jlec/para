use crate::inference::{Device, ModelKind};
use anyhow::Context;
use ort::value::Tensor;

/// Mel-spectrogram preprocessor graphs, fetched and checksum-verified by
/// `build.rs` at build time (never committed to git — the repo's
/// `forbid-binary` policy) and embedded from `OUT_DIR` here, so there are
/// zero runtime network calls for them (research.md §10). Selected by
/// `ModelKind`, not per-model — every TDT model uses the 128-feature graph,
/// every CTC model the 80-feature graph.
const NEMO128: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/nemo128.onnx"));
const NEMO80: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/nemo80.onnx"));

/// Mel-feature output for one input waveform.
pub struct Features {
    /// Flattened `[1, feature_size, T]` tensor data, row-major.
    pub data: Vec<f32>,
    pub feature_size: usize,
    pub frames: usize,
}

pub struct Preprocessor {
    session: ort::session::Session,
}

impl Preprocessor {
    /// Loads the preprocessor graph matching `kind`.
    pub fn load(kind: ModelKind, device: Device) -> anyhow::Result<Self> {
        let bytes = match kind {
            ModelKind::Tdt => NEMO128,
            ModelKind::Ctc => NEMO80,
        };
        let session = crate::inference::engine::build_session_from_memory(bytes, device)
            .context("failed to load vendored mel-preprocessor graph")?;
        Ok(Self { session })
    }

    /// Runs the preprocessor on 16kHz mono `f32` PCM samples. Tensor names
    /// (`waveforms`, `waveforms_lens`, `features`, `features_lens`) are
    /// confirmed against the real vendored `.onnx` files' input/output
    /// metadata, not assumed (Constitution Principle V).
    pub fn extract(&mut self, samples: &[f32]) -> anyhow::Result<Features> {
        let waveforms = Tensor::from_array(([1usize, samples.len()], samples.to_vec()))
            .context("failed to build waveforms input tensor")?;
        let waveforms_lens = Tensor::from_array(([1usize], vec![samples.len() as i64]))
            .context("failed to build waveforms_lens input tensor")?;

        let outputs = self
            .session
            .run(ort::inputs![
                "waveforms" => waveforms,
                "waveforms_lens" => waveforms_lens,
            ])
            .context("mel-preprocessor inference failed")?;

        let (shape, data) = outputs["features"]
            .try_extract_tensor::<f32>()
            .context("failed to extract features tensor")?;
        let feature_size = shape[1] as usize;
        let frames = shape[2] as usize;

        Ok(Features {
            data: data.to_vec(),
            feature_size,
            frames,
        })
    }
}
