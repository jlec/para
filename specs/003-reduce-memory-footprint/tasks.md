# Tasks: Reduce Transcription Memory Footprint

**Input**: Design documents from `/specs/003-reduce-memory-footprint/`

**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, quickstart.md

**Tests**: Unit tests for `chunk_ranges` are extended (existing project convention — see `src/inference/engine.rs`'s current test module). Real-measurement verification (peak RSS, transcript diff) is required by FR-005 and is captured as explicit tasks below, not skipped as "just tests."

**Organization**: Tasks are grouped by user story from spec.md. The two-threshold chunking fix is a single, small change in one file (`src/inference/engine.rs`) — per plan.md's Structure Decision, this is a Foundational change both user stories depend on for their independent verification.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2)
- Exact file paths are included in each description

## Path Conventions

Single-project Rust CLI — `src/`, `tests/` at repository root (per plan.md).

---

## Phase 1: Setup

**Purpose**: Confirm the environment needed to measure and verify this fix is ready

- [x] T001 Confirm a release build exists (`task rust:release`), the default and CTC models are cached, and both a short (~2 min) and long (~25 min) sample audio file are available, per quickstart.md's Prerequisites — this feature's tasks are verified with real measurements, not just `cargo test`. Done: release build present, all 3 models cached, long file present; a fair 2-minute short clip was synthesized from the same long recording via `ffmpeg -t 120` for an apples-to-apples comparison.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The two-threshold chunking change itself — both user stories' independent tests depend on this being in place first

**⚠️ CRITICAL**: No user story verification can begin until this phase is complete

- [x] T002 In `src/inference/engine.rs`, rename the existing `CHUNK_SECONDS` constant's role: add `SINGLE_PASS_THRESHOLD_SECONDS: f64 = 300.0` (same value, same meaning as today's single-pass gate — data-model.md's "Single-pass threshold" row)
- [x] T003 In `src/inference/engine.rs`, add a new, smaller `CHUNK_SECONDS` constant governing per-chunk size once chunking is needed. **Final value: 30.0s, not research.md Phase 0's originally-tested 15.0s** — real content-parity testing (T008) found 15s made the TDT transducer drop whole phrases near chunk boundaries (up to 194 consecutive words in one case), even with overlap; 30s keeps differences to cosmetic wording/punctuation only. See T003a/T003b below for the additional mechanism this required.
- [x] T003a _(added during implementation, not in original plan)_ Diagnosed the T003 content-loss regression: verified in `src/inference/decoder.rs` that the TDT decoder's autoregressive state (LSTM hidden state + previous token) is already threaded correctly across chunks — the bug was not there. The actual cause: each chunk's encoder pass ran on zero-overlap audio, so the Conformer encoder had no acoustic context at chunk boundaries, causing the transducer to emit blank (drop content) near cuts — more frequent with smaller chunks.
- [x] T003b _(added during implementation, not in original plan)_ Implemented overlap-and-trim in `encode_chunked` (TDT path only — `src/inference/engine.rs`): added `CHUNK_OVERLAP_SECONDS: f64 = 5.0` and `ENCODER_FRAMES_PER_SECOND: f64 = 12.5` (real, measured value — matches FluidAudio's independently-published CoreML encoder's own frame rate) plus a `trim_frames` helper. Each chunk's encoder now sees `CHUNK_OVERLAP_SECONDS` of extra audio on both sides for context; only the frames covering the chunk's own original (non-overlapping) range are decoded, so no audio is ever decoded twice. CTC (`encode_chunked_ctc`) does not need this — measured separately to tolerate small chunks with zero overlap since its per-frame classification doesn't depend on carried decoder state the way the transducer's blank/emit decision does.
- [x] T004 Update `chunk_ranges` in `src/inference/engine.rs` so the single-pass check (`total_samples <= ...`) reads `SINGLE_PASS_THRESHOLD_SECONDS`, while the window size used once chunking is needed reads the new, smaller `CHUNK_SECONDS` — preserving the existing "single range when short enough / split into windows otherwise" structure (data-model.md)
- [x] T005 Update `chunk_ranges`'s existing unit tests in `src/inference/engine.rs` (`single_chunk_for_input_under_threshold` → renamed `single_chunk_for_input_under_single_pass_threshold`, `splits_into_multiple_chunks_over_threshold` → renamed `splits_into_chunk_seconds_windows_once_over_single_pass_threshold`, `chunk_ranges_cover_input_with_no_gaps_or_overlap`, `exact_multiple_of_chunk_length_does_not_add_empty_trailing_chunk`) to reflect the two separate constants; added a new test `single_chunk_when_over_chunk_seconds_but_under_single_pass_threshold` covering the case that motivated splitting the constants in the first place
- [x] T006 Run `task rust:test` and `task rust:lint` to confirm the updated `chunk_ranges` logic and tests build clean and pass. Done: 52 unit tests + 7 contract tests pass; clippy and fmt clean.

**Checkpoint**: Two-threshold chunking (with overlap-and-trim for TDT) is implemented and unit-tested — user story verification can now begin

---

## Phase 3: User Story 1 - Transcribe a long recording without excessive memory use (Priority: P1) 🎯 MVP

**Goal**: A long recording's peak memory stays close to a short recording's, rather than growing with duration (FR-001, SC-001, SC-002)

**Independent Test**: Transcribe a short clip and a long recording with the same model; peak memory for the long one should be close to the short one's, not proportionally larger, and at least 40% below the ~5.79GB baseline on the original reported file

- [x] T007 [US1] Run quickstart.md's US1 scenario: poll peak RSS transcribing a short clip and a long recording with the default model (`--no-progress`); confirm peak RSS values are close and memory plateaus rather than climbing continuously through the long run. **Result**: short clip (2 min) 2.48GB, long recording (~25.7 min) 2.75GB — close and flat, not proportional to duration.
- [x] T008 [US1] Run quickstart.md's "Regression check": transcribe a long recording with the fix vs. pre-fix (300s single-chunk) code; `diff` the two transcripts and confirm no real content loss (FR-004) — note per `001-media-transcription` research.md §17 that differences exactly at shifted chunk boundaries are an acceptable, documented caveat, not a failure. **Result**: with the final 30s/5s-overlap values, the two transcripts differ only in cosmetic wording/punctuation (e.g. "T4" vs "T four", filler words); the longest run of consecutive differing words is 8, with no dropped clauses — the same regression check with the initially-tested 15s value (no overlap) had failed this check outright, dropping up to 194 consecutive real words, which is why T003/T003a/T003b exist.
- [x] T009 [US1] Run quickstart.md's "original reported case": transcribe `~/tmp/07-01_Meeting_GPU_Management_Strategy_Forecasting_and_Global-China_HPC_Deployment.mp3` with the fix and default model; confirm peak memory is at least 40% below the 5.79GB baseline (SC-002). **Result**: 2.570GB peak, exit 0, real transcript — a 50.0% reduction, comfortably clearing the 40% bar.
- [x] T010 [US1] Time the same long-recording transcription before and after the fix; confirm no meaningful speed regression (plan.md's Performance Goals). **Result**: post-fix (30s chunks + overlap) took 3m28s real time; pre-fix (300s single-chunk) took 4m38s — the fix is faster, not slower, plausibly because the much smaller per-call working set reduces memory-pressure overhead.

**Checkpoint**: User Story 1 is independently verified — the core problem (memory growing with duration) is fixed and confirmed on the original reported case, with no correctness or speed regression

---

## Phase 4: User Story 2 - Memory use stays proportional to the model in use (Priority: P2)

**Goal**: Peak memory for each of the tool's models stays within a small, consistent multiple of that model's on-disk size (FR-002, FR-003, SC-003)

**Independent Test**: Transcribe the same short input with the smallest and largest available models; confirm each model's peak memory is a small, consistent multiple of its on-disk size, and the smaller model uses proportionally less

- [x] T011 [US2] Run quickstart.md's US2 scenario: transcribe the same short clip with `parakeet-ctc-0.6b` (smallest) and `parakeet-tdt-0.6b-v3` (largest, default); record peak RSS for each and compare against each model's on-disk cache size. **Result**: CTC peak 2.99GB (on-disk 2.4GB), TDT peak 3.35GB (on-disk 2.4GB).
- [x] T012 [US2] Confirm the peak-memory-to-on-disk-size ratio is small and consistent across both models (SC-003). **Result**: CTC ≈1.25x, TDT ≈1.4x on-disk size — both small and consistent with each other, confirming the fix (model-agnostic — it changes chunk size, not model loading) holds across models, not just the default.

**Checkpoint**: Both user stories are independently verified — memory is flat with duration (US1) and proportional to model size (US2)

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: Final cleanliness and full-suite confirmation across both stories

- [x] T013 [P] Grep `src/inference/engine.rs` for any leftover `PARA_DEBUG`/temporary diagnostic code and confirm none remains — including the additional `PARA_DEBUG_CHUNK_SECONDS`/`PARA_DEBUG_OVERLAP_SECONDS`/`PARA_DEBUG_FRAME_RATE` scaffolding used during T003/T003b's retuning sweep. **Result**: grep returns empty; all diagnostic code was reverted after the final values were chosen.
- [x] T014 Run `task rust:release` and the full `task rust:test` suite one final time on the finished state of `src/inference/engine.rs` to confirm everything builds and passes together. **Result**: clean release build, 52 unit tests + 7 contract tests pass, clippy and fmt clean.
- [x] T015 Re-run quickstart.md's edge cases informally: a multi-hour synthetic recording to confirm memory still plateaus rather than growing unbounded (spec.md Edge Cases, SC-004). **Result**: a synthetic 1-hour (3600s) recording — 2.4x longer than the original reported case — peaked at 2.82GB, exit 0, confirming memory stays bounded well beyond the originally reported file's length.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup — BLOCKS both user stories (T002-T004 are strictly sequential, same function; T005 depends on T004; T006 depends on T005)
- **User Story 1 (Phase 3)**: Depends on Foundational completion (T006)
- **User Story 2 (Phase 4)**: Depends on Foundational completion (T006); independent of US1's tasks, though T009's tuning (if it changes `CHUNK_SECONDS`) should land before T011-T012 re-measure
- **Polish (Phase 5)**: Depends on both user stories being verified

### Within Each Phase

- T002 → T003 → T004 → T005 → T006 (same file, same function — strictly sequential, no [P])
- T007, T008, T009, T010 can be run in any order once T006 is done, but T009 may loop back into T003/T004 if the tuning value needs adjustment
- T011 → T012 (T012 interprets T011's measurements)

### Parallel Opportunities

- T013 has no dependency on T014/T015's outcomes and can run alongside them
- US1 (Phase 3) and US2 (Phase 4) verification tasks can run in parallel with each other once Phase 2 is complete, since they measure different things (duration-scaling vs. model-scaling) on the same fixed code

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (the actual code change — this is most of the real work)
3. Complete Phase 3: User Story 1 — this alone resolves the reported complaint (SC-002 on the original file)
4. **STOP and VALIDATE**: T009's real measurement against the original reported file is the definitive check
5. Phase 4 (US2) and Phase 5 (Polish) confirm the fix generalizes and leaves nothing behind, but the MVP is complete after Phase 3

### Incremental Delivery

1. Setup + Foundational → the fix exists and passes unit tests
2. User Story 1 → real-measurement proof the original problem is solved
3. User Story 2 → real-measurement proof it holds across all models, not just the default
4. Polish → cleanliness and full-suite confirmation
