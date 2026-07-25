# Phase 0 Research: Native CoreML Backend and Transcript Polish

## Amendment: superseded by a Swift-shim architecture (see §7)

Everything below through §6 was the original research and was acted on first: `objc2-core-ml` was
added, and a hand-rolled Rust reimplementation of FluidAudio's sliding-window/token-dedup/TDT-decode
logic was begun. Real implementation work (specs/004-native-coreml-backend/tasks.md's Foundational
phase) surfaced that this meant re-deriving genuinely hard, already-solved logic — RNNT joint-network
stepping, duration prediction, blank/emit arbitration, an 80ms first-frame bug FluidAudio needed a
dedicated PR to fix — the same class of bug this project had _already_ reproduced once from scratch
during 003-reduce-memory-footprint's chunk-size regression. Given this is a single-user personal
tool with no portability requirement and no objection to a Swift build-time dependency, continuing
down that path was reconsidered mid-implementation. §7 documents the real alternative that was
built instead, verified end-to-end, and shipped. §1-§6 remain below as an honest record of the
investigation — the CoreML-model-coverage finding (§1) and the constitution amendment (§5) are
still accurate and still apply; the `objc2-core-ml`/hand-rolled-chunking decision (§2-§4, §6) is
superseded.

## Original findings (§1-§6)

All findings below are from direct inspection of real, external source code and crates.io data —
Constitution Principle V — not assumed. Primary source: a local clone of FluidAudio
(github.com/FluidInference/FluidAudio, Apache License 2.0, `~/src/opensource/FluidAudio`), the
same open-source Swift library VoiceInk itself is built on (confirmed during
002-transcription-progress's planning). No FluidAudio _code_ is reused or vendored — para is Rust,
FluidAudio is Swift, so direct code reuse isn't possible either way — only its published model
files and documented design choices inform this Rust-native reimplementation.

## 1. CoreML model coverage for all three of para's models — CONFIRMED, no gap

`FluidAudio/Sources/FluidAudio/ModelNames.swift` lists real, published HuggingFace repos for every
model para currently offers via ONNX Runtime:

| para model                       | FluidAudio CoreML repo                       |
| -------------------------------- | -------------------------------------------- |
| `parakeet-tdt-0.6b-v3` (default) | `FluidInference/parakeet-tdt-0.6b-v3-coreml` |
| `parakeet-tdt-0.6b-v2`           | `FluidInference/parakeet-tdt-0.6b-v2-coreml` |
| `parakeet-ctc-0.6b`              | `FluidInference/parakeet-ctc-0.6b-coreml`    |

**This resolves spec.md's open Phase-0 question in the best possible direction**: FR-005's
ONNX-Runtime-fallback-for-uncovered-models clause is not needed for any of para's current three
models — all three get the native CoreML backend. The fallback clause stays in the spec as a
forward-looking guard (a future new model without a conversion), not because it's needed today.

## 2. Calling CoreML from Rust — `objc2-core-ml`

Evaluated three real options via crates.io:

| Crate                                 | Downloads              | Repo                           | Verdict                                                                                                                                                                 |
| ------------------------------------- | ---------------------- | ------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `coreml` (doom-fish/coreml-rs)        | 175                    | github.com/doom-fish/coreml-rs | Created 2026-05-16 (~2 months old at research time) — too new and unproven to trust for this project's correctness bar                                                  |
| `coreml-rs-fork` (swift-bridge based) | (not checked in depth) | —                              | Adds a Swift toolchain/build step into a pure-Rust build — conflicts with keeping this a single `cargo build`                                                           |
| `objc2-core-ml`                       | 131,349                | github.com/madsmtm/objc2       | **Chosen** — part of the `objc2` project (parent crate `objc2` itself: 84.5M downloads), maintained since 2024, safe Rust bindings directly to Apple's CoreML.framework |

**Decision**: `objc2-core-ml`. It's raw-but-safe bindings (`MLModel`, `MLDictionaryFeatureProvider`,
`MLMultiArray`, etc.) — not a ready-made "load model, run prediction" high-level API the way `ort`
is. This feature's implementation must build a thin Rust wrapper analogous to what `ort::Session`
currently provides for ONNX models, but for `.mlmodelc` bundles. This is real, scoped engineering
work, not a drop-in swap.

**Constitution fit**: CoreML.framework is a system framework already present on every macOS
install — linking against it via `objc2-core-ml` (a normal Rust dependency, compiled/statically
linked into the binary like every other crate) satisfies Principle VII (Minimal Runtime
Dependencies) exactly as `ort` does today. No new external install for the user.

## 3. Chunking design for a fixed ~15s encoder window

FluidAudio's own real constants (`Shared/ASRConstants.swift`), read directly:

- `maxDurationSeconds = 15.0`, `maxModelSamples = 240_000` (16kHz × 15s) — the CoreML encoder's
  fixed input window, confirming 003-reduce-memory-footprint's earlier finding (`mel: [1, 128,
1501]`) from direct model introspection.
- `melHopSize = 160` samples (10ms), `encoderSubsampling = 8` → `samplesPerEncoderFrame = 1280`
  (80ms/frame, 12.5 frames/sec) — **exactly matches** the `ENCODER_FRAMES_PER_SECOND = 12.5` value
  003 measured empirically against para's own ONNX encoder. Independent confirmation from a real,
  external source that this is a fixed architectural property of the model family, not an
  ONNX-Runtime-specific artifact.
- `standardOverlapFrames = 25` (2.0s) — FluidAudio's own chosen overlap, matching the 2.0s figure
  003 first tried (and found insufficient alone) before landing on wider testing.

`ASR/Parakeet/SlidingWindow/TDT/ChunkProcessor.swift` reveals a materially more refined design than
003's frame-trim approach:

- **Stride, not fixed non-overlapping ranges**: windows advance by `chunk size − overlap`
  (`strideSamples`), so consecutive windows genuinely overlap in the source-audio timeline, rather
  than 003's design of non-overlapping "core" ranges with symmetric encode-only padding.
- **An 80ms mel-context prepend** (`melContextSamples`, one full encoder frame) specifically to fix
  a "blank first frames" bug their own issue tracker documents (PR #264) — the encoder's
  convolutions need a small amount of left context even within an otherwise-full window.
- **End-aligned final window** (`lastChunkWarmupSamples`): the last chunk is shifted backward to end
  at the last real speech, not EOF, because a window trailing into dead silence decodes
  degenerately — filling with real preceding audio (decoded as a suppressed "warmup" prefix)
  instead of zero-padding.
- **Token-level deduplication across overlapping windows** (`ASR/Parakeet/TokenDeduplication/
SequenceMatcher.swift`), rather than 003's frame-count-based trimming — likely more robust to
  windows whose actual useful content boundary doesn't land exactly where a naive frame-count
  arithmetic would put it.

**Decision**: this feature reimplements a stride-based sliding window with a small mel-context
prepend and token-level dedup, informed by this real design, in Rust — not a direct port, but the
same proven shape rather than 003's simpler (and, for TDT, initially insufficient) frame-trim
scheme. `decoder.rs`'s existing `DecoderState` threading (already correct, per 003's diagnosis)
continues to carry the autoregressive state across windows underneath this.

## 4. Transcript polish: filler words, paragraphs, number/acronym normalization

### Filler-word removal (FR-006) — simple, self-contained

`FluidAudioCLI/Utils/TextNormalizer.swift` (a CLI utility, not the core library) does this with a
plain regex: `\b(hmm|mm|mhm|mmm|uh|um)\b` removed, plus a separate stutter-prefix pattern
(`\b[a-z]{1,2}-\s+`, e.g. "th- the" → "the"). This is deliberately low-complexity — no NLP model,
no dependency. **Decision**: implement the same shape directly in para's own output layer (a small,
tested Rust function operating on the decoded token/word sequence), no new dependency.

### Paragraph breaks (FR-007) — already have the data, just weren't using it

`decoder.rs`'s existing `group_into_segments`/`SEGMENT_GAP_SECONDS` (built for spec 001) already
splits tokens into `Segment`s wherever the gap between consecutive tokens' encoder frames exceeds
a threshold — precisely the pause information needed for paragraphing. `text.rs` currently discards
this by flattening every segment into one `Transcript.text` string. **Decision**: no new
segmentation logic needed; change `text.rs` (or wherever `Transcript.text` is assembled) to join
segments with a paragraph break instead of a space when the gap between them is large, using data
that already exists.

### Number/acronym normalization (FR-008) — scoped down from FluidAudio's full ITN, by design

FluidAudio's real, production ITN (`Sources/FluidAudio/ITN/TextNormalizer.swift`) is far larger in
scope than a simple regex: it dynamically loads (`dlopen`-style C function pointers) a **native
NeMo text-normalization library** (`nemoNormalize`/`nemoTnNormalize` etc.), plus uses Apple's
`NaturalLanguage` framework to disambiguate words like "period" (punctuation command vs. an
ordinary noun) by part-of-speech tagging. That native NeMo TN library is a real, separate runtime
dependency whose availability, packaging, and license terms for this specific library are not
established here — bundling it would conflict with Principle VII (Minimal Runtime Dependencies)
unless it can be statically linked cleanly, which hasn't been verified.

**Decision**: build a much smaller, self-contained, rule-based normalizer in Rust for the specific
patterns FR-008 actually asks for — common spoken numbers ("A one hundred" → "A100" where a
confident pattern match exists) and well-known acronyms already spelled letter-by-letter in the
model's own output ("S C P" → "SCP") — without attempting FluidAudio's full generalized ITN system.
This is a deliberate, smaller scope than FluidAudio's own feature, chosen specifically to avoid a
new native runtime dependency; ambiguous cases are left alone rather than guessed, per FR-008's own
"without inventing or guessing" requirement and Constitution Principle IV (Fail Loud, Fail Fast —
extended here to "don't silently guess wrong" for formatting, not just errors).

## 5. Constitution amendment required — completed

Principle VI ("Apple Silicon Is a First-Class Target") previously mandated CPU as the default
execution path because ONNX Runtime's CoreML _execution provider_ measured zero speedup
(research.md §15 of 001-media-transcription). That finding is about ORT's general-purpose session
with compute partially delegated to CoreML — architecturally distinct from the genuine native
CoreML backend this feature builds (confirmed in 003's own research: ORT's CoreML EP doesn't get
Apple's ahead-of-time whole-graph memory planning). The constitution has been amended (2.0.0 →
3.0.0, see `.specify/memory/constitution.md`'s Sync Impact Report) to make native CoreML the
default for any model with a real conversion, while keeping ORT's CoreML execution provider
non-default and unchanged, and ONNX Runtime CPU as the fallback for any model without a native
conversion. This was necessary before this plan's Constitution Check could pass.

## 6. What real measurement will still be required (Phase 1+ execution, not assumed here)

Per Constitution Principle V and this project's established practice, the actual memory/speed
numbers (SC-001, SC-002) can only be confirmed once the native backend is built and run against
real recordings — nothing here fabricates those figures in advance. FluidAudio's own published
numbers are about _their_ Swift application, not a guarantee for para's independent Rust
reimplementation using the same model files; the real verification work is `tasks.md`'s job.

## 7. What was actually built: a Swift shim linking FluidAudio directly

`fluidaudio-rs` (github.com/FluidInference/fluidaudio-rs, MIT, crates.io, actively maintained) was
found while reconsidering the `objc2-core-ml` path. It proves the real, working mechanism: a
`build.rs` that runs `swift build` against a small SPM package depending on FluidAudio directly,
producing a static library (`libFluidAudioBridge.a`) linked into the Rust binary via
`cargo:rustc-link-lib`, with `@_cdecl`-exported C functions called from Rust through raw
`extern "C"` declarations and an opaque pointer handle. This — not `objc2-core-ml` — is what
para adopted, with its own shim (`swift/Sources/ParaBridge`) rather than depending on
`fluidaudio-rs` itself, because that crate's own public API (`AsrResult { text, confidence,
duration, processing_time, rtfx }`) has no segment-level timestamps, which para's SRT/JSON output
requires. Checked directly: FluidAudio's own underlying Swift type, `ASRResult` (not
`fluidaudio-rs`'s simplified wrapper), already carries `tokenTimings: [TokenTiming]?` plus a
`buildWordTimings()` helper producing word-level `start`/`end` timestamps — `fluidaudio-rs` simply
didn't surface them. Writing an independent, narrow shim directly against FluidAudio's real API
(not through `fluidaudio-rs`) gets the timestamps para needs with a small amount of new Swift code,
not a reimplementation of anything hard.

**What `swift/Sources/ParaBridge/ParaBridge.swift` exposes**: `para_bridge_create`/`_destroy`,
`para_load_model(version, cpuOnly)` (loads TDT v2 or v3 via `AsrModels.downloadAndLoad` +
`AsrManager.loadModels`, FluidAudio's own real download-and-cache logic, into FluidAudio's own
default cache directory), `para_model_is_cached`/`para_refresh_model` (for `--list-models`/
`--refresh-model`), and `para_transcribe_file` (returns text plus parallel word/start/end arrays
from `buildWordTimings`, mirroring the array-of-timed-things C-ABI pattern `fluidaudio-rs` already
uses for its own diarization-segment results). A real, non-obvious fix was needed for
`para_bridge_last_error`'s string: Swift's `UnsafeMutablePointer.allocate`/`.deallocate` is not
guaranteed-interchangeable with `libc::free` from the Rust side, so a dedicated
`para_free_error_string` Swift function frees it correctly, matching the existing
`para_free_transcribe_result` pattern for the word arrays.

**A real linking bug found and fixed**: the first working build crashed at runtime —
`Library not loaded: @rpath/libswift_Concurrency.dylib`. Modern macOS's Swift runtime dylibs
(`libswiftCore`, `libswift_Concurrency`, ...) live only in the dyld shared cache, not as on-disk
files; `swiftc`-built binaries reference them by the literal absolute path
`/usr/lib/swift/lib*.dylib`, which dyld resolves specially even with nothing on disk there.
Cargo's own linker never adds that search path, so `@rpath` never resolved. Fixed with an explicit
`cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift` in `build.rs`.

**A real directory-handling inconsistency found in FluidAudio itself**: `AsrModels.download(to:)`
uses its `directory` parameter as the exact target directory, but `AsrModels.load(from:)` (called
right after, inside `downloadAndLoad(to:)`) derives its actual search path from
`directory.deletingLastPathComponent()` plus the repo's own folder name — a different
interpretation of the same parameter. Passing a custom `to:` directory produced no visible error
but silently fell back to FluidAudio's own default cache directory instead. Rather than work around
this upstream quirk, `para_load_model` deliberately always uses FluidAudio's own default location
(`to:` left `nil`) — real, simple, and avoids the inconsistency entirely. **Consequence**: para's
`--cache-dir`/`PARA_CACHE_DIR` no longer controls where these CoreML model files are cached; they
live wherever FluidAudio's own `AsrModels.defaultCacheDirectory(for:)` puts them
(`~/Library/Application Support/FluidAudio/Models/...`), the same location any other FluidAudio-based
app (including VoiceInk) would use.

**ONNX Runtime, CTC, and the `--device coreml` execution-provider path were removed entirely**
(user decision, given the native CoreML numbers below): `src/inference/engine.rs`, `mel.rs`,
`decoder.rs`'s token-decode logic, the `ort`/`reqwest`/`sha2`/`zip`/`thiserror`/`dirs` dependencies,
and `parakeet-ctc-0.6b` from the model registry are all gone. FluidAudio has no equivalent
standalone CTC-transcription public API this project found within its research budget (its CTC
code lives under a keyword-spotting/custom-vocabulary path, not a general `AsrManager`-equivalent),
so CTC support was dropped rather than kept via a second, now-orphaned ONNX-only path. Two models
remain: `parakeet-tdt-0.6b-v3` (default) and `parakeet-tdt-0.6b-v2`, both through the same native
CoreML `AsrManager` path FluidAudio itself uses.

### Real, measured results (not projected)

Verified end-to-end via the actual `para` CLI binary, on the same long recording and original
reported file used throughout specs 002-003, and cross-checked word-for-word against the real
VoiceInk reference transcript:

| Metric                                         | ONNX Runtime + 003's chunk-size fix | Native CoreML (this feature)                            |
| ---------------------------------------------- | ----------------------------------- | ------------------------------------------------------- |
| Peak memory, long recording (~25.7 min)        | ~2.7GB                              | **211-214MB**                                           |
| Wall-clock time, same recording                | 3m28s                               | **6.3 seconds** (~33x faster)                           |
| Word count vs. VoiceInk reference (3808 words) | 3907 (with cosmetic diffs)          | 3787-3787, closely matching, no dropped content         |
| Filler words in output                         | Present (um/uh kept)                | Zero (removed)                                          |
| Paragraph structure                            | None (one block)                    | Real, pause-based paragraphs (23 on the long recording) |

This exceeds every success criterion in spec.md (SC-001's ≥70% memory reduction, SC-002's ≥50%
speed reduction) by a wide margin, and independently exceeds VoiceInk's own reported ~500MB
incremental figure — on a cold start, not a warm one.
