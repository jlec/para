# Implementation Plan: Transcription Progress Indicators

**Branch**: `002-transcription-progress` | **Date**: 2026-07-16 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/002-transcription-progress/spec.md`

## Summary

Add stderr-only progress reporting across every phase of a `para` run — reading stdin (indeterminate
byte counter), loading the model (indeterminate spinner), and transcribing (determinate bar keyed to
audio-milliseconds processed, with an adaptive ETA) — for both `-i` file input and piped stdin alike,
which converge on identical behavior once input is staged and its duration known (research.md; UX
consult in spec.md's Context). Falls back to plain, unconditional milestone lines (extending spec
001's existing `"transcribing chunk N of M"` line) whenever stderr isn't an interactive terminal —
verified against indicatif's real source that its own default behavior is to go fully silent in that
case, not degrade gracefully, so this feature implements its own explicit terminal/dumb check rather
than relying on indicatif's default (research.md §1-3). A new `--no-progress`/`PARA_NO_PROGRESS`
option suppresses all of it. Supersedes spec 001's FR-023 clarification that a progress bar wasn't
required.

## Technical Context

**Language/Version**: Rust, 2024 edition, MSRV 1.85 (unchanged — this extends the existing `para` crate)

**Primary Dependencies**:

- `indicatif` (already a dependency, used today for model-download progress bars) — reused for the
  transcription-phase determinate bar and the two indeterminate spinners; no version change needed
  (0.18.6 already supports everything this feature needs, research.md §1-5)
- `console` (already present transitively via `indicatif` at 0.16.4) — promoted to a **direct**
  dependency so this feature can call `Term::stderr().is_term()` / `is_dumb()` directly (research.md
  §2-3) rather than reimplementing terminal/dumb detection by hand. Adds zero new entries to the
  dependency tree (Constitution Principle VII).

No other new dependencies. No change to `ort`, `clap`, `reqwest`, or any model-inference crate.

**Storage**: Unchanged — no persisted state; this feature is entirely in-process runtime behavior for
a single invocation (data-model.md).

**Testing**: Extends the existing `tests/contract/` suite (stderr-shape assertions for the new
interactive/non-interactive/suppressed cases) and `tests/integration.rs` (gated behind the
`integration` feature, for end-to-end runs against a real cached model). No new test infrastructure
needed.

**Target Platform**: Unchanged from spec 001 — darwin/arm64 (primary), darwin/amd64, linux/amd64. This
feature has no platform-specific behavior; terminal detection via `console` is cross-platform.

**Project Type**: Single-project CLI binary (unchanged).

**Performance Goals**: Per spec.md's Clarifications, qualitative only — progress reporting must not
be perceptibly slower than the same run without it (FR-013/SC-007); no numeric overhead budget is
committed to, consistent with this project's established practice of not inventing unverified
performance figures (Constitution Principle V).

**Constraints**:

- stderr-only; stdout must remain byte-for-byte the transcript regardless of progress-reporting
  behavior (Constitution III; FR-010).
- A run must show _some_ visible activity indicator within 3 seconds of starting, for every input
  length (spec.md SC-005).
- Non-interactive stderr (redirected, or `TERM` unset/`dumb`) must degrade to plain, newline-terminated
  text — never animation or color codes, and never silence (FR-007; this is the one place this
  feature deliberately does _not_ use indicatif's own default behavior, research.md §1).
- A failure to display progress must never change the run's exit code or fail the transcription
  itself (FR-012) — distinct from Constitution IV's "Fail Loud" principle, which governs
  correctness-relevant failures (checksum mismatch, missing ffmpeg, etc.); progress display is a
  best-effort UX affordance, not part of the tool's correctness contract, the same category as the
  existing CoreML first-compile notice or download progress bar.

**Scale/Scope**: No change to spec 001's scale assumptions (no artificial duration/size cap). This
feature only affects observability of an existing pipeline, not its capacity.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Principle                         | Check                                                                                                                                                                                                                                                                                                                                | Status                      |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------- |
| I. Single Binary, No Daemon       | No new process, thread lifecycle, or persistent state across invocations — progress state lives entirely within one `run()` call.                                                                                                                                                                                                    | PASS                        |
| II. Offline After Setup           | No network I/O introduced.                                                                                                                                                                                                                                                                                                           | PASS                        |
| III. Stdout Is Sacred             | All progress output (animated or plain-fallback) goes exclusively through stderr — either an indicatif `ProgressBar` explicitly targeting stderr, or direct `eprintln!`. Enforced by contract tests asserting stdout is byte-for-byte the transcript under every progress-reporting mode (interactive, non-interactive, suppressed). | PASS                        |
| IV. Fail Loud, Fail Fast          | Unaffected for all existing correctness-relevant failure paths. Progress-display failures specifically (e.g., unwritable stderr) are deliberately swallowed (FR-012) — justified above under Constraints as a different category (best-effort UX, not correctness), matching existing precedent (CoreML notice, download bar).       | PASS — see Constraints note |
| V. No Fabricated Data             | Every indicatif/console behavioral claim in this plan (default hiding behavior, `is_dumb()`'s exact semantics, `eta()`'s computation) was verified directly against the real vendored crate source, not assumed from training-data familiarity (research.md).                                                                        | PASS                        |
| VI. Apple Silicon First-Class     | No execution-provider or platform-specific behavior introduced.                                                                                                                                                                                                                                                                      | PASS / N/A                  |
| VII. Minimal Runtime Dependencies | `console` promoted from transitive to direct at its already-pinned version — no new dependency-tree entries, no new crate to trust.                                                                                                                                                                                                  | PASS                        |
| VIII. Composability Over Features | `--no-progress` directly serves pipeline/automation use, following the existing `--list-models`/`--refresh-model` flag pattern; no server/GUI/plugin surface added.                                                                                                                                                                  | PASS                        |
| Engineering Standards             | New fallible operations (writing progress output) must not panic and must have their failure-tolerance behavior tested; contract tests extend the existing suite rather than introducing new test infrastructure.                                                                                                                    | PASS                        |

No violations requiring justification — Complexity Tracking is intentionally empty.

## Project Structure

### Documentation (this feature)

```text
specs/002-transcription-progress/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md         # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/           # Phase 1 output (/speckit-plan command)
│   └── cli-interface.md # Additions to spec 001's CLI contract
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
para/
├── Cargo.toml                    # Add `console` as a direct dependency (already transitive)
├── src/
│   ├── main.rs                  # New `--no-progress`/`PARA_NO_PROGRESS` CLI flag; construct and
│   │                             # thread a progress handle through the run() pipeline
│   ├── progress.rs               # NEW — the progress-reporting abstraction: is_interactive() check,
│   │                             # phase transitions (acquiring input / loading model /
│   │                             # transcribing), interactive-vs-plain-fallback-vs-suppressed
│   │                             # branching, all indicatif/console usage lives here so other
│   │                             # modules stay free of presentation concerns
│   ├── audio.rs                  # Stdin staging loop reports bytes read to the progress handle
│   │                             # (FR-004) — no change to staging logic itself
│   └── inference/
│       └── engine.rs             # Model-session construction reports phase start/end (FR-003);
│                                  # encode_chunked/encode_chunked_ctc's existing per-chunk
│                                  # eprintln! (spec 001, FR-023) is replaced by a call into the
│                                  # progress handle, which itself decides animated-bar vs.
│                                  # plain-line output
└── tests/
    └── contract/                  # New tests for the stderr-shape contract (interactive /
                                    # non-interactive / suppressed), extending the existing suite
```

**Structure Decision**: No new top-level module boundary beyond a single new `src/progress.rs` —
this keeps every indicatif/console-specific concern in one place, so `audio.rs`/`engine.rs`/`main.rs`
each make one or two calls into a small, phase-oriented API rather than embedding presentation logic
themselves. Consistent with spec 001's existing module boundaries (one file per concern); no
frontend/backend split applies.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No violations. Table intentionally empty.
