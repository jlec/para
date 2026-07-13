# Implementation Plan: Local Audio & Video Transcription

**Branch**: `001-media-transcription` | **Date**: 2026-07-09 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-media-transcription/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command. See `.specify/templates/plan-template.md` for the execution workflow.

## Summary

para is a single-invocation Rust CLI that transcribes a local audio or video file (or piped bytes) to text using an on-device ONNX speech model, entirely offline once the model is cached. The user selects one of three models trading speed for accuracy (default: NVIDIA Parakeet TDT 0.6B v3, multilingual; also v2 English-only TDT and a CTC English-only fast tier), and one of three output forms (plain text, JSON with segment timestamps, SRT subtitles). ffmpeg handles format normalization (the only dependency the user installs manually); the ONNX Runtime is fetched automatically at build time via the `ort` crate. Per model, inference chains up to 3 ONNX sessions — a bundled preprocessor graph (mel-feature extraction), an encoder, and (for TDT models) a decoder-joint network — with token-to-text decoding via a plain `vocab.txt` lookup rather than a tokenizer library (research.md §10). Model listing and forced refresh (`--refresh-model`) are supported; standalone removal is not. Model downloads retry a bounded number of times with backoff before failing loud; long inputs that require chunked encoding emit minimal per-chunk progress to stderr.

## Technical Context

**Language/Version**: Rust, 2024 edition, MSRV 1.85 (minimum toolchain shipping the 2024 edition)

**Primary Dependencies**:

- `ort` (ONNX Runtime bindings; CoreML execution provider on Apple Silicon, CPU elsewhere) — resolved to `2.0.0-rc.12` via `cargo add`; runs up to 3 ONNX sessions per transcription (bundled mel preprocessor, encoder, and for TDT models a decoder-joint network). Built with the `download-binaries` feature alone (no `load-dynamic`) — a real prebuilt ONNX Runtime archive is downloaded and **statically linked** at build time, producing a genuine single-binary output with no co-located dylib to ship (research.md §11; corrected 2026-07-12 after the `load-dynamic` combination, never actually exercised until then, turned out to silently skip the build-time fetch and hang the process on first real session use instead of erroring)
- `clap` (CLI parsing, derive + env features)
- `reqwest` + `indicatif` (model download with progress, stderr-only)
- `anyhow` (error propagation to the top-level handler) + `thiserror` (typed internal errors in the model manager, needed to distinguish retryable vs. terminal download failures per FR-022)
- `tempfile`, `which`, `dirs` (stdin staging, ffmpeg discovery, cache path resolution)
- `sha2` (checksum verification — Constitution Engineering Standards: prefer well-maintained crates for anything correctness-sensitive, checksums explicitly named)

**Build dependencies** (`[build-dependencies]`, not shipped in the final binary): `reqwest`
(blocking), `sha2`, `zip` (deflate only) — used solely by `build.rs` to fetch and verify the
preprocessor graphs described below.

**No mel-DSP or tokenizer crate**: research.md §10 (found while implementing the Foundational
phase, superseding §4 and §5) established that neither is needed. Mel-spectrogram extraction runs
through one of two small, shared ONNX graphs (`nemo128.onnx` for TDT models, `nemo80.onnx` for
CTC) — not reliably hosted per-model on HuggingFace, so `build.rs` downloads and checksum-verifies
them from the real `onnx-asr` PyPI wheel at build time and writes them to `OUT_DIR`, where
`src/inference/mel.rs` embeds them via `include_bytes!`. This isn't committed to git (the repo's
`forbid-binary` policy) but still costs zero runtime network calls — corrected mid-implementation
from an earlier version of this plan that vendored the files directly into the source tree
(research.md §10's 2026-07-11 addendum). No `rustfft`/`ndarray` DSP implementation to write or
verify. Token decoding is a lookup into each model's plain-text `vocab.txt` (no `tokenizer.json`
exists for this model family) plus the SentencePiece `▁`→space convention — no `tokenizers` crate
needed. All three (`rustfft`, `ndarray`, `tokenizers`) were added then removed from `Cargo.toml`
during implementation once this was confirmed against the real files.

**Storage**: Local filesystem only — model cache under the OS cache directory (or `--cache-dir`/`PARA_CACHE_DIR`), no database

**Testing**: `cargo test` for inline unit tests; a `tests/contract/` suite for CLI-surface contracts (stdout/stderr separation, exit codes, `--list-models` shape, output schemas); `tests/integration.rs` gated behind an `integration` feature for tests that require a real cached model

**Target Platform**: darwin/arm64 (primary, CoreML-accelerated), darwin/amd64 and linux/amd64 (CPU execution provider)

**Project Type**: Single-project CLI binary (no frontend/backend split)

**Performance Goals**: Not independently specified beyond spec.md's relative claim (SC-005: fastest model tier measurably faster than the most accurate tier on the same input) — no absolute real-time-factor or word-error-rate target is committed to in this plan; see research.md

**Constraints**:

- Offline at runtime once a model is cached (Constitution Principle II); network is used only for first-use/`--refresh-model` model downloads
- Stdout carries only the transcript; all progress, warnings, and errors go to stderr (Constitution Principle III)
- ffmpeg is the only dependency the user installs manually (Constitution Principle VII) — see research.md for how the ONNX Runtime and tokenizer avoid becoming a second one
- No panics in library code; `main` is the sole boundary that converts `Result::Err` to a message + exit code (Constitution Principle IV / Engineering Standards)

**Scale/Scope**: Single-file-per-invocation CLI; no artificial cap on input duration (per spec.md Assumptions), bounded only by host hardware

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Principle                         | Check                                                                                                                                                                                                                                                                                                                                                                                                                                              | Status                                                                                                         |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| I. Single Binary, No Daemon       | `para` is a single `clap`-driven binary invoked once per transcription; no server/listener code anywhere in the design.                                                                                                                                                                                                                                                                                                                            | PASS                                                                                                           |
| II. Offline After Setup           | Network I/O is confined to the model manager's download path, triggered only when a model isn't cached or `--refresh-model` is passed. Inference, audio preprocessing, and output writing perform no I/O beyond the filesystem.                                                                                                                                                                                                                    | PASS                                                                                                           |
| III. Stdout Is Sacred             | Transcript writers (`output/text.rs`, `json.rs`, `srt.rs`) write only to the user-selected writer (stdout or file). `indicatif` progress bars, the CoreML first-compile notice, and chunk-progress messages (FR-023) are stderr-only by construction.                                                                                                                                                                                              | PASS — enforced by a contract test that asserts stdout is byte-for-byte the transcript for every output format |
| IV. Fail Loud, Fail Fast          | ffmpeg-missing, checksum mismatch, unknown model, and bounded-retry-exhausted download all return `Err` immediately with a specific message; `main` is the only place a process exit code is decided. No silent fallback to a different model or partial output.                                                                                                                                                                                   | PASS                                                                                                           |
| V. No Fabricated Data             | Model checksums must be computed from the actual downloaded files during implementation (task-level step, not invented here); the chunking threshold is marked in research.md as "verify before implementing," not hardcoded on assumption. Mel-spectrogram parameters are no longer a verification risk at all — research.md §10 found the model ships its own preprocessor ONNX graph, so there are no hand-computed DSP constants to get wrong. | PASS — contingent on the implementation phase doing the verification research.md defers to it                  |
| VI. Apple Silicon First-Class     | CoreML execution provider is the default `Device::Auto` choice on darwin/aarch64, falling back to CPU only on other targets — not an opt-in flag. Compiled `.mlmodelc` artifacts are cached across runs (`with_model_cache_dir`), not recompiled every invocation.                                                                                                                                                                                                                                                                                                  | PASS — corrected 2026-07-14: `ort`'s `coreml` Cargo feature was never actually enabled, so CoreML had silently never run at all until now (research.md §13); a real ONNX-Runtime/CoreML-EP bug with this model's external-data storage also required `with_static_input_shapes(true)`, so only the static-shaped portion of each graph is currently CoreML-accelerated |
| VII. Minimal Runtime Dependencies | ffmpeg remains the only manual install. The ONNX Runtime is downloaded and **statically linked** at build time (research.md §11) — a `cargo build --release` binary needs no co-located dylib, satisfying both halves of "statically linked or fetched automatically at build time" at once. The `tokenizers` crate dependency was dropped entirely once §10 found this model family has no `tokenizer.json` to load.                                                       | PASS — no packaging caveat (research.md §11 supersedes §2's two-file distribution note)                |
| VIII. Composability Over Features | No server/GUI/plugin surface proposed; `--list-models` and `--refresh-model` are the only additions beyond the spec's core transcription flow, both scoped by clarification.                                                                                                                                                                                                                                                                       | PASS                                                                                                           |
| Engineering Standards             | Every error path in the table above has a corresponding planned test (see Phase 1 contracts and quickstart.md); library code returns `Result`, `main` is the only `eprintln!`/`exit` site; `ort`, `tokenizers`, `reqwest` are well-maintained crates rather than hand-rolled ONNX/HTTP/tokenization code.                                                                                                                                          | PASS                                                                                                           |

No violations requiring justification — Complexity Tracking is intentionally empty.

## Project Structure

### Documentation (this feature)

```text
specs/001-media-transcription/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md         # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/           # Phase 1 output (/speckit-plan command)
│   ├── cli-interface.md
│   ├── output-json-schema.json
│   └── output-srt.md
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
para/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── build.rs                      # Downloads + checksum-verifies the mel-preprocessor ONNX graphs
│                                  # (nemo128.onnx, nemo80.onnx) into OUT_DIR at build time — not
│                                  # committed to git (research.md §10's 2026-07-11 addendum)
├── src/
│   ├── main.rs                  # Entry point; CLI definition (clap); top-level error handling (sole panic/exit boundary)
│   ├── audio.rs                 # ffmpeg subprocess; format detection (diagnostics only); stdin temp-file staging
│   ├── model/
│   │   ├── mod.rs               # Re-exports
│   │   ├── registry.rs          # Static model manifest: IDs, HuggingFace repos, file lists, checksums
│   │   └── manager.rs           # Download, verify (SHA256), cache, list, refresh; bounded retry/backoff (FR-022)
│   ├── inference/
│   │   ├── mod.rs               # Re-exports; Transcript and Segment types
│   │   ├── engine.rs            # ORT session orchestration (preprocessor/encoder/decoder-joint); execution provider selection; chunking for long inputs (FR-023)
│   │   ├── mel.rs                # Loads and runs the build-time-fetched preprocessor ONNX graph (research.md §10) — no hand-rolled DSP, no runtime download
│   │   └── decoder.rs           # TDT greedy decode; CTC greedy decode; token ids → text via vocab.txt lookup
│   └── output/
│       ├── mod.rs               # OutputFormat enum; write_transcript dispatch
│       ├── text.rs
│       ├── json.rs
│       └── srt.rs
└── tests/
    ├── contract/                 # CLI-surface contract tests (stdout/stderr separation, exit codes, --list-models)
    └── integration.rs            # End-to-end tests gated behind `--features integration`
```

**Structure Decision**: Single-project Rust binary crate at the repository root (Option 1 from the template). No frontend/backend or mobile split applies — `para` has one interface surface, the CLI itself. A `tests/contract/` directory is added alongside the prior draft's `tests/integration.rs` to hold CLI-contract-level tests (Phase 1 `contracts/` artifacts), consistent with the Engineering Standard that every error path has a test.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No violations. Table intentionally empty.
