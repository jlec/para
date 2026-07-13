use crate::inference::Device;
use crate::inference::mel::{self, Features};
use anyhow::Context;
use ort::ep::{CPU, CoreML, ExecutionProviderDispatch};
use ort::session::Session;
use ort::value::Tensor;
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Audio sample rate throughout this pipeline (16 kHz mono — `audio.rs` transcodes to this).
const SAMPLE_RATE: usize = 16_000;

/// Single-pass chunk length, in seconds (research.md §6). Set well under the
/// empirically-found ~400s hard cutoff (a fixed-size relative positional
/// encoding buffer inside the TDT encoder graph), with margin for models that
/// haven't been checked the same way yet.
const CHUNK_SECONDS: f64 = 300.0;

static COREML_NOTICE_SHOWN: AtomicBool = AtomicBool::new(false);

/// Whether this target could plausibly run the CoreML execution provider.
/// (`Device::Auto` only tries CoreML on darwin/aarch64 — Constitution Principle VI.)
fn coreml_capable_target() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

/// Directory CoreML caches its compiled models in, so the ~30-60s compile
/// (Apple Neural Engine) only happens once ever, not on every session load
/// (`ort`'s `CoreML::with_model_cache_dir` — without it, ORT's own docs say
/// it "will be compiled and saved to disk on each instantiation of a
/// session", which is exactly the repeated-compile behavior this fixes).
/// Shares the same cache root as downloaded models so `--cache-dir`/
/// `PARA_CACHE_DIR` consistently relocates everything `para` persists.
fn coreml_cache_dir(cache_dir: Option<&Path>) -> Option<std::path::PathBuf> {
    let root = crate::model::manager::cache_root(cache_dir).ok()?;
    let dir = root.join("coreml-cache");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn execution_providers(device: Device, cache_dir: Option<&Path>) -> Vec<ExecutionProviderDispatch> {
    let coreml = || {
        let mut ep = CoreML::default().with_static_input_shapes(true);
        if let Some(dir) = coreml_cache_dir(cache_dir) {
            ep = ep.with_model_cache_dir(dir.to_string_lossy().into_owned());
        }
        ep.build()
    };
    match device {
        Device::Cpu => vec![CPU::default().build()],
        Device::Coreml => vec![coreml()],
        Device::Auto if coreml_capable_target() => {
            vec![coreml(), CPU::default().build()]
        }
        Device::Auto => vec![CPU::default().build()],
    }
}

/// Whether the CoreML compiled-model cache already has *any* content. Used
/// as a coarse but accurate-in-practice signal for "has a compile already
/// happened on this machine" — after the very first successful run, every
/// session `para` builds (preprocessor/encoder/decoder-joint) has already
/// been compiled and cached, so any content at all means later runs won't
/// need to recompile.
fn coreml_cache_has_content(cache_dir: Option<&Path>) -> bool {
    coreml_cache_dir(cache_dir)
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .is_some_and(|mut entries| entries.next().is_some())
}

/// Emits the CoreML first-compile stderr notice at most once per process,
/// before the session is built, so a user doesn't mistake a silent 30-60s
/// compile for a hang. Skipped entirely once the cache already has content
/// (`coreml_cache_has_content`) — otherwise this printed "first run only" on
/// *every* run regardless of whether a compile was actually about to happen,
/// which is exactly the misleading behavior a user reported.
fn maybe_emit_coreml_notice(device: Device, cache_dir: Option<&Path>) {
    let will_try_coreml = matches!(device, Device::Coreml)
        || matches!(device, Device::Auto if coreml_capable_target());
    if will_try_coreml
        && !coreml_cache_has_content(cache_dir)
        && COREML_NOTICE_SHOWN
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        eprintln!(
            "Note: compiling model for Apple Neural Engine (first run only — may take up to a minute)"
        );
    }
}

/// Builds an ONNX Runtime session from a file on disk (used for the
/// downloaded encoder/decoder-joint graphs).
pub fn build_session_from_file(
    path: &Path,
    device: Device,
    cache_dir: Option<&Path>,
) -> anyhow::Result<Session> {
    maybe_emit_coreml_notice(device, cache_dir);
    Session::builder()
        .context("failed to create ONNX Runtime session builder")?
        .with_execution_providers(execution_providers(device, cache_dir))
        .map_err(|e| anyhow::anyhow!("failed to configure execution providers: {e}"))?
        .commit_from_file(path)
        .with_context(|| format!("failed to load ONNX model: {}", path.display()))
}

/// Builds an ONNX Runtime session from an in-memory byte slice (used for the
/// vendored preprocessor graphs — research.md §10; no file on disk at all).
pub fn build_session_from_memory(
    bytes: &[u8],
    device: Device,
    cache_dir: Option<&Path>,
) -> anyhow::Result<Session> {
    maybe_emit_coreml_notice(device, cache_dir);
    Session::builder()
        .context("failed to create ONNX Runtime session builder")?
        .with_execution_providers(execution_providers(device, cache_dir))
        .map_err(|e| anyhow::anyhow!("failed to configure execution providers: {e}"))?
        .commit_from_memory(bytes)
        .context("failed to load vendored preprocessor model from memory")
}

/// One chunk's encoder output: a `(1, hidden_size, frames)` tensor flattened
/// row-major, plus its dimensions.
pub struct EncoderOutput {
    pub data: Vec<f32>,
    pub hidden_size: usize,
    pub frames: usize,
}

/// Splits `total_samples` into sequential, non-overlapping ranges of at most
/// `CHUNK_SECONDS` each (research.md §6). Returns a single range spanning the
/// whole input when it fits in one chunk — single-pass inputs are never
/// forced to chunk just to produce progress output (FR-023).
fn chunk_ranges(total_samples: usize) -> Vec<Range<usize>> {
    let chunk_len = (CHUNK_SECONDS * SAMPLE_RATE as f64) as usize;
    if total_samples <= chunk_len {
        // A single range representing the whole (unchunked) input, not a
        // sequence of per-sample ranges -- clippy's suggested rewrite would
        // change the meaning here.
        #[allow(clippy::single_range_in_vec_init)]
        return vec![0..total_samples];
    }
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < total_samples {
        let end = (start + chunk_len).min(total_samples);
        ranges.push(start..end);
        start = end;
    }
    ranges
}

/// Runs the encoder session on one chunk's mel features.
fn run_encoder(session: &mut Session, features: &Features) -> anyhow::Result<EncoderOutput> {
    let audio_signal = Tensor::from_array((
        [1usize, features.feature_size, features.frames],
        features.data.clone(),
    ))?;
    let length = Tensor::from_array(([1usize], vec![features.frames as i64]))?;
    let outputs = session.run(ort::inputs![
        "audio_signal" => audio_signal,
        "length" => length,
    ])?;
    let (shape, data) = outputs["outputs"].try_extract_tensor::<f32>()?;
    Ok(EncoderOutput {
        data: data.to_vec(),
        hidden_size: shape[1] as usize,
        frames: shape[2] as usize,
    })
}

/// Runs the full preprocessor-then-encoder pass over `samples`, splitting
/// into chunks per [`chunk_ranges`] and emitting `"transcribing chunk N of
/// M"` to stderr for each chunk when more than one is needed (FR-023).
pub fn encode_chunked(
    samples: &[f32],
    preprocessor: &mut mel::Preprocessor,
    encoder: &mut Session,
) -> anyhow::Result<Vec<EncoderOutput>> {
    let ranges = chunk_ranges(samples.len());
    let total = ranges.len();
    ranges
        .into_iter()
        .enumerate()
        .map(|(i, range)| {
            if total > 1 {
                eprintln!("transcribing chunk {} of {total}", i + 1);
            }
            let features = preprocessor.extract(&samples[range])?;
            run_encoder(encoder, &features)
        })
        .collect()
}

/// One chunk's CTC log-probability output: `(1, frames, vocab_size)`
/// flattened row-major — time-major, unlike [`EncoderOutput`]'s hidden-major
/// layout (verified separately against the real `parakeet-ctc-0.6b`
/// `model.onnx`, not assumed identical to the TDT encoder's convention).
pub struct CtcOutput {
    pub data: Vec<f32>,
    pub frames: usize,
    pub vocab_size: usize,
}

/// Runs the CTC model (combined encoder+projection in one graph) on one
/// chunk's mel features.
fn run_ctc(session: &mut Session, features: &Features) -> anyhow::Result<CtcOutput> {
    let audio_signal = Tensor::from_array((
        [1usize, features.feature_size, features.frames],
        features.data.clone(),
    ))?;
    let length = Tensor::from_array(([1usize], vec![features.frames as i64]))?;
    let outputs = session.run(ort::inputs![
        "audio_signal" => audio_signal,
        "length" => length,
    ])?;
    let (shape, data) = outputs["logprobs"].try_extract_tensor::<f32>()?;
    Ok(CtcOutput {
        data: data.to_vec(),
        frames: shape[1] as usize,
        vocab_size: shape[2] as usize,
    })
}

/// CTC counterpart to [`encode_chunked`] — same chunking/progress-message
/// behavior, different per-chunk model call and output layout.
pub fn encode_chunked_ctc(
    samples: &[f32],
    preprocessor: &mut mel::Preprocessor,
    model: &mut Session,
) -> anyhow::Result<Vec<CtcOutput>> {
    let ranges = chunk_ranges(samples.len());
    let total = ranges.len();
    ranges
        .into_iter()
        .enumerate()
        .map(|(i, range)| {
            if total > 1 {
                eprintln!("transcribing chunk {} of {total}", i + 1);
            }
            let features = preprocessor.extract(&samples[range])?;
            run_ctc(model, &features)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_chunk_for_input_under_threshold() {
        let ranges = chunk_ranges(100 * SAMPLE_RATE);
        assert_eq!(ranges, vec![0..100 * SAMPLE_RATE]);
    }

    #[test]
    fn splits_into_multiple_chunks_over_threshold() {
        let total = (CHUNK_SECONDS as usize + 60) * SAMPLE_RATE;
        let ranges = chunk_ranges(total);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0], 0..(CHUNK_SECONDS as usize * SAMPLE_RATE));
        assert_eq!(ranges[1], (CHUNK_SECONDS as usize * SAMPLE_RATE)..total);
    }

    #[test]
    fn chunk_ranges_cover_input_with_no_gaps_or_overlap() {
        let total = (2.0 * CHUNK_SECONDS) as usize * SAMPLE_RATE + 12345;
        let ranges = chunk_ranges(total);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges.last().unwrap().end, total);
        for pair in ranges.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
    }

    #[test]
    fn coreml_cache_is_empty_before_any_compile() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!coreml_cache_has_content(Some(dir.path())));
    }

    #[test]
    fn coreml_cache_has_content_once_something_is_cached() {
        let dir = tempfile::tempdir().unwrap();
        let coreml_dir = coreml_cache_dir(Some(dir.path())).unwrap();
        std::fs::write(coreml_dir.join("some-compiled-model"), b"").unwrap();
        assert!(coreml_cache_has_content(Some(dir.path())));
    }

    #[test]
    fn exact_multiple_of_chunk_length_does_not_add_empty_trailing_chunk() {
        let total = CHUNK_SECONDS as usize * SAMPLE_RATE;
        let ranges = chunk_ranges(total);
        assert_eq!(ranges, vec![0..total]);
    }

    /// Exercises `encode_chunked` against the real, live-downloaded
    /// `parakeet-tdt-0.6b-v3` encoder in the default cache location. Skipped
    /// (not failed) when the model isn't cached, since this repo's own
    /// automated checks don't download multi-GB model files -- run
    /// explicitly with `cargo test -- --ignored` after a real `ensure_cached`
    /// (or manual download) has populated the cache.
    #[test]
    #[ignore = "requires the real ~2.5GB parakeet-tdt-0.6b-v3 model in the local cache"]
    fn encode_chunked_runs_against_the_real_encoder() {
        use crate::inference::ModelKind;

        let Some(cache_dir) = dirs::cache_dir() else {
            return;
        };
        let model_dir = cache_dir.join("para/models/parakeet-tdt-0.6b-v3");
        let encoder_path = model_dir.join("encoder-model.onnx");
        if !encoder_path.exists() {
            return;
        }

        let mut preprocessor = mel::Preprocessor::load(ModelKind::Tdt, Device::Cpu, None).unwrap();
        let mut encoder = build_session_from_file(&encoder_path, Device::Cpu, None).unwrap();

        let samples = vec![0.0f32; 2 * SAMPLE_RATE];
        let outputs = encode_chunked(&samples, &mut preprocessor, &mut encoder).unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].hidden_size, 1024);
        assert!(outputs[0].frames > 0);
    }
}
