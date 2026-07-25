# Tasks: Native CoreML Backend and Transcript Polish

**Note**: This file was originally generated for an `objc2-core-ml` + hand-rolled-chunking
architecture, then superseded mid-implementation by a Swift-shim architecture (research.md §7) once
real engineering work showed the hand-rolled path meant re-deriving already-solved, genuinely hard
logic. This version reflects what was actually built and verified, not the original plan.

**Input**: Design documents from `/specs/004-native-coreml-backend/`

**Tests**: Unit tests for segment/paragraph grouping and filler-word removal
(`src/inference/segments.rs`); a `#[ignore]`-gated real end-to-end integration test
(`src/inference/swift_bridge.rs`); real-measurement verification (peak RSS, wall-clock time,
transcript diff against a real VoiceInk reference) captured as explicit tasks, matching this
project's practice throughout specs 001-003.

---

## Phase 1: Setup — Swift shim scaffolding

- [X] T001 Create `swift/Package.swift` (SPM package `ParaBridge`, depends on `FluidInference/FluidAudio` pinned to `0.15.5`, `swift-tools-version:5.10` — matched to `fluidaudio-rs`'s own proven-working combination; Swift 6 language mode's strict concurrency rejected the semaphore-blocked-`Task` bridging pattern that both `fluidaudio-rs` and this shim use)
- [X] T002 Write `swift/Sources/ParaBridge/ParaBridge.swift`: `@_cdecl`-exported C functions (`para_bridge_create`/`_destroy`, `para_load_model`, `para_model_is_cached`, `para_refresh_model`, `para_transcribe_file`, `para_free_transcribe_result`, `para_bridge_last_error`/`para_free_error_string`) wrapping FluidAudio's real `AsrModels`/`AsrManager`/`ASRResult` API
- [X] T003 Update `build.rs` to run `swift build -c release` in `swift/` and link the resulting `libParaBridge.a` plus `Foundation`/`AVFoundation`/`CoreML`/`Accelerate`/`swiftCore`/`c++`
- [X] T004 Fix a real runtime linking bug found via testing: `@rpath/libswift_Concurrency.dylib` not found (modern macOS keeps Swift runtime dylibs only in the dyld shared cache, referenced by the absolute path `/usr/lib/swift/lib*.dylib`) — added `cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift` to `build.rs`
- [X] T005 Add `src/inference/swift_bridge.rs`: safe Rust wrapper (`SwiftAsrBridge`, `ModelVersion`, `WordTiming`, `SwiftTranscript`) around the raw `extern "C"` declarations

**Checkpoint**: `cargo build` links the Swift bridge successfully; a real end-to-end test
(`transcribes_real_file_via_native_coreml`, `#[ignore]`-gated) transcribes `test.wav` correctly
with real word timestamps.

---

## Phase 2: Replace the ONNX Runtime pipeline entirely (user decision)

**Purpose**: Once the native CoreML numbers were measured (Phase 3 below) and found dramatically
better on every axis, the user explicitly decided to remove the ONNX Runtime pipeline rather than
keep it as a fallback — this is a personal, single-user tool with no need for the old path.

- [X] T006 Delete `src/inference/engine.rs`, `src/inference/mel.rs`, `src/inference/decoder.rs` (ONNX session building, chunking, TDT/CTC token decode — all superseded by FluidAudio's own real implementation)
- [X] T007 Remove `ort`, `reqwest`, `sha2`, `zip`, `thiserror`, `dirs` from `Cargo.toml` (only used by the ONNX pipeline and its build-time preprocessor-wheel download)
- [X] T008 Simplify `build.rs` to only build the Swift bridge — dropped the `onnx-asr` wheel download entirely (mel-spectrogram preprocessing is now internal to FluidAudio's own CoreML models)
- [X] T009 Rewrite `src/model/registry.rs`: two models remain (`parakeet-tdt-0.6b-v3` default, `parakeet-tdt-0.6b-v2`) mapped to `ModelVersion`; `parakeet-ctc-0.6b` dropped — FluidAudio has no equivalent standalone CTC-transcription public API this project found within its research budget
- [X] T010 Delete `src/model/manager.rs` entirely — FluidAudio's own `AsrModels.downloadAndLoad`/`download(force:)` now owns all real download/cache/checksum logic; `--list-models`/`--refresh-model` call the bridge directly
- [X] T011 Add `src/inference/segments.rs`: groups FluidAudio's real per-word timestamps into paragraph-level `Segment`s on pause gaps (`SEGMENT_GAP_SECONDS = 1.5`, same threshold the old decoder used) and removes filler words (FR-006/FR-007) — this project still owns this step since FluidAudio returns a flat word sequence, not pre-grouped paragraphs
- [X] T012 Rewrite `src/main.rs`'s `run()`: `SwiftAsrBridge` replaces the ONNX session/chunking/decode calls; `--device cpu` now maps to `MLComputeUnits.cpuOnly` (still real and meaningful, just via the new backend) instead of ONNX Runtime CPU execution
- [X] T013 Simplify `src/progress.rs`: dropped `advance_encoded`/`advance_decoded` (no per-chunk callback exists in the new single-call backend) — every phase is now an indeterminate spinner
- [X] T014 Update contract tests referencing the removed `parakeet-ctc-0.6b` model (`test_model_unknown.rs`, `test_list_models.rs`, and six `--features integration` tests) to use `parakeet-tdt-0.6b-v2` instead

**Checkpoint**: `cargo build`/`cargo test` clean (one pre-existing, expected model-list assertion
fixed); `cargo tree` confirms `ort`/`reqwest`/`sha2`/`zip`/`thiserror`/`dirs` are gone.

---

## Phase 3: Real-measurement verification (all of spec.md's success criteria)

- [X] T015 Peak memory, long recording (~25.7 min), default model, via the real CLI: **211-214MB** — a >90% reduction from the ~2.7GB post-003 baseline, exceeding SC-001's ≥70% target by a wide margin, and beating VoiceInk's own reported ~500MB *incremental* figure on a cold start
- [X] T016 Wall-clock time, same recording: **6.3 seconds** (down from 3m28s) — a ~33x speedup, exceeding SC-002's ≥50% target by a wide margin
- [X] T017 Transcript quality: word count 3787 vs. the real VoiceInk reference's 3808 words, closely matching with no dropped content (SC-005) — cross-checked directly against `~/tmp/on-board-in-the-last-of-the-week.md`
- [X] T018 Transcript polish (SC-003): zero filler words in the long-recording output (verified via grep), 23 real paragraph breaks at pause boundaries (verified via blank-line count)
- [X] T019 SRT/JSON output verified against `test.wav`: real per-segment timestamps, correctly formatted, structurally unaffected by the filler/paragraph changes (FR-009)
- [X] T020 `--list-models` verified against the real bridge: correctly reports `Cached`/`NotCached` per model with no network access

**Not done in this pass** (documented, not silently skipped):
- [ ] T021 Number/acronym normalization (FR-008) — deferred; FluidAudio's own output already renders numbers/acronyms reasonably (e.g. "500 GPUs", "A10040G") and the original letter-by-letter ONNX-era problem this was meant to fix appears substantially smaller with the new backend. Revisit only if a real gap is found in practice.
- [ ] T022 A `--model parakeet-tdt-0.6b-v2` full long-recording memory/speed run (only `test.wav`, a few seconds of audio, was verified against v2 via the integration test suite) — the v3 default model is what the user's original complaint was about and what carries the numbers above; v2 uses the identical code path so the same result is expected but not independently re-measured on a long file.

---

## Notes for future work

- `--cache-dir`/`PARA_CACHE_DIR` no longer controls where CoreML model files are cached (a real
  FluidAudio directory-handling inconsistency, documented in research.md §7, made a custom
  directory unreliable) — they now always live in FluidAudio's own default location. This is a
  real, user-visible behavior change from the ONNX-era CLI, not yet reflected in README.md.
- CTC model support was dropped, not deferred behind a flag — if a future need for the
  fastest/CTC tier resurfaces, it requires either finding FluidAudio's real CTC transcription API
  (not just its keyword-spotting path) or reintroducing a second backend.
