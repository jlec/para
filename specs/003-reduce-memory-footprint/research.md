# Phase 0 Research: Reduce Transcription Memory Footprint

All findings below are from direct, live measurement against real builds of `para` (peak RSS via
`ps` polling at 0.3-0.5s intervals), not assumption — Constitution Principle V. Every lever tested
was implemented as temporary, env-var-gated diagnostic code in `src/inference/engine.rs`, measured,
then reverted; none of these experimental toggles remain in the codebase.

## Baseline: growth is real but not simply linear with chunk count

Measured on `parakeet-ctc-0.6b` against pure-silence WAV files at controlled durations (isolating
chunk count from real-speech confounds), `--no-progress`, `--device cpu`:

| Duration | Chunks | Peak RSS |
|---|---|---|
| 250s | 1 | 2.90 GB |
| 350s | 2 | 3.26 GB |
| 650s | 3 | 5.19 GB (also independently re-measured at 4.15 GB in a later run — see run-to-run variance note below) |
| 950s | 4 | 5.36 GB |

Also reproduced directly on the user's own reported file (`07-01_Meeting_...mp3`, ~25 minutes,
default `parakeet-tdt-0.6b-v3` model, default settings): **5.79GB** peak, exit code 0, real
transcript produced — closely matching the user's own "above 6GB" observation.

**Run-to-run variance is real and substantial** — the same 650s file measured 5.19GB in one run and
4.15GB in another (baseline, no changes). Single-run comparisons below should be read as directional
signals, not precise percentages; the largest, clearest effects are the ones treated as real
findings.

## Four levers tested, all ruled out as the fix

### 1. ONNX Runtime's CPU arena allocator (`CPU::default().with_arena_allocator(bool)`)

Tested on the 650s/3-chunk case: disabling the arena **increased** peak memory (5.28GB vs. a
4.15GB baseline measured in the same batch) — the opposite of the hypothesis that the arena's
grow-and-retain behavior was the cause. Plain malloc/free per allocation is evidently worse here,
not better.

### 2. ONNX Runtime's memory-pattern optimization (`SessionBuilder::with_memory_pattern(bool)`)

Same test: disabling it produced 4.20GB vs. the 4.15GB baseline — within run-to-run noise, no real
effect either way.

### 3. Model weight precision (int8 vs. fp32 encoder)

The one result expected to matter most going in, and the most surprising negative finding: swapping
the fp32 encoder (2.4GB on disk) for the real int8-quantized encoder from the same trusted source
(`istupakov/parakeet-tdt-0.6b-v3-onnx`'s `encoder-model.int8.onnx`, 652MB on disk — a ~3.7x smaller
file) changed peak memory from 2.18GB to 2.12GB on a short single-chunk clip. A ~3% difference,
within noise. **This rules out static weight storage as the dominant driver of peak memory** — the
bulk of the footprint is not "the model weights sitting in RAM."

### 4. Execution provider and threading

- `--device coreml` (ORT's CoreML execution provider, already supported as an explicit opt-in) on
  the same short clip: 2.31GB, slightly *worse* than CPU's 2.19-2.39GB range. This still goes
  through ONNX Runtime's general session/graph/memory infrastructure — CoreML only takes over
  compute for statically-shaped subgraph portions (research.md §13 of `001-media-transcription`),
  so it doesn't get the memory benefits of a purpose-built native pipeline.
- Intra-op thread count forced to 1 (`SessionBuilder::with_intra_threads(1)`): 2.19GB vs. a
  2.39GB baseline — a real but modest ~8% reduction, plausibly from fewer per-thread workspace
  buffers.
- Sequential vs. parallel graph execution (`SessionBuilder::with_parallel_execution(false)`): no
  meaningful difference (2.42GB vs. 2.39GB baseline).
- The best *combination* found across all four experiments (arena disabled + memory pattern
  disabled together, tested on the 650s case) gave 3.67GB vs. a 4.15GB baseline — a real ~12%
  reduction, the single largest effect measured, though still far short of resolving the underlying
  problem.

## What this rules in

None of the four levers tested — separately or combined — comes close to explaining or fixing a
multi-gigabyte footprint, or approaching VoiceInk's ~200MB *incremental* figure. Combined with
finding 3 (weight precision barely matters), this points away from "one wrong config flag" and
toward something more structural: **ONNX Runtime's general-purpose CPU session for a ~600M-parameter
Conformer encoder appears to have a large, largely fixed memory floor that isn't controllable
through the configuration surface `ort`'s Rust API exposes** — most plausibly dominated by
intermediate activation/workspace tensors scaled by the encoder's depth and sequence length, not by
weight storage, allocator strategy, or thread count.

This is architecturally consistent with why VoiceInk looks so different: it doesn't run ONNX
Runtime at all. It runs Parakeet through **native CoreML** (`.mlmodelc`, via FluidAudio — confirmed
from VoiceInk's own source, researched during `002-transcription-progress`'s planning), where
Apple's own compiler performs ahead-of-time memory planning and fusion across the *entire* compiled
graph. `ort`'s CoreML execution provider (tested above) does not get this benefit — it's still an
ONNX Runtime session underneath, just with compute for some nodes delegated to CoreML.

## What a real fix would require

Matching VoiceInk's footprint would require **bypassing ONNX Runtime entirely** for the model
inference and calling Apple's CoreML framework directly (`.mlpackage`/`.mlmodelc`, real conversion
via `coremltools`, a new inference backend) — the same trade this project explicitly declined
before, for different reasons (research.md §13-15 of `001-media-transcription`: CoreML measured zero
*speed* benefit through ORT's wrapper, so it was dropped from the default path). This is a
substantially larger undertaking than a memory-footprint feature should absorb on its own:

- A new, CoreML-only code path — Linux and Intel Mac (darwin/amd64) would need to keep the existing
  ONNX Runtime CPU path, meaning two inference backends to maintain, not one.
- Real model conversion and verification work (`coremltools`, not just downloading an existing file
  the way the int8 ONNX variant was).
- A materially larger scope than this feature's spec anticipated.

## The actual fix: bound the per-call processing window (found via FluidAudio comparison)

Before committing to the native-CoreML path, its actual memory characteristics were verified
directly rather than assumed: FluidAudio's real, published CoreML model
(`FluidInference/parakeet-tdt-0.6b-v3-coreml` on HuggingFace) was downloaded and introspected with a
small native Swift/CoreML program (`MLModel(contentsOf:)` + `modelDescription`), not just read about.
Its encoder's input signature is `mel: [1, 128, 1501]` — a **fixed 15-second window**, not a dynamic
chunk of up to 300s the way `encode_chunked`/`encode_chunked_ctc` currently work. That's the more
likely real explanation for VoiceInk's flat memory profile: small, fixed-size processing windows
bound peak per-call memory, independent of "CoreML vs. ONNX Runtime" as such.

This was tested directly, entirely within the existing ONNX Runtime architecture, by temporarily
lowering `CHUNK_SECONDS` from 300 to 15 (`src/inference/engine.rs`) and re-running the same
measurements used throughout this research:

| Case | 300s chunks (current) | 15s chunks |
|---|---|---|
| 650s file, CTC (the case with the clearest growth signal above) | 4.15-5.19 GB | **2.38 GB** (~50% lower; memory plateaued and stayed flat from ~30s into the run onward, rather than climbing throughout) |
| 62s file, TDT default model (previously single-pass, under the old 300s threshold) | ~2.2 GB | 2.52 GB (slightly *worse* — forcing chunking on a file too short to need it adds per-call overhead with no growth problem to offset) |

**This is the real fix, and it requires no architecture change.** It directly explains the original
growth-vs-chunk-count data (§ above): more/larger chunks meant more accumulated per-call working set
before anything was released; bounding each call's window keeps that working set — and therefore
peak RSS — flat regardless of total recording length. The nuance from the 62s case means the fix
isn't "always use exactly 15s" but rather "tune the threshold so long recordings get bounded
per-call windows without penalizing short ones" — a tuning question for Phase 1 design, not a reason
to abandon the approach.

**Decision for this feature**: implement chunk-size tuning as the fix — a ~50% real, measured
reduction on a long recording, achieved with a small, low-risk change to existing chunking logic, no
new dependency, no platform restriction. The native-CoreML path remains real and is now backed by
concrete numbers (FluidAudio's actual model files, not a guess) rather than dismissed, but is
deferred to a separate, future feature given its substantially larger scope — see spec.md's
Clarifications for the full reasoning, including the user's confirmation that `para`'s target
platform is Apple Silicon macOS only, which removes the dual-backend-maintenance cost that made the
rewrite look worse and makes it a stronger future candidate than the SC-002 discussion first
assumed.

## Implementation-time correction: 15s broke TDT correctness — the real fix needed overlap

Phase 1 implementation (tasks.md T007-T010) found that naively shipping the 15s value tested above
caused a real regression this research hadn't caught: on the full long recording, transcribing with
15s chunks and the default **TDT** model dropped substantial real content near chunk boundaries — one
instance lost 194 consecutive words of real dialogue, not just cosmetic rewording. The original 15s
finding above was measured on **CTC** only; CTC tolerates arbitrarily small chunks cleanly (verified
directly: 15s/60s/300s CTC transcripts on the same file differ only in minor spelling/wording, never
in dropped content), but TDT does not.

Root cause, verified by reading `src/inference/decoder.rs` rather than assumed: the TDT decoder's
autoregressive state (LSTM hidden state + previous token, `DecoderState`) is already threaded
correctly across chunk boundaries — that was never the bug. The actual cause is upstream, in the
encoder: each chunk's encoder pass ran on exactly that chunk's audio with zero surrounding context, so
whenever speech straddled a cut point, the encoder had no acoustic evidence there, and the transducer
tended to emit blank (silently drop content) rather than guess — a problem that gets worse, not better,
as chunks get smaller and boundaries more frequent.

**Fix**: extend each chunk's encoder input by a fixed overlap on *both* sides (`CHUNK_OVERLAP_SECONDS`)
before running the encoder, then trim exactly the overlap's worth of frames off both ends of the
encoder output before decoding (`trim_frames` in `engine.rs`) — so the encoder gets real context at
every boundary, but no audio is ever decoded twice. A left-context-only version was tried first and
was insufficient (still dropped a 32-194 word block depending on chunk size); both-sides context was
needed. This required measuring the encoder's real output frame rate (`ENCODER_FRAMES_PER_SECOND`,
~12.5 frames/sec, confirmed stable from 10s to 100s chunk durations, and matching FluidAudio's own
published CoreML encoder's 188 frames per 15s window almost exactly) to convert an overlap duration
into a frame-trim count. CTC does not use this mechanism — it doesn't need it.

**Final retuned values** (superseding the 15s figure above): `CHUNK_SECONDS = 30.0`,
`CHUNK_OVERLAP_SECONDS = 5.0`. Real-measurement results on the original reported file and a
same-source long recording:

| Metric | Pre-fix (300s) | Post-fix (30s + 5s overlap) |
|---|---|---|
| Peak memory, original reported file (mp3) | 5.79GB (baseline) | 2.570GB (**50.0% reduction**) |
| Peak memory, short clip (2 min) vs. long recording (~25.7 min) | n/a | 2.48GB vs. 2.75GB — close, flat |
| Transcript content, long recording | (reference) | Cosmetic differences only (max 8 consecutive words), no dropped clauses |
| Wall-clock time, long recording | 4m38s | 3m28s — **faster**, not slower |
| Peak memory, 1-hour synthetic recording | (untested at this length pre-fix) | 2.82GB — still flat |

The speed result was unexpected in the opposite direction from the risk flagged in plan.md
(more, smaller `session.run()` calls could have added overhead) — smaller per-call working sets
apparently reduce memory-pressure overhead enough to outweigh the extra call count on this machine.
