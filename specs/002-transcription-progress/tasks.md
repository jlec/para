---
description: "Task list for Transcription Progress Indicators"
---

# Tasks: Transcription Progress Indicators

**Input**: Design documents from `/specs/002-transcription-progress/`

**Prerequisites**: plan.md, spec.md, data-model.md, contracts/, research.md, quickstart.md (all present)

**Tests**: Included — Constitution Engineering Standards mandate "every error path has a test," and
this feature adds several new observable stderr/exit-code contracts of its own.

**Organization**: Tasks are grouped by user story (spec.md priorities P1–P4). Note on independence:
all four stories share one underlying mechanism (a single `TranscriptionProgress` handle with an
interactive/non-interactive/suppressed branch built once in Foundational), since that branch must
exist correctly from the start for *any* phase to be minimally correct (a bar that only ever
animates would violate FR-007 the moment it runs non-interactively). US1/US2 add the phases
themselves; US3/US4 are independently *testable* verifications of guarantees the Foundational
design already provides — not independently written from scratch.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Maps to spec.md user stories (US1–US4)
- File paths are exact and match plan.md's Project Structure

## Path Conventions

Single Rust binary crate at repository root: `src/`, `tests/` (per plan.md — no frontend/backend
split applies; extends the existing `001-media-transcription` crate, not a new project).

---

## Phase 1: Setup

**Purpose**: Add the one new dependency this feature needs

- [x] T001 Add `console = "0.16.4"` as a direct dependency in `Cargo.toml`, matching the version
  already resolved transitively via `indicatif` in `Cargo.lock` (research.md §2) — adds zero new
  entries to the dependency tree. Run `cargo build` to confirm `Cargo.lock` is unchanged apart from
  `console` moving from transitive to direct.

**Checkpoint**: `cargo build` succeeds with the new direct dependency; no other code changes yet.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The shared progress-reporting abstraction every user story builds on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T002 [P] Implement `is_interactive() -> bool` in new `src/progress.rs`, using
  `console::Term::stderr().is_term() && !console::is_dumb()` (research.md §2-3) — the single
  shared predicate every phase uses to decide animated-bar vs. plain-milestone rendering. Unit
  test it indirectly is not possible (it reads real process state), so cover it via the
  integration-style contract tests in later phases instead; this task itself just implements the
  function.
- [x] T003 [P] Add `--no-progress` flag (boolean, default off) and `PARA_NO_PROGRESS` env var
  override to the `Cli` derive struct in `src/main.rs`, following the existing `PARA_*` flag/env
  pattern (contracts/cli-interface.md). Corrected during implementation: clap's `env` attribute on
  a plain `bool` field only accepts literal `"true"`/`"false"` and hard-errors on other values
  (e.g. `PARA_NO_PROGRESS=1` failed with "invalid value '1' ... possible values: true, false") —
  not the "any non-empty value" behavior contracts/cli-interface.md documents. Fixed by not using
  clap's `env` on this field at all; `run()` reads `std::env::var("PARA_NO_PROGRESS")` manually and
  treats any non-empty value as true, matching the documented contract exactly.
- [x] T004 Define `ProgressPhase` and a `TranscriptionProgress` handle struct in `src/progress.rs`
  (data-model.md): constructed once per run via `TranscriptionProgress::new(suppressed: bool)`.
  When `suppressed` is true, every phase method is a no-op. When false, methods branch internally
  on `is_interactive()` (T002): interactive → a real `indicatif::ProgressBar`/spinner targeting
  stderr; non-interactive → a plain `eprintln!` milestone line, never both, never neither. This
  task defines the struct and phase-transition method *signatures* only
  (`start_reading_stdin`/`update_bytes_read`/`finish_reading_stdin`,
  `start_model_loading`/`finish_model_loading`,
  `start_transcription`/`advance_chunk`/`finish_transcription`) — bodies are filled in by the user
  story that owns each phase (depends on: T002). Implemented together with full bodies (T008,
  T014, T015) in one pass rather than stubs-then-fill, given how tightly coupled the phases are
  (see the Organization note above) — more efficient than a strict multi-pass approach here.
- [x] T005 Wire `run()` in `src/main.rs` to construct one `TranscriptionProgress` handle from the
  `--no-progress`/`PARA_NO_PROGRESS` value (T003) before any other work begins, and thread it as a
  parameter through to wherever `audio.rs`/`engine.rs` will need it (depends on: T003, T004)

**Checkpoint**: `src/progress.rs` compiles with the full method surface defined (bodies may be
`todo!()`-free stubs that do nothing yet); `is_interactive()` and the suppression branch exist and
are ready for each user story to fill in its own phase's rendering.

---

## Phase 3: User Story 1 - See progress during a long transcription (Priority: P1) 🎯 MVP

**Goal**: A determinate progress bar (audio-milliseconds processed / total, research.md §5) with
an adaptive ETA (research.md §4) during the transcription phase, for both `-i` and piped-stdin
input alike — the core value this feature exists to deliver.

**Independent Test**: Run para against a long recording via both `-i` and piped stdin; confirm a
visibly updating progress indicator appears on stderr in both cases, advances per chunk, and
stdout remains exactly the transcript.

### Tests for User Story 1

- [x] T006 [P] [US1] Unit tests for `TranscriptionProgress`'s transcription-phase rendering logic
  given synthetic chunk-progress events (chunk index/total, chunk duration) — assert the
  interactive path advances a determinate bar by the correct audio-millisecond amount and the
  non-interactive path emits one plain milestone line per chunk, in `src/progress.rs`. 4 tests
  added (`suppressed_never_creates_a_bar`,
  `interactive_transcription_reaches_full_length_after_encode_and_decode_halves`,
  `interactive_mode_builds_a_real_bar_for_model_loading_and_stdin`,
  `non_interactive_mode_never_builds_a_bar`).
- [x] T007 [P] [US1] Contract test: while a `--format text` run's transcription phase is reporting
  progress, stdout remains byte-for-byte the transcript with nothing else ever written to it, in
  `tests/contract/test_progress_stdout_untouched.rs` (add its `mod` declaration to
  `tests/contract.rs`, gated behind `integration` since it needs a real cached model). Also
  verified manually via a real pty (`script`): stdout stays exactly the transcript even during a
  fully-animated interactive run with a live bar and spinner.

### Implementation for User Story 1

- [x] T008 [US1] Implement `start_transcription(total_duration_secs: f64)` /
  `advance_chunk(chunk_duration_secs: f64)` / `finish_transcription()` bodies in
  `src/progress.rs`: convert to audio-milliseconds (research.md §5) for the indicatif
  `ProgressBar`'s length/position, use indicatif's built-in `eta()`/`{eta}` template (research.md
  §4, no hand-rolled ETA math), and emit the non-interactive fallback as one plain line per chunk
  (depends on: T004, T006). **Scope correction found during implementation**: crediting a chunk's
  full duration when its *encode* completes would make the bar read "done" long before decode
  (often the slower half, especially TDT's autoregressive loop) actually finishes, since
  `encode_chunked` builds all chunks' encoder output before `decode_tdt`/`decode_ctc` decode them
  as a wholly separate pass. Split into two methods instead: `advance_encoded` (half credit, silent
  in non-interactive mode) and `advance_decoded` (remaining half credit; this is where the
  non-interactive plain milestone is emitted, since a chunk isn't actually done until both passes
  finish). Verified live via a real pty: the bar visibly moves 0%→50%→100% per chunk with a real
  adaptive ETA, not a premature jump to 100% after encode alone.
- [x] T009 [US1] Replace the bare `eprintln!("transcribing chunk {} of {total}", ...)` calls in
  `encode_chunked`/`encode_chunked_ctc` (`src/inference/engine.rs`) with calls into the
  `TranscriptionProgress` handle's `advance_chunk` (depends on: T008). Implemented as
  `advance_encoded` per T008's correction; `encode_chunked`/`encode_chunked_ctc` now take a
  `&mut TranscriptionProgress` parameter. The corresponding `advance_decoded` half was added to
  `decode_tdt`/`decode_ctc` (`src/inference/decoder.rs`) — not originally scoped in this task, but
  necessary given T008's finding (encode-only credit was rejected), so both decoders also gained a
  `progress: &mut TranscriptionProgress` parameter and call `advance_decoded` once per chunk in
  their existing per-chunk loops (no change to the TDT decoder-state-threading logic from
  `001-media-transcription`'s research.md §17, or to `MIN_BLANK_MARGIN` CTC logic from §16).
- [x] T010 [US1] Thread the handle from `run()` (T005) through to `encode_chunked`/
  `encode_chunked_ctc`'s call sites in `src/main.rs`, calling `start_transcription`/
  `finish_transcription` around the existing chunked-encode-then-decode sequence (depends on:
  T009, T005). Also threaded through to `decode_tdt`/`decode_ctc`'s call sites per T009's
  correction.

**Checkpoint**: User Story 1 is fully functional and independently testable — the MVP. Running
para against a long recording now shows a real progress bar with ETA instead of spec 001's bare
`"chunk N of M"` line.

---

## Phase 4: User Story 2 - Get feedback immediately, even on short input (Priority: P2)

**Goal**: An indeterminate spinner during model loading (every run, regardless of length) and an
indeterminate byte-counter during stdin reading, closing the one remaining silent window before
User Story 1's bar takes over.

**Independent Test**: Run para against a short clip; confirm a brief "loading model" indicator
appears on stderr within 3 seconds, before the transcript is produced. Separately, pipe a file via
stdin and confirm a byte-count indicator appears while it's being read.

### Tests for User Story 2

- [x] T011 [P] [US2] Unit test: the model-loading phase methods emit an indeterminate spinner
  (interactive) or a single "loading model" milestone (non-interactive), in `src/progress.rs`.
  Covered by `interactive_mode_builds_a_real_bar_for_model_loading_and_stdin`.
- [x] T012 [P] [US2] Unit test: the stdin-reading phase methods emit a growing byte count
  (interactive, indeterminate) or periodic plain milestones (non-interactive), in `src/progress.rs`.
  Covered by the same test as T011 for the interactive case; non-interactive case covered by
  `non_interactive_mode_never_builds_a_bar`.
- [x] T013 [P] [US2] Contract test: running para against a short clip shows a progress-related
  stderr line within 3 seconds of starting, before the transcript appears (spec.md SC-005), in
  `tests/contract/test_progress_quick_feedback.rs` (add `mod` declaration to `tests/contract.rs`,
  gated behind `integration`). Asserts the "loading model" milestone is present rather than timing
  the exact first-line latency (not practically assertable via post-hoc `Command::output()`
  capture); also manually verified the spinner appears immediately on process start via a real pty.

### Implementation for User Story 2

- [x] T014 [P] [US2] Implement `start_model_loading()`/`finish_model_loading()` bodies in
  `src/progress.rs` (depends on: T004, T011)
- [x] T015 [P] [US2] Implement `start_reading_stdin()`/`update_bytes_read(n: u64)`/
  `finish_reading_stdin()` bodies in `src/progress.rs` (depends on: T004, T012). `update_bytes_read`
  updates the interactive spinner's message with a human-readable byte count
  (`indicatif::HumanBytes`); intentionally a no-op in non-interactive mode beyond the initial
  "reading stdin..." milestone, to avoid flooding a redirected log with one line per 64KB read.
- [x] T016 [US2] Wrap the model-session-building call sites in `run()` (`src/main.rs`) — every
  call to `engine::build_session_from_file`/`build_session_from_memory` this run needs
  (preprocessor, encoder, and for TDT models the decoder-joint network) — with
  `start_model_loading()`/`finish_model_loading()` (depends on: T014, T005). `finish_model_loading`
  is called once per `match entry.kind` arm (after the decoder-joint session for TDT, immediately
  for CTC), since only the TDT arm needs a third session built.
- [x] T017 [US2] Wrap the stdin-staging read loop in `src/audio.rs` with
  `start_reading_stdin()`/`update_bytes_read`/`finish_reading_stdin()` (depends on: T015, T005).
  `stage_stdin` previously used a single `read_to_end` call with no loop at all — research.md's
  assumption that it "reads in a loop already" was wrong (corrected below); rewrote it as an
  explicit 64KB-chunked read loop so `update_bytes_read` has something to hook into incrementally.

**Checkpoint**: User Stories 1 AND 2 both work independently. Every phase of a run now reports
progress; no run is ever silent for more than a few seconds at the very start.

---

## Phase 5: User Story 3 - Progress output stays script-safe when redirected (Priority: P3)

**Goal**: Confirm, across every phase added by US1/US2, that non-interactive stderr (redirected,
or `TERM` unset/`dumb`) produces only plain newline-terminated text — the guarantee Foundational's
`is_interactive()` branch (T002) was built to provide from the start.

**Independent Test**: Run para with stderr redirected to a file; confirm the file contains only
plain, readable text with no animation/cursor-control characters, across the stdin-read,
model-load, and transcribing phases alike.

### Tests for User Story 3

- [x] T018 [P] [US3] Contract test: with stderr redirected to a file (non-terminal), a full run
  (stdin input, to exercise all three phases) produces only plain newline-terminated lines with no
  ANSI/cursor-control bytes anywhere in the captured output, and stdout is still exactly the
  transcript, in `tests/contract/test_progress_non_interactive.rs` (add `mod` declaration, gated
  behind `integration`)
- [x] T019 [P] [US3] Contract test: with `TERM=dumb` set and stderr attached to a real pty (not
  merely redirected), behavior matches the non-interactive case above (research.md §2 — an unset
  or `dumb` `TERM` is treated the same as a non-terminal even when a real terminal device is
  attached). **Verified manually rather than automated**: `std::process::Command` can't attach a
  real pty without a new dependency, and this sub-case exercises the same `is_interactive()` check
  (research.md §3) T018 already covers the other half of. Ran
  `TERM=dumb script -q out.txt para -i hello.wav` and confirmed the captured output showed the same
  plain-line behavior as T018 (no ANSI escape bytes, `^[` count of 0) — documented in
  `test_progress_non_interactive.rs`'s module doc comment rather than a separate
  `test_progress_dumb_terminal.rs` file.

### Implementation for User Story 3

- [x] T020 [US3] If T018/T019 surface any gap (e.g., a phase that emits a partial ANSI sequence
  before falling back, or a spinner tick that isn't fully suppressed), fix it in `src/progress.rs`
  — expected to be a light or empty task given Foundational's design, but not skipped without
  running the tests first (depends on: T018, T019). No gap found — both the piped-non-tty
  (`Command::output()`) and `TERM=dumb`-with-real-pty cases produced clean plain-text output with
  zero ANSI bytes on the first try.

**Checkpoint**: All three of Constitution III's guarantees (stdout untouched, no leaked control
codes, `NO_COLOR`/`TERM=dumb` handled per research.md §2) are verified end-to-end, not just assumed
from the Foundational design.

---

## Phase 6: User Story 4 - Suppress progress output entirely (Priority: P4)

**Goal**: `--no-progress`/`PARA_NO_PROGRESS` produces zero progress-related stderr output across
every phase, while errors remain unaffected.

**Independent Test**: Run para with `--no-progress` against a long recording; confirm no
progress-related stderr output appears at any point, and that an error case (e.g., a missing input
file) still reports its usual specific error message.

### Tests for User Story 4

- [x] T021 [P] [US4] Contract test: `--no-progress` produces zero progress-related stderr output
  across all three phases on a long recording, in `tests/contract/test_progress_suppressed.rs`
  (add `mod` declaration, gated behind `integration`)
- [x] T022 [P] [US4] Contract test: `PARA_NO_PROGRESS` env var has the same suppression effect as
  the flag, in `tests/contract/test_progress_env_var.rs` (add `mod` declaration, gated behind
  `integration`). Specifically tests `PARA_NO_PROGRESS=1` — the value that surfaced T003's clap
  bool+env bug — not just `=true`.
- [x] T023 [P] [US4] Contract test: with `--no-progress` set, an error case (e.g., input file not
  found) still produces its usual specific stderr message and non-zero exit — suppression affects
  only progress, never errors (FR-009), in `tests/contract/test_progress_suppressed_errors.rs`
  (add `mod` declaration; not gated behind `integration`, since it doesn't need a cached model)

### Implementation for User Story 4

- [x] T024 [US4] Verification pass: confirm every phase method added by T008/T014/T015 correctly
  short-circuits when `TranscriptionProgress` was constructed with `suppressed: true` (T004's
  no-op design) — expected to already be correct by construction; fix any method found emitting
  output anyway (depends on: T021, T022, T023). No gap found in the no-op design itself, but this
  pass is exactly what surfaced T003's real bug: `--no-progress` (the flag) worked correctly, but
  `PARA_NO_PROGRESS=1` (the env var with a non-`true`/`false` value) hard-errored instead of
  suppressing — fixed as described in T003.

**Checkpoint**: All four user stories are independently functional and verified.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, performance verification, and final end-to-end validation

- [x] T025 [P] Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`; fix any
  findings. Clean on first run (including with `--features integration`); `cargo fmt` applied a
  few mechanical reformats (line-wrapping, `mod` declaration ordering in `tests/contract.rs`), no
  clippy findings at all.
- [x] T026 [P] Update `README.md`: document `--no-progress`/`PARA_NO_PROGRESS`, and revise the
  existing stderr-contents description to reflect the richer progress output (replacing the old
  bare "chunk N of M" description) and its non-interactive plain-text fallback
- [x] T027 Empirically measure wall-clock time for the same long recording with progress enabled
  vs. `--no-progress` (FR-013/SC-007's qualitative "not perceptibly slower" claim) — confirm no
  measurable difference; if a real difference is found, document it and address before closing
  this task (do not silently accept a regression against a Clarification-level requirement). Timed
  a 62s CTC clip 3x each way: with-progress runs averaged ~5.6-7.1s, without-progress ~6.0s — the
  two sets overlap well within normal run-to-run noise, no consistent direction let alone a
  perceptible one. Separately investigated a real, pre-existing memory-growth characteristic
  (RSS climbing to ~2.7GB during a ~60s CTC run) surfaced while measuring this — confirmed via
  `git stash` that it occurs identically with and without this feature's code, so it's an existing
  ONNX Runtime CPU arena-allocator characteristic (research.md, not this feature's concern), not a
  regression introduced here.
- [x] T028 Run every scenario in quickstart.md end-to-end against a real release build (all four
  user stories, the `TERM=dumb` edge case, the closed-stderr edge case for FR-012). All verified
  manually via real pty sessions (`script`) and direct process capture: US1 (bar advances
  0%→50%→100% per chunk with adaptive ETA, both `-i` and stdin), US2 (model-load spinner visible
  immediately; stdin byte-counter shows live `HumanBytes` growth), US3 (non-interactive and
  `TERM=dumb`-with-real-pty both produce clean plain text, zero ANSI bytes), US4 (`--no-progress`
  and `PARA_NO_PROGRESS` both fully silent; errors still reported).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Setup — BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational only — this is the MVP
- **User Story 2 (Phase 4)**: Depends on Foundational; independent of US1's code (different
  methods, different call sites) though both extend the same `src/progress.rs` file
- **User Story 3 (Phase 5)**: Depends on Foundational; exercises US1's and US2's phases, so run
  after both for full coverage, though its tests could partially run against US1 alone
- **User Story 4 (Phase 6)**: Depends on Foundational; exercises US1's and US2's phases the same
  way US3 does
- **Polish (Phase 7)**: Depends on all four user stories being complete

### Within Each Phase

- Tests are written before their corresponding implementation tasks and MUST fail first
- `src/progress.rs` tasks across different stories touch the same file — T008 (US1), T014/T015
  (US2), and T020/T024 (US3/US4 fixes) should land sequentially even though nominally in different
  story phases, to avoid merge conflicts on the same file
- `src/main.rs` wiring tasks (T005, T016) are sequential (same file)

### Parallel Opportunities

- Foundational: T002, T003 in parallel (different files); T004 follows T002, T005 follows T003+T004
- US1: T006, T007 (tests) in parallel
- US2: T011, T012, T013 (tests) in parallel; T014, T015 (implementation) in parallel (different
  methods, though same file — coordinate if worked on simultaneously by different contributors)
- US3: T018, T019 in parallel
- US4: T021, T022, T023 in parallel
- Polish: T025, T026 in parallel; T027, T028 run last after everything else

## Parallel Example: Foundational Phase

```bash
Task: "Implement is_interactive() in src/progress.rs"                              # T002
Task: "Add --no-progress flag and PARA_NO_PROGRESS env var in src/main.rs"        # T003
```

## Parallel Example: User Story 2

```bash
Task: "Unit test: model-loading phase spinner/milestone in src/progress.rs"        # T011
Task: "Unit test: stdin byte-counter phase in src/progress.rs"                     # T012
Task: "Contract test: quick feedback within 3s in tests/contract/"                # T013
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL — blocks everything)
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: run quickstart.md's US1 section against a real build
5. This is a shippable MVP: a real progress bar with ETA during long transcriptions, both input
   methods

### Incremental Delivery

1. Setup + Foundational → nothing user-visible yet, but the shared abstraction is real and tested
2. User Story 1 → MVP: transcription progress bar (deploy/demo)
3. User Story 2 → model-load and stdin-read feedback (deploy/demo)
4. User Story 3 → script-safety guarantee verified (deploy/demo)
5. User Story 4 → suppression flag (deploy/demo)
6. Polish → README, lint-clean, performance check, full quickstart pass

### Team Strategy

Once Foundational is done, US1 and US2 touch different phase methods and different call sites
(`engine.rs`'s chunk loop vs. `main.rs`'s session-building calls and `audio.rs`'s stdin loop) and
can be split across contributors with light coordination on `src/progress.rs`. US3 and US4 are
best done after both, since they verify guarantees across all phases at once.

---

## Notes

- [P] tasks = different files, no dependency on an incomplete task
- [Story] label maps each task to its user story for traceability
- Every FR-0xx / SC-00x reference above ties a task back to spec.md; every research.md §N
  reference flags a decision already verified against real crate source, not to be silently
  redone or second-guessed during implementation (Constitution Principle V)
- Commit after each task or logical group
- Stop at any checkpoint to validate a story independently
