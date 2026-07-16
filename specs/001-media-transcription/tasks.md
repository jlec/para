---
description: "Task list for Local Audio & Video Transcription"
---

# Tasks: Local Audio & Video Transcription

**Input**: Design documents from `/specs/001-media-transcription/`
**Prerequisites**: plan.md, spec.md, data-model.md, contracts/, research.md, quickstart.md (all present)

**Tests**: Included — Constitution Engineering Standards mandate "every error path has a test," and plan.md's Constitution Check ties several PASS verdicts directly to contract tests existing.

**Organization**: Tasks are grouped by user story (spec.md priorities P1–P4) to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Maps to spec.md user stories (US1–US4)
- File paths are exact and match plan.md's Project Structure

## Path Conventions

Single Rust binary crate at repository root: `src/`, `tests/` (per plan.md — no frontend/backend split applies).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization

- [x] T001 Create the project structure per plan.md's Project Structure section: `Cargo.toml`, `src/main.rs`, `src/audio.rs`, `src/model/{mod.rs,registry.rs,manager.rs}`, `src/inference/{mod.rs,engine.rs,mel.rs,decoder.rs}`, `src/output/{mod.rs,text.rs,json.rs,srt.rs}`, `tests/contract/`, `tests/integration.rs`
- [x] T002 Add dependencies to `Cargo.toml`: `clap` (derive, env), `ort`, `rustfft`, `ndarray`, `tokenizers` (full default features, including `onig` — corrected mid-implementation, see research.md §4), `reqwest` (blocking, stream), `indicatif`, `serde`/`serde_json`, `anyhow`, `thiserror`, `tempfile`, `which`, `dirs`. Resolve real current versions via `cargo add` for each — do not guess or hardcode a version number that hasn't been confirmed to resolve.
- [x] T003 Configure `Cargo.toml`'s `[profile.release]` (lto, codegen-units) and the `integration` test feature flag; add `rust:build`, `rust:release`, `rust:release-all`, `rust:test`, `rust:integration`, `rust:clippy`, `rust:fmt`, `rust:lint`, `rust:clean` tasks to the existing `Taskfile.yml` (this repo's task runner — no separate Makefile) (cross-compilation caveat documented in README, not baked into the task definitions — research.md §9)

**Checkpoint**: `cargo build` succeeds with an empty skeleton before any foundational logic is added.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T004 [P] Define shared types per data-model.md — `Transcript`, `Segment`, `OutputFormat`, `Device`, `ModelKind` — in `src/inference/mod.rs` and `src/output/mod.rs`
- [x] T005 [P] Implement the `Cli` derive struct (`-i/--input`, `-o/--output`, `-m/--model`, `-f/--format`, `--device`, `--cache-dir`, `--list-models`, `--refresh-model`, env var overrides) in `src/main.rs` per contracts/cli-interface.md
- [x] T006 Implement the top-level error boundary in `src/main.rs`: `main()` calls `run() -> anyhow::Result<()>`, converts any `Err` to `eprintln!("error: {e:#}")` + `std::process::exit(1)`; detect stdin-is-a-TTY-with-no-input and exit 1 with a usage error (FR-002) (depends on: T005)
- [x] T007 [P] Implement ffmpeg discovery (`which::which("ffmpeg")`) with a specific "ffmpeg not found" error in `src/audio.rs`
- [x] T008 Implement audio transcoding to 16 kHz mono 16-bit PCM WAV via an ffmpeg subprocess, plus probing for duration and audio-track presence, in `src/audio.rs` (FR-001, FR-003, FR-015) (depends on: T007). Handle non-UTF-8 paths via `Result`/`to_string_lossy()`, not `.unwrap()` — the prior draft spec's `input_path.to_str().unwrap()` sample panics on such paths, which Constitution Engineering Standards prohibit in library code; do not transcribe that pattern.
- [x] T009 Implement stdin staging via `tempfile::NamedTempFile` and magic-byte format detection (WAV/MP3/M4A/MKV/FLAC/OGG, diagnostics only) in `src/audio.rs` (FR-002) (depends on: T008)
- [x] T010 [P] Define the static model registry (3 verified-real models: `parakeet-tdt-0.6b-v3` default TDT, `parakeet-tdt-0.6b-v2` TDT, `parakeet-ctc-0.6b` CTC — research.md §3) in `src/model/registry.rs`. Each entry's `files` list is its encoder + vocab.txt + (TDT only) decoder-joint graph — the mel preprocessor is fetched by `build.rs`, not part of this list (research.md §10; data-model.md). Download each model's real files first and compute actual SHA256 checksums — never placeholder them (Constitution Principle V). All three models downloaded for real (~7.3GB total) and all real SHA-256 checksums filled in and verified end-to-end (`--list-models` reports `Cached` for all three against them).
- [x] T011 Implement model cache path resolution (`--cache-dir`/`PARA_CACHE_DIR`/`dirs::cache_dir()` default) and cache-state checking (`NotCached`/`Cached` via file existence + checksum match) in `src/model/manager.rs` (depends on: T010)
- [x] T012 Implement model download with stderr-only progress (`indicatif`), atomic `.tmp`-then-rename on success, stale-`.tmp` cleanup on startup, and a `download.lock` guard file in `src/model/manager.rs` (depends on: T011). Implemented and unit-tested for cache-state logic; not yet exercised against a live download (no network fetch has actually been run through this path).
- [x] T013 Implement bounded download retry with exponential backoff (3 attempts) in `src/model/manager.rs`; on exhaustion, return a specific `thiserror` error and never fall back to a different cached model (FR-022) (depends on: T012)
- [x] T014 Implement `--refresh-model` support (delete cached files, then re-download) as a manager function in `src/model/manager.rs` (FR-020) (depends on: T013)
- [x] T015 [P] Implement `src/inference/mel.rs` to load and run the preprocessor ONNX graph (`nemo128.onnx` for TDT models, `nemo80.onnx` for CTC, selected by `ModelKind`) as an `ort` session: raw waveform samples + lengths in, mel-feature tensor + lengths out (research.md §10 — supersedes the original rustfft/ndarray plan; no runtime download, no cache-state, always present). `build.rs` downloads and checksum-verifies both files at build time into `OUT_DIR`; `mel.rs` embeds them via `include_bytes!(concat!(env!("OUT_DIR"), ...))` — never committed to git (repo's `forbid-binary` policy; research.md §10's 2026-07-11 addendum). Read the actual input/output tensor names (`waveforms`/`waveforms_lens`/`features`/`features_lens` per the reference implementation, but verify against the real file) rather than hardcoding unconfirmed names. Tensor names confirmed via `onnxruntime` introspection of the real downloaded files.
- [x] T016 [P] Implement ONNX Runtime session-building in `src/inference/engine.rs`: a shared helper to construct a session from a model file path with device selection (CoreML default on darwin/aarch64, CPU elsewhere, explicit `--device` override) and the CoreML first-compile stderr notice. `engine.rs` will hold one such session per pipeline stage (preprocessor, encoder, and for TDT models a decoder-joint network — research.md §10). Verify the exact `ort` crate API (execution-provider construction, module path) against the version resolved in T002's docs.rs page before writing — do not transcribe the 1.x-style sample from the prior draft spec (research.md §1). API verified directly against the vendored `ort` 2.0.0-rc.12 source in the local cargo registry cache. Actual session creation (not just compilation) verified while implementing T017 — this surfaced and fixed a real linking-config bug (research.md §11).
- [x] T017 Implement chunked-encoding support for long inputs in `src/inference/engine.rs`, including the per-chunk `"transcribing chunk N of M"` stderr message (FR-023). Determine the actual chunk-length threshold empirically against the loaded encoder rather than hardcoding a guessed value (research.md §6) (depends on: T016). Downloaded the real `parakeet-tdt-0.6b-v3` encoder and binary-searched its actual input-length limit: a genuine `ort` runtime error (not a soft memory/latency tradeoff) past ~400s of audio, from a fixed-size relative positional encoding buffer in the graph. Chunk threshold set to 300s with margin (research.md §6). Surfaced and fixed a real bug along the way: the `ort` linking config (`load-dynamic` + `download-binaries`) silently skipped the build-time fetch and hung the process instead of erroring on first real session use — corrected to `download-binaries` alone (true static linking, research.md §11). `encode_chunked`/`chunk_ranges` unit-tested (pure logic, 4 tests) plus a real-encoder `#[ignore]`d integration test (passing, ~3s).
- [x] T018 Implement vocab loading and token-id-to-text decoding (read `vocab.txt` as a line-indexed piece list; join pieces and replace SentencePiece `▁` with a space) as part of model resource loading in `src/inference/decoder.rs` (research.md §10 — supersedes the original `tokenizers` crate plan; no such crate is needed) (depends on: T017). Implemented ahead of T017 — pure vocab-lookup logic has no dependency on the loaded encoder, unlike T017's empirical chunk threshold. Real `vocab.txt` downloaded and inspected directly: each line is `<piece> <id>`, not a bare piece (research.md §10's 2026-07-11 correction); blank token located by name (`<blk>`) rather than a hardcoded id. Unit-tested with synthetic vocab fixtures (6 tests); not yet wired to a real decode loop (T022/T030).

**Checkpoint**: Foundation ready — audio in, model acquired and cached, ONNX sessions buildable, preprocessor graph runnable. No transcript can be produced yet (no decoder logic, no output formatter) — that's what the user story phases add.

---

## Phase 3: User Story 1 - Get a plain-text transcript from a file (Priority: P1) 🎯 MVP

**Goal**: Point para at a file (or pipe bytes in) and get a plain-text transcript, printed or written to a file, with no manual conversion step.

**Independent Test**: Run para against a sample audio file and a sample video file in different common formats; confirm a readable transcript from each with no prior manual conversion, and that output can be redirected to a file or piped in via stdin.

### Tests for User Story 1

- [x] T019 [P] [US1] Contract test: `--format text` (default) writes only the transcript to stdout, nothing else, in `tests/contract/test_stdout_contract.rs`. Gated behind `integration` (needs a real cached model); verified passing against `parakeet-ctc-0.6b`.
- [x] T020 [P] [US1] Contract test: ffmpeg-missing, input-not-found, no-audio-track, empty/corrupted-file, and an unwritable output destination (no disk space / no write permission, FR-024) all exit non-zero with a specific stderr message and empty stdout, in `tests/contract/test_error_paths.rs`. Runs in the default suite (no model needed) — surfaced and fixed a real ordering bug: input/output validation now happens before model resolution/download, not after (see T024/T025).
- [x] T021 [P] [US1] Integration test (feature = `integration`): end-to-end text transcription against a cached fixture model produces a non-empty transcript, in `tests/integration.rs`. Uses macOS `say` to generate real speech at test time and asserts on actual recognized words, not just non-emptiness.

### Implementation for User Story 1

- [x] T022 [P] [US1] Implement the TDT greedy decoder (token+duration decode loop, segment collection) in `src/inference/decoder.rs`. Read actual tensor names/shapes from the real downloaded ONNX files before hardcoding any (research.md §3). Algorithm verified against the real `onnx-asr` Python source (`NemoConformerTdt`/`_AsrWithTransducerDecoding`), not re-derived from tensor shapes alone. End-to-end verified against real speech (macOS `say` fixture): near-perfect transcription. Segment grouping (phrase-level, silence-gap heuristic) is this project's own design decision — the reference library only returns flat token timestamps.
- [x] T023 [P] [US1] Implement the plain-text output formatter (`transcript.text` + trailing newline, nothing else) in `src/output/text.rs`
- [x] T024 [US1] Wire `run()` in `src/main.rs`: resolve input (file path or staged stdin) → transcode via `audio.rs` → ensure the default model is cached via `manager.rs` → build an ONNX session via `engine.rs` → `mel.rs` → `decoder.rs` (TDT) → `text.rs` → write to stdout or the `-o` file (FR-004, FR-011) (depends on: T022, T023). Input/output validation reordered ahead of model download for fail-fast behavior (Constitution Principle IV) — a bad `-i`/`-o` path now fails in under a second instead of after a multi-GB download.
- [x] T025 [US1] Add the specific stderr error messages for each FR-015 rejection case (unsupported/corrupted/no-audio input) and the FR-024 unwritable-output-destination case in `src/main.rs`, matching contracts/cli-interface.md's error table (depends on: T024)

**Checkpoint**: At this point, User Story 1 is fully functional and independently testable — this is the MVP.

---

## Phase 4: User Story 2 - Choose a model to balance speed and accuracy (Priority: P2)

**Goal**: Let the user pick from ≥3 models trading speed for accuracy, with a sensible default, a listing view, and a forced-refresh option.

**Independent Test**: Run the same input through each model option; confirm each completes, the fastest option is measurably quicker than the most accurate, `--list-models` shows every option's cache state, and an invalid `--model` value fails immediately with a valid-options list.

### Tests for User Story 2

- [x] T026 [P] [US2] Contract test: an unrecognized `--model` value exits non-zero, lists valid IDs, and attempts no transcription, in `tests/contract/test_model_unknown.rs`
- [x] T027 [P] [US2] Contract test: `--list-models` lists every registered model with cache state and marks exactly one default, in `tests/contract/test_list_models.rs`. Gated behind `integration`.
- [x] T028 [P] [US2] Contract test: selecting a specific model actually uses that model (echoed in output/status), never silently substituted, in `tests/contract/test_model_selection.rs`. Gated behind `integration`.
- [x] T029 [P] [US2] Contract test: the CLI's flag surface has no standalone command whose only effect is removing a cached model without also re-fetching it (guards FR-021), in `tests/contract/test_no_standalone_remove.rs`

### Implementation for User Story 2

- [x] T030 [US2] Implement the CTC greedy decoder (argmax + collapse-repeats + remove-blanks, single whole-file segment) in `src/inference/decoder.rs`. Verify actual tensor names from the real CTC ONNX files before hardcoding (research.md §3) (depends on: T022). Algorithm verified against the real `onnx-asr` `_AsrWithCtcDecoding._decoding` source. `parakeet-ctc-0.6b`'s `model.onnx` output layout (`logprobs [B,T,V]`, time before vocab) verified directly — differs from the TDT encoder's hidden-before-time layout, not assumed identical. End-to-end verified against real speech.
- [x] T031 [US2] Wire `--model` flag validation against the registry (unknown ID → error + valid-options list, FR-010) in `src/main.rs` (depends on: T024, T030)
- [x] T032 [US2] Emit a stderr status line identifying the model actually used (e.g., `"using model: <id>"`) on every transcription run, in `src/main.rs` (FR-009; spec.md US2 acceptance scenarios 1 and 3 — the model used must be "clearly identified," not just correct) (depends on: T031)
- [x] T033 [US2] Implement the `--list-models` command output (id, description including language/timing-granularity per data-model.md's `ModelOption`, cache state, default marker), exiting 0 without transcribing (FR-019) (depends on: T032). Surfaced and fixed a real perf bug while verifying end-to-end: cache-state checking re-hashed every checksummed file on every call, turning `--list-models` into a 79-second, ~7GB checksum sweep once T010's real checksums landed — fixed to existence-only checks (research.md §7's 2026-07-13 correction); now ~1s.
- [x] T034 [US2] Wire `--refresh-model` to the manager's refresh function from T014, in `src/main.rs` (FR-020) (depends on: T033)

**Checkpoint**: User Stories 1 AND 2 both work independently.

---

## Phase 5: User Story 3 - Get timed, structured output for further processing (Priority: P3)

**Goal**: Produce the transcript as machine-parseable JSON with segment-level start/end timestamps.

**Independent Test**: Run para requesting structured timed output; confirm the result validates against contracts/output-json-schema.json and every segment has `end > start`.

### Tests for User Story 3

- [x] T035 [P] [US3] Contract test: `--format json` output validates against `contracts/output-json-schema.json`, and every segment has `end > start`, in `tests/contract/test_json_output.rs`. Gated behind `integration`.
- [x] T036 [P] [US3] Integration test (feature = `integration`): end-to-end JSON transcription; deserialize and confirm `text`/`segments`/`model`/`duration_seconds` are present, in `tests/integration.rs`. Uses a real `say`-generated speech fixture.

### Implementation for User Story 3

- [x] T037 [P] [US3] Implement the JSON output formatter (`serde_json`, seconds rounded to 2 decimal places) in `src/output/json.rs` per contracts/output-json-schema.json
- [x] T038 [US3] Wire the `--format json` dispatch in `src/main.rs` (depends on: T037). `output::write_transcript`'s existing format dispatch (mod.rs) meant `run()` only needed to build a `Transcript` and call it once per `OutputFormat` — no per-format branching needed in `main.rs` itself.

**Checkpoint**: User Stories 1, 2, AND 3 all work independently.

---

## Phase 6: User Story 4 - Get a subtitle file for a video (Priority: P4)

**Goal**: Produce a subtitle file with correctly ordered, non-overlapping timed captions usable by common video players.

**Independent Test**: Run para against a sample video requesting subtitle output; confirm the result loads correctly and matches the SRT contract (comma-separated milliseconds, sequential block numbers).

### Tests for User Story 4

- [x] T039 [P] [US4] Contract test: SRT block numbering, comma millisecond separator, blank-line spacing, and the single-segment CTC-model fallback, in `tests/contract/test_srt_output.rs`. Gated behind `integration`.
- [x] T040 [P] [US4] Integration test (feature = `integration`): end-to-end SRT transcription; verify `-->` and the comma separator are present, in `tests/integration.rs`. Uses a real `say`-generated speech fixture.

### Implementation for User Story 4

- [x] T041 [P] [US4] Implement the `fmt_srt_time` helper and SRT output formatter in `src/output/srt.rs` per contracts/output-srt.md
- [x] T042 [US4] Wire the `--format srt` dispatch in `src/main.rs` (depends on: T041). Same `write_transcript` dispatch as T038 covers this.

**Checkpoint**: All four user stories are independently functional.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Documentation and final verification across all stories

- [x] T043 [P] Write `README.md` per plan.md's README requirements: prerequisites, build, first-run behavior (model download progress, CoreML compile notice), all flags with examples, model IDs and when to use each, `PARA_CACHE_DIR`, language support notes, the ONNX Runtime build-time-fetch-and-static-link note (research.md §11 — corrected from an earlier `load-dynamic`/`ORT_DYLIB_PATH` design that was dropped before ever being exercised), and the cross-compilation-tooling caveat (research.md §9). Also fixed a stale reference in `contracts/cli-interface.md` to the nonexistent `parakeet-ctc-1.1b` (a leftover from before the registry-size correction, research.md §3).
- [x] T044 [P] Run `cargo clippy -- -D warnings` and `cargo fmt --check`; fix any findings. Clean (including `--all-targets`, covering tests).
- [x] T045 Run every scenario in quickstart.md end-to-end against a real release build (all four user stories, offline operation, pipeline safety, `--refresh-model`). All passed against real `say`-generated speech + a synthesized video file: US1 (wav/mp4/file-redirect/stdin), US2 (`--list-models`, invalid model, and — on a 60s clip where decode cost outweighs fixed model-load overhead — CTC measurably faster than TDT-v3, 8.1s vs 9.9s), US3 (JSON schema), US4 (SRT), pipeline safety, and `--refresh-model` (real re-download verified). Offline-operation scenario (disconnecting the network) not exercised in this environment — would require disabling the host's network interface; the design (no runtime network calls outside model download, verified throughout T010-T017) has been checked by inspection instead.
- [x] T046 [P] Add inline `#[cfg(test)]` unit tests for output-formatter edge cases not already covered by contract tests (e.g., `fmt_srt_time(3661.5) == "01:01:01,500"`, CTC single-segment SRT block) in `src/output/srt.rs` and `src/output/json.rs`. Already present from Foundational-phase work.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Setup — BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational only — this is the MVP
- **User Story 2 (Phase 4)**: Depends on Foundational; T030 also depends on US1's T022 (both extend `decoder.rs`)
- **User Story 3 (Phase 5)**: Depends on Foundational; independent of US2, only needs a `Transcript` with `segments` (produced by either decoder from US1/US2)
- **User Story 4 (Phase 6)**: Depends on Foundational; independent of US2/US3
- **Polish (Phase 7)**: Depends on all four user stories being complete

### Within Each Phase

- Tests are written before their corresponding implementation tasks and MUST fail first
- `src/model/manager.rs` tasks (T011→T014) are strictly sequential (same file, each builds on the last)
- `src/inference/engine.rs` tasks (T016→T018) are strictly sequential (same file)
- `src/main.rs` wiring tasks within a story are strictly sequential (same file)

### Parallel Opportunities

- Setup: none (T002/T003 both edit `Cargo.toml` sequentially)
- Foundational: T004, T005, T007, T010, T015, T016 can start in parallel (five independent files); their sequential follow-ons (T006, T008→T009, T011→T014, T017→T018) proceed once each starting task lands
- US1: T019, T020, T021 (tests) in parallel; T022, T023 (decoder vs. text formatter) in parallel
- US2: T026, T027, T028, T029 (tests, four separate files) in parallel
- US3: T035, T036 in parallel; T037 has no same-phase counterpart to parallelize with
- US4: T039, T040 in parallel; T041 has no same-phase counterpart to parallelize with
- Polish: T043, T044, T046 in parallel; T045 runs last, after everything else

## Parallel Example: Foundational Phase

```bash
Task: "Define shared types in src/inference/mod.rs and src/output/mod.rs"          # T004
Task: "Implement the Cli derive struct in src/main.rs"                             # T005
Task: "Implement ffmpeg discovery in src/audio.rs"                                 # T007
Task: "Define the static model registry in src/model/registry.rs"                 # T010
Task: "Implement mel spectrogram extraction in src/inference/mel.rs"              # T015
Task: "Implement ONNX Runtime session setup in src/inference/engine.rs"           # T016
```

## Parallel Example: User Story 1

```bash
Task: "Contract test: stdout-only-transcript in tests/contract/test_stdout_contract.rs"   # T019
Task: "Contract test: error paths in tests/contract/test_error_paths.rs"                  # T020
Task: "Integration test: end-to-end text in tests/integration.rs"                         # T021
Task: "Implement TDT decoder in src/inference/decoder.rs"                                 # T022
Task: "Implement text formatter in src/output/text.rs"                                    # T023
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL — blocks everything)
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: run quickstart.md's US1 section against a real build
5. This is a shippable MVP: `para -i file.wav` → plain-text transcript, offline-capable, fail-loud

### Incremental Delivery

1. Setup + Foundational → nothing user-visible yet, but the substrate is real and tested
2. - User Story 1 → MVP: plain-text transcription (deploy/demo)
3. - User Story 2 → model choice, listing, refresh (deploy/demo)
4. - User Story 3 → structured JSON output (deploy/demo)
5. - User Story 4 → subtitle output (deploy/demo)
6. - Polish → README, lint-clean, full quickstart pass

### Team Strategy

Once Foundational is done, US2/US3/US4 can be split across contributors — US3 and US4 only need a `Transcript` with `segments`, which US1's T022 (TDT decoder) already produces; they don't need to wait on US2's CTC decoder work. Only T030 (CTC decoder) has a real cross-story dependency, on T022.

---

## Notes

- [P] tasks = different files, no dependency on an incomplete task
- [Story] label maps each task to its user story for traceability
- Every FR-0xx / SC-00x reference above ties a task back to spec.md; every research.md §N reference flags a place where a plan-time decision was deliberately left open for implementation-time verification (Constitution Principle V) — do not silently fill in a guessed value where one of these references appears
- Commit after each task or logical group
- Stop at any checkpoint to validate a story independently

---

## Phase 8: Convergence

**Purpose**: Close gaps found by `/speckit-converge` between spec.md/plan.md/tasks.md intent and the current codebase — see each task's source-ref and gap-type.

- [x] T047 [HIGH] Fix CTC greedy decoder fabricating a word on silent/no-speech input per FR-015 (contradicts). Verified live: 3s of pure digital silence through `parakeet-ctc-0.6b` deterministically produces the transcript `"uh"` (exit 0), while the identical input through the default TDT model correctly returns an empty transcript. spec.md's edge case "What happens when the audio contains no detectable speech (e.g., music only, silence)?" was never resolved in research.md/data-model.md/plan.md, and the two models now behave inconsistently on it — investigate a blank-probability floor or confidence threshold in `decode_ctc_chunk` (`src/inference/decoder.rs`) so it doesn't hallucinate content from silence, aligning with TDT's correct empty-transcript behavior. Fixed with an empirically-measured `MIN_BLANK_MARGIN = 2.5` log-probability margin (research.md §16): measured hallucination margins (0.62-2.14 across 1s/2s/3s silence) sit well below measured genuine-token margins (3.14 for the softest real disfluency, "um", up to 12+ for crisp phonemes). Verified: silence at 1s/2s/3s/5s/10s now all produce empty transcripts; real speech (including the borderline "um" case) is unaffected. Two new unit tests added (`ctc_decode_suppresses_low_margin_token_as_blank`, `ctc_decode_accepts_token_at_the_margin_boundary`); the existing collapse/drop-blank test's synthetic margins were widened (1.0 → 5.0) to stay clear of the new filter.
- [x] T048 [HIGH] Add test coverage for the model-download retry/backoff/lock/refresh error paths per FR-022, FR-020, and Constitution Engineering Standards ("every error path MUST have a test") (missing). `src/model/manager.rs` currently has zero tests for `DownloadError::is_retryable()` classification, the bounded-retry-with-backoff loop (`download_one_with_retry`), checksum-mismatch-is-terminal-not-retried behavior, `DownloadLock`'s stale-`.tmp` cleanup-on-acquire, or `refresh()`'s delete-then-redownload sequencing — only `cache_state_in`'s two existence-check tests exist. Use an unreachable/local URL fixture (not real network) to exercise the retry loop's bounded-attempts and backoff behavior without depending on network conditions. Added 9 tests: a one-shot local `TcpListener`-based mock HTTP server (no real network dependency) exercises real download+checksum-verify round-trips (success, mismatch-leaves-no-file-behind), `is_retryable()` classification for all three `DownloadError` variants (an unparseable URL for `Network`, a constructed `Io`, a constructed `ChecksumMismatch`), `download_one_with_retry`'s checksum-mismatch-is-terminal (fast-fail, <500ms) vs. network-failure-retries-with-backoff-then-gives-up (port-1 connection-refused, ~3s — the real 1s+2s backoff cost, not mocked away), `DownloadLock`'s stale-`.tmp` cleanup-on-acquire and self-removal-on-drop, and `refresh()`'s delete-then-redownload-via-the-mock-server sequencing.
- [x] T049 [MEDIUM] Evaluate and address the TDT chunk-boundary transcription-quality artifact per FR-023 / research.md §6 (partial). Verified live with a real 338s speech clip (2 chunks split at the 300s threshold): the TDT decoder's LSTM state resets fully at each chunk boundary with no overlap or context carry-over, producing an observable artifact exactly at the split (the phrase "chunk transcription" became "chunking. Transcription" right at the boundary). research.md §6 decided "no overlap between chunks" as a design choice but never empirically verified the transcription-quality cost of that decision until this convergence pass. Either implement a lightweight mitigation (e.g., a small audio overlap between chunks, or carrying decoder LSTM state across the boundary) in `src/inference/engine.rs`/`decoder.rs`, or explicitly document the current boundary-artifact behavior as an accepted tradeoff in research.md §6 if no fix is pursued. Implemented the decoder-state-carry-over mitigation (research.md §17): a new `DecoderState` struct threads the prediction network's LSTM state and previous-token context across chunk boundaries in `decode_tdt`/`decode_tdt_chunk`, instead of resetting to a fresh beginning-of-utterance state per chunk. Verified with a true before/after (`git stash` the fix, rebuild, re-run the same 436s clip of 160 numbered sentences chunked 300s+136s, then diff word-for-word against the post-fix transcript — not just an isolated after-only read, which initially led to a wrong attribution): a numeral slip ("Sentence number 111" heard as "11") is present in *both* runs, confirming it's an unrelated acoustic-ambiguity issue, not a boundary artifact; the real, measured difference is a spurious duplicated "Test." word inserted near the boundary (sentences 115-116) only in the pre-fix run, which the fix removes. A real, verified improvement, not a complete one — the encoder itself still lacks cross-chunk acoustic context; documented as a residual, accepted limitation in research.md §17 rather than pursuing a riskier audio-overlap change with no reference algorithm to verify against.

---

## Phase 9: Convergence

**Purpose**: Close gaps found by `/speckit-converge` between spec.md/plan.md/tasks.md intent and the current codebase — see each task's source-ref and gap-type.

- [x] T050 [HIGH] Update the `--device` flag row in `contracts/cli-interface.md` per Constitution VI (contradicts). The row still reads "`auto` picks CoreML on darwin/aarch64 and CPU elsewhere per Constitution Principle VI" — stale since the 2026-07-14 amendment to Constitution v2.0.0, which made CPU the default execution provider everywhere and CoreML an explicit `--device coreml` opt-in only (research.md §15). This frozen-contract file was not updated alongside that amendment and now contradicts both the constitution and the actual `engine.rs` behavior, as well as README.md's already-corrected wording.
- [x] T051 [MEDIUM] Update the Target Platform line in `plan.md`'s Technical Context per Constitution VI (contradicts). Still reads "darwin/arm64 (primary, **CoreML-accelerated**)" — inconsistent with the same file's own Constitution Check row and Technical Context dependency bullet, both already corrected in the 2026-07-14 amendment commit to reflect CPU-default/CoreML-opt-in.
