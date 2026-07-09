# Implementation Plan: Local Audio & Video Transcription

**Branch**: `001-media-transcription` | **Date**: 2026-07-09 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-media-transcription/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command. See `.specify/templates/plan-template.md` for the execution workflow.

## Summary

para is a single-invocation Rust CLI that transcribes a local audio or video file (or piped bytes) to text using an on-device ONNX speech model, entirely offline once the model is cached. The user selects one of at least three models trading speed for accuracy (default: NVIDIA Parakeet TDT 0.6B v3, multilingual), and one of three output forms (plain text, JSON with segment timestamps, SRT subtitles). ffmpeg handles format normalization (the only dependency the user installs manually); the ONNX Runtime is fetched automatically at build time via the `ort` crate. Model listing and forced refresh (`--refresh-model`) are supported; standalone removal is not. Model downloads retry a bounded number of times with backoff before failing loud; long inputs that require chunked encoding emit minimal per-chunk progress to stderr.

## Technical Context

**Language/Version**: Rust, 2024 edition, MSRV 1.85 (minimum toolchain shipping the 2024 edition)

**Primary Dependencies**:
- `ort` (ONNX Runtime bindings; CoreML execution provider on Apple Silicon, CPU elsewhere) — exact version and execution-provider module path to be confirmed via `cargo add` and that version's docs.rs page at implementation time; the crate is at major version 2 (RC series) as of this plan and its execution-provider API surface has moved between RCs
- `rustfft` + `ndarray` (mel spectrogram extraction)
- `tokenizers` (HuggingFace tokenizer, default features — see research.md for why the `onig` feature from the prior draft spec is dropped)
- `clap` (CLI parsing, derive + env features)
- `reqwest` + `indicatif` (model download with progress, stderr-only)
- `anyhow` (error propagation to the top-level handler) + `thiserror` (typed internal errors in the model manager, needed to distinguish retryable vs. terminal download failures per FR-022)
- `tempfile`, `which`, `dirs` (stdin staging, ffmpeg discovery, cache path resolution)

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

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Check | Status |
|---|---|---|
| I. Single Binary, No Daemon | `para` is a single `clap`-driven binary invoked once per transcription; no server/listener code anywhere in the design. | PASS |
| II. Offline After Setup | Network I/O is confined to the model manager's download path, triggered only when a model isn't cached or `--refresh-model` is passed. Inference, audio preprocessing, and output writing perform no I/O beyond the filesystem. | PASS |
| III. Stdout Is Sacred | Transcript writers (`output/text.rs`, `json.rs`, `srt.rs`) write only to the user-selected writer (stdout or file). `indicatif` progress bars, the CoreML first-compile notice, and chunk-progress messages (FR-023) are stderr-only by construction. | PASS — enforced by a contract test that asserts stdout is byte-for-byte the transcript for every output format |
| IV. Fail Loud, Fail Fast | ffmpeg-missing, checksum mismatch, unknown model, and bounded-retry-exhausted download all return `Err` immediately with a specific message; `main` is the only place a process exit code is decided. No silent fallback to a different model or partial output. | PASS |
| V. No Fabricated Data | Model checksums must be computed from the actual downloaded files during implementation (task-level step, not invented here); mel-spectrogram parameters and any chunking threshold are marked in research.md as "verify before implementing," not hardcoded on assumption. | PASS — contingent on the implementation phase doing the verification research.md defers to it |
| VI. Apple Silicon First-Class | CoreML execution provider is the default `Device::Auto` choice on darwin/aarch64, falling back to CPU only on other targets — not an opt-in flag. | PASS |
| VII. Minimal Runtime Dependencies | ffmpeg remains the only manual install. The ONNX Runtime is fetched automatically at build time (not a manual runtime install); see research.md for the specific linking strategy and the one caveat this creates for prebuilt-binary distribution. The `tokenizers` crate's optional native-library feature (`onig`) is dropped in favor of its default, dependency-free tokenization path. | PASS — with a documented packaging caveat, not a violation (see research.md §2) |
| VIII. Composability Over Features | No server/GUI/plugin surface proposed; `--list-models` and `--refresh-model` are the only additions beyond the spec's core transcription flow, both scoped by clarification. | PASS |
| Engineering Standards | Every error path in the table above has a corresponding planned test (see Phase 1 contracts and quickstart.md); library code returns `Result`, `main` is the only `eprintln!`/`exit` site; `ort`, `tokenizers`, `reqwest` are well-maintained crates rather than hand-rolled ONNX/HTTP/tokenization code. | PASS |

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
├── Makefile
├── README.md
├── src/
│   ├── main.rs                  # Entry point; CLI definition (clap); top-level error handling (sole panic/exit boundary)
│   ├── audio.rs                 # ffmpeg subprocess; format detection (diagnostics only); stdin temp-file staging
│   ├── model/
│   │   ├── mod.rs               # Re-exports
│   │   ├── registry.rs          # Static model manifest: IDs, HuggingFace repos, file lists, checksums
│   │   └── manager.rs           # Download, verify (SHA256), cache, list, refresh; bounded retry/backoff (FR-022)
│   ├── inference/
│   │   ├── mod.rs               # Re-exports; Transcript and Segment types
│   │   ├── engine.rs            # ORT session setup; execution provider selection; chunking for long inputs (FR-023)
│   │   ├── mel.rs                # Mel spectrogram extraction (rustfft + ndarray)
│   │   └── decoder.rs           # TDT greedy decode; CTC greedy decode; token → string
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
