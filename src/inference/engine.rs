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

/// Threshold below which an input is processed in a single pass, never
/// chunked (001-media-transcription research.md §6). Set well under the
/// empirically-found ~400s hard cutoff (a fixed-size relative positional
/// encoding buffer inside the TDT encoder graph), with margin for models that
/// haven't been checked the same way yet. This is a separate concern from
/// `CHUNK_SECONDS` below (003-reduce-memory-footprint data-model.md): this
/// value only decides *whether* chunking happens, not how large each chunk
/// is once it does.
const SINGLE_PASS_THRESHOLD_SECONDS: f64 = 300.0;

/// Size of each chunk once an input exceeds `SINGLE_PASS_THRESHOLD_SECONDS`
/// (003-reduce-memory-footprint research.md's "actual fix"). ONNX Runtime's
/// per-call working set for the encoder graph scales with this window, not
/// with total recording length — bounding it here keeps peak memory flat
/// regardless of how long the recording is, at the cost of more, smaller
/// `session.run()` calls for long inputs. 30s (not research.md Phase 0's
/// originally-tested 15s) is the real, tuned value: real-measurement
/// content-parity testing on a long recording found 15s made the TDT
/// transducer drop whole phrases near chunk boundaries even with overlap
/// (`CHUNK_OVERLAP_SECONDS`); 30s keeps differences to cosmetic
/// wording/punctuation only, matching the existing 300s chunking's own
/// baseline quality, while still cutting peak memory roughly in half.
const CHUNK_SECONDS: f64 = 30.0;

/// Extra look-back *and* look-ahead audio (seconds) fed to the TDT encoder
/// around each chunk's own range, so the encoder has acoustic context on
/// both sides of a chunk boundary (003-reduce-memory-footprint research.md's
/// follow-up finding: with zero overlap, small chunks made the transducer
/// emit blank — dropping whole phrases — near boundaries, because the
/// Conformer encoder had no context there; a left-only-context version still
/// lost content, since the encoder needs lookahead too, not just lookback;
/// the autoregressive decoder state itself is already threaded correctly
/// across chunks and was never the problem). Only the frames corresponding
/// to the chunk's own original, non-overlapping range are decoded
/// (`trim_frames`) — the overlap on both sides is encoder-context only and
/// is never decoded by any chunk, so nothing is ever decoded twice. CTC does
/// not need this — measured separately to tolerate small chunks with no
/// overlap at all, since its per-frame classification doesn't depend on
/// carried decoder state the way the transducer's blank/emit decision does.
const CHUNK_OVERLAP_SECONDS: f64 = 5.0;

/// The TDT encoder's measured output frame rate — encoder frames per second
/// of input audio (real, live measurement against `parakeet-tdt-0.6b-v3`;
/// consistently ~12.5 across chunk durations from 10s to 100s, and matching
/// FluidAudio's independently-published CoreML encoder's own 188 frames per
/// 15s window — both fixed-size Conformer subsampling by the same factor).
/// Used only to convert `CHUNK_OVERLAP_SECONDS` into a frame count to trim.
const ENCODER_FRAMES_PER_SECOND: f64 = 12.5;

/// Drops `lead` frames from the start and `tail` frames from the end of an
/// encoder output's `[1, hidden_size, frames]` (hidden-major) buffer. Used to
/// discard the look-back/look-ahead overlap regions' frames — after they've
/// given the encoder context on both sides of a chunk boundary — before
/// decoding, leaving only the frames covering the chunk's own original range.
fn trim_frames(output: &mut EncoderOutput, lead: usize, tail: usize) {
    if lead == 0 && tail == 0 {
        return;
    }
    let new_frames = output.frames - lead - tail;
    let mut new_data = Vec::with_capacity(output.hidden_size * new_frames);
    for h in 0..output.hidden_size {
        let start = h * output.frames + lead;
        let end = start + new_frames;
        new_data.extend_from_slice(&output.data[start..end]);
    }
    output.data = new_data;
    output.frames = new_frames;
}

static COREML_NOTICE_SHOWN: AtomicBool = AtomicBool::new(false);

/// Whether this target could plausibly run the CoreML execution provider —
/// only meaningful for an explicit `--device coreml` request now (research.md
/// §15: CoreML is no longer attempted by `Auto`, having measured zero benefit
/// for this model family and an outright conflict with the optimized-graph
/// cache below).
fn coreml_capable_target() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

/// Directory CoreML caches its compiled models in, so the ~30-60s compile
/// (Apple Neural Engine) only happens once ever, not on every session load
/// (`ort`'s `CoreML::with_model_cache_dir`). Only reachable via explicit
/// `--device coreml` (research.md §15).
fn coreml_cache_dir(cache_dir: Option<&Path>) -> Option<std::path::PathBuf> {
    let root = crate::model::manager::cache_root(cache_dir).ok()?;
    let dir = root.join("coreml-cache");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// `Auto` and `Cpu` both run on CPU alone — CoreML measured zero speedup for
/// this model family and, worse, produces compiled nodes ORT can't serialize
/// alongside the optimized-graph cache below ("Unable to serialize model as
/// it contains compiled nodes"), so it's no longer the default (Constitution
/// Principle VI, amended). `Coreml` remains available as an explicit opt-in,
/// unchanged from before, for anyone who wants to try it on different
/// hardware or models — it does not participate in the optimized-graph
/// cache (research.md §15).
fn execution_providers(device: Device, cache_dir: Option<&Path>) -> Vec<ExecutionProviderDispatch> {
    match device {
        Device::Cpu | Device::Auto => vec![CPU::default().build()],
        Device::Coreml => {
            let mut ep = CoreML::default().with_static_input_shapes(true);
            if let Some(dir) = coreml_cache_dir(cache_dir) {
                ep = ep.with_model_cache_dir(dir.to_string_lossy().into_owned());
            }
            vec![ep.build(), CPU::default().build()]
        }
    }
}

/// Whether the CoreML compiled-model cache already has *any* content. Used
/// as a coarse but accurate-in-practice signal for "has a compile already
/// happened on this machine" — after the first successful `--device coreml`
/// run, every session built that way has already been compiled and cached.
fn coreml_cache_has_content(cache_dir: Option<&Path>) -> bool {
    coreml_cache_dir(cache_dir)
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .is_some_and(|mut entries| entries.next().is_some())
}

/// Emits the CoreML first-compile stderr notice at most once per process,
/// before the session is built, so a user doesn't mistake a silent 30-60s
/// compile for a hang. Only relevant for explicit `--device coreml`; skipped
/// entirely once the cache already has content (`coreml_cache_has_content`).
fn maybe_emit_coreml_notice(device: Device, cache_dir: Option<&Path>) {
    if matches!(device, Device::Coreml)
        && coreml_capable_target()
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
///
/// An ORT "optimized-graph" disk cache (`with_optimized_model_path` +
/// `GraphOptimizationLevel::Disable` on the cached reload) was tried here and
/// reverted: measured directly, loading the serialized "optimized" file was
/// *slower* than just re-optimizing the original file fresh each time
/// (~1.7-2.7s vs. ~1.4s for the encoder), not faster — the dominant per-load
/// cost isn't the optimization passes themselves (research.md §15).
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
/// `CHUNK_SECONDS` each, once the input exceeds `SINGLE_PASS_THRESHOLD_SECONDS`
/// (001-media-transcription research.md §6; 003-reduce-memory-footprint
/// data-model.md). Returns a single range spanning the whole input when it
/// fits within the single-pass threshold — single-pass inputs are never
/// forced to chunk just to produce progress output (FR-023) or to bound
/// memory they don't need bounding for.
fn chunk_ranges(total_samples: usize) -> Vec<Range<usize>> {
    let single_pass_len = (SINGLE_PASS_THRESHOLD_SECONDS * SAMPLE_RATE as f64) as usize;
    if total_samples <= single_pass_len {
        // A single range representing the whole (unchunked) input, not a
        // sequence of per-sample ranges -- clippy's suggested rewrite would
        // change the meaning here.
        #[allow(clippy::single_range_in_vec_init)]
        return vec![0..total_samples];
    }
    let chunk_len = (CHUNK_SECONDS * SAMPLE_RATE as f64) as usize;
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
/// into chunks per [`chunk_ranges`]. Reports half of each chunk's audio
/// duration to `progress` once that chunk's encode completes (spec
/// 002-transcription-progress) — the other half is credited once that
/// chunk's *decode* completes (`decoder::decode_tdt`/`decode_ctc`), which is
/// also where the non-interactive plain-text milestone (extending spec
/// 001's FR-023 line) is now emitted, since a chunk isn't actually done
/// being transcribed until both passes finish.
pub fn encode_chunked(
    samples: &[f32],
    preprocessor: &mut mel::Preprocessor,
    encoder: &mut Session,
    progress: &mut crate::progress::TranscriptionProgress,
) -> anyhow::Result<Vec<EncoderOutput>> {
    let ranges = chunk_ranges(samples.len());
    let overlap_samples = (CHUNK_OVERLAP_SECONDS * SAMPLE_RATE as f64) as usize;
    let total_samples = samples.len();
    ranges
        .into_iter()
        .map(|range| {
            let chunk_duration_secs = range.len() as f64 / SAMPLE_RATE as f64;
            let encode_start = range.start.saturating_sub(overlap_samples);
            let encode_end = (range.end + overlap_samples).min(total_samples);
            let lead_len = range.start - encode_start;
            let tail_len = encode_end - range.end;
            let features = preprocessor.extract(&samples[encode_start..encode_end])?;
            let mut output = run_encoder(encoder, &features)?;
            if lead_len > 0 || tail_len > 0 {
                let to_frames = |secs: f64| (secs * ENCODER_FRAMES_PER_SECOND).floor() as usize;
                let lead_trim = to_frames(lead_len as f64 / SAMPLE_RATE as f64);
                let tail_trim = to_frames(tail_len as f64 / SAMPLE_RATE as f64);
                // Never trim away the whole chunk even if rounding is generous.
                let max_trim = output.frames.saturating_sub(1);
                let (lead_trim, tail_trim) = if lead_trim + tail_trim > max_trim {
                    (
                        lead_trim.min(max_trim),
                        tail_trim.min(max_trim - lead_trim.min(max_trim)),
                    )
                } else {
                    (lead_trim, tail_trim)
                };
                trim_frames(&mut output, lead_trim, tail_trim);
            }
            progress.advance_encoded(chunk_duration_secs);
            Ok(output)
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

/// CTC counterpart to [`encode_chunked`] — same chunking/progress-reporting
/// behavior, different per-chunk model call and output layout.
pub fn encode_chunked_ctc(
    samples: &[f32],
    preprocessor: &mut mel::Preprocessor,
    model: &mut Session,
    progress: &mut crate::progress::TranscriptionProgress,
) -> anyhow::Result<Vec<CtcOutput>> {
    let ranges = chunk_ranges(samples.len());
    ranges
        .into_iter()
        .map(|range| {
            let chunk_duration_secs = range.len() as f64 / SAMPLE_RATE as f64;
            let features = preprocessor.extract(&samples[range])?;
            let output = run_ctc(model, &features)?;
            progress.advance_encoded(chunk_duration_secs);
            Ok(output)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_chunk_for_input_under_single_pass_threshold() {
        let ranges = chunk_ranges(100 * SAMPLE_RATE);
        assert_eq!(ranges, vec![0..100 * SAMPLE_RATE]);
    }

    /// Well over `CHUNK_SECONDS` (15s) but still under
    /// `SINGLE_PASS_THRESHOLD_SECONDS` (300s) — must not be chunked at all.
    /// Chunking only kicks in once the single-pass threshold is exceeded;
    /// `CHUNK_SECONDS` alone no longer gates that decision
    /// (003-reduce-memory-footprint data-model.md).
    #[test]
    fn single_chunk_when_over_chunk_seconds_but_under_single_pass_threshold() {
        let total = 200 * SAMPLE_RATE;
        let ranges = chunk_ranges(total);
        assert_eq!(ranges, vec![0..total]);
    }

    #[test]
    fn splits_into_chunk_seconds_windows_once_over_single_pass_threshold() {
        let total = (SINGLE_PASS_THRESHOLD_SECONDS as usize + 60) * SAMPLE_RATE;
        let ranges = chunk_ranges(total);
        let chunk_len = CHUNK_SECONDS as usize * SAMPLE_RATE;
        let expected_chunks = total.div_ceil(chunk_len);
        assert_eq!(ranges.len(), expected_chunks);
        assert_eq!(ranges[0], 0..chunk_len);
        assert!(
            ranges
                .iter()
                .take(ranges.len() - 1)
                .all(|r| r.len() == chunk_len)
        );
    }

    #[test]
    fn chunk_ranges_cover_input_with_no_gaps_or_overlap() {
        let total = (SINGLE_PASS_THRESHOLD_SECONDS as usize + 2 * CHUNK_SECONDS as usize)
            * SAMPLE_RATE
            + 12345;
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
        let chunk_len = CHUNK_SECONDS as usize * SAMPLE_RATE;
        let total = (SINGLE_PASS_THRESHOLD_SECONDS as usize + CHUNK_SECONDS as usize) * SAMPLE_RATE;
        assert_eq!(
            total % chunk_len,
            0,
            "test fixture must be an exact multiple"
        );
        let ranges = chunk_ranges(total);
        assert!(ranges.iter().all(|r| !r.is_empty()));
        assert_eq!(ranges.last().unwrap().end, total);
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
        let mut progress = crate::progress::TranscriptionProgress::new(true);
        let outputs =
            encode_chunked(&samples, &mut preprocessor, &mut encoder, &mut progress).unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].hidden_size, 1024);
        assert!(outputs[0].frames > 0);
    }
}
