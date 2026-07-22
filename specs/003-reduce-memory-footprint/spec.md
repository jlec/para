# Feature Specification: Reduce Transcription Memory Footprint

**Feature Branch**: `003-reduce-memory-footprint`

**Created**: 2026-07-17

**Status**: Draft

**Input**: User description: "We need to reduce the memory footprint to reasonable numbers. Using this example, we are at above 6 GB of memory consumption, which is far too high. ~/tmp/07-01_Meeting_GPU_Management_Strategy_Forecasting_and_Global-China_HPC_Deployment.mp3"

## Context

A prior investigation (spec `002-transcription-progress`'s T027) found that peak memory grows
continuously throughout a transcription rather than staying roughly flat, and confirmed (via a
before/after code comparison) that this is not something either of the two most recent features
introduced — it predates both. Directly reproducing the user's example (the exact file referenced, default model and settings, a
~25-minute real meeting recording) confirms the underlying complaint precisely: peak memory
reached **5.79GB** over the course of a successful, correctly-completed transcription (exit code
0, real transcript produced) — closely matching the user's own observation of "above 6GB," for a
tool whose entire model weights are ~2.4GB on disk (a ratio of roughly 2.4x). Memory climbed
steadily during encoding, plateaued for a stretch, then rose again near the end of the run. This
spec treats that growth pattern itself, not just a single absolute number, as the problem: the
concern is that memory scales with *how long the recording is*, not that a one-time fixed cost is
too big.

## Clarifications

### Session 2026-07-18

- Q: What specific ceiling (or model-size multiple) should this tool commit to hitting? → A: Match
  VoiceInk's observed memory footprint on the same file (~200MB of growth during transcription).
  **Recorded as-is, with an important caveat applied below**: VoiceInk's number reflects a
  persistently-running, pre-warmed process measuring only the *incremental* growth during
  transcription, not a cold start; it also runs Parakeet via native CoreML, not ONNX Runtime.
  `para` is constitutionally required to be a single-invocation, no-daemon tool (Constitution
  Principle I) — it cannot pre-warm across runs the way VoiceInk does, so an exact "~200MB total"
  figure measured the same way VoiceInk's is are not directly comparable. This spec adopts
  VoiceInk's number as the qualitative aspiration and a north star for how far to push, translated
  into two things this architecture actually controls: eliminating the duration-dependent growth
  entirely (FR-001, already specified) and reducing the baseline cold-start footprint as far as
  real investigation shows is achievable — rather than committing to an unverified exact figure a
  fundamentally different architecture produced.
- Q: Given the real fix requires bypassing ONNX Runtime (a native CoreML backend), how should this
  feature be scoped? → A: Ship the smaller, proven fix now (bounding the per-call processing window
  size, found during Phase 0 research to cut peak memory ~50% on a long recording with zero
  architecture change) and defer the native-CoreML rewrite to a separate, future feature. The user
  also confirmed `para`'s actual target platform is Apple Silicon macOS only (no Linux/Intel-Mac
  support needed) — this removes the dual-backend-maintenance cost that made the CoreML rewrite
  look worse, and is worth revisiting as a well-informed follow-up now that real FluidAudio model
  numbers exist to plan against, but it's still a substantially larger undertaking than this
  feature's scope and is not attempted here.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Transcribe a long recording without excessive memory use (Priority: P1)

A user transcribes a long recording (tens of minutes) on a normal personal computer and expects
memory use to stay in the same ballpark as transcribing a short clip — not grow dramatically just
because the recording is longer.

**Why this priority**: This is the entire reason the feature exists. Memory use that scales with
recording length undermines the tool's promise of running comfortably on ordinary hardware,
independent of how long a person's recordings happen to be.

**Independent Test**: Transcribe a short clip (a few minutes) and a long recording (tens of
minutes) with the same model and measure peak memory for each; confirm the long recording's peak
is close to the short clip's, not proportionally larger.

**Acceptance Scenarios**:

1. **Given** a user transcribes a long recording, **When** the run completes, **Then** peak memory
   usage stays close to what the same model uses on a short recording, rather than growing with
   the recording's length.
2. **Given** a user transcribes the same long recording twice in a row (two separate runs), **When**
   both complete, **Then** each run's peak memory is consistent with the other — the growth is not
   random or dependent on incidental system state.

---

### User Story 2 - Memory use stays proportional to the model in use (Priority: P2)

A user picks a specific model (the tools offers a choice trading speed for accuracy) and expects
that model's memory footprint to relate sensibly to that model's own size, not balloon to many
times more than the model itself occupies on disk.

**Why this priority**: Builds on User Story 1 by giving users a concrete way to judge whether a
given run's memory use is reasonable — relative to the model they explicitly chose — rather than
relying on a single hardcoded number that might not fit every model this tool offers.

**Independent Test**: Load each of the tool's available models against the same short input and
compare each one's peak memory to that model's on-disk size; confirm the ratio is consistent and
modest across models rather than wildly different.

**Acceptance Scenarios**:

1. **Given** a user selects a specific model, **When** transcription completes, **Then** that run's
   peak memory usage is within a small, consistent multiple of that model's on-disk size.
2. **Given** two different models of different sizes, **When** each transcribes the same short
   input, **Then** the smaller model's run uses proportionally less memory, not the same absolute
   amount regardless of model size.

---

### Edge Cases

- What happens on a recording so long it would, under the old behavior, have used memory far
  beyond what's typical for this tool (e.g., multiple hours)? Memory use must not become the
  limiting factor before the existing chunked-processing duration limit does.
- What happens when the machine running `para` has limited available memory? A run should not be
  more likely to exhaust system memory on a long recording than on a short one, for the same model.
- What happens with the fastest/smallest available model versus the most accurate/largest one on
  the same long recording? Both must show the same flat-rather-than-growing memory pattern, even
  though their absolute memory levels will naturally differ by model size.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: System MUST keep peak memory usage during a transcription from growing
  proportionally with the input recording's duration — a long recording MUST NOT use dramatically
  more memory than a short one processed by the same model.
- **FR-002**: System MUST keep a given model's peak memory usage within a small, consistent
  multiple of that model's own on-disk size, across the range of input lengths this tool supports.
- **FR-003**: System MUST maintain this bounded memory behavior across every model this tool
  offers, not only the default one.
- **FR-004**: This change MUST NOT alter transcription output (text, timing, or format) — this is
  strictly a resource-usage improvement, not a behavior change to what gets produced.
- **FR-005**: Any claimed improvement MUST be verified against a real, complete transcription of a
  long recording, not a proxy or partial measurement — this project's practice throughout has been
  to measure end-to-end rather than assume an optimization holds up in practice (Constitution
  Principle V), and the memory-growth pattern here was itself only confirmed this way.

### Key Entities

- **Memory Profile**: The peak and pattern-over-time of memory a single transcription run
  consumes; characterized by how it relates to input duration (should be flat) and to the loaded
  model's on-disk size (should be a small, consistent multiple).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Transcribing a 25-minute recording uses peak memory within 25% of what the same
  model uses transcribing a 2-minute recording — not several times more.
- **SC-002**: Peak memory for the default model on a long recording is reduced by at least 40% from
  the measured ~5.79GB baseline (plan.md's research found a real, verified ~50% reduction on a
  comparable long recording via bounding the per-call processing window, with no architecture
  change and no loss of output correctness). Full parity with VoiceInk's ~200MB *incremental*
  figure is not achievable within this feature's scope — that would require replacing this tool's
  ONNX Runtime-based inference with a native CoreML backend, a substantially larger,
  Apple-Silicon-only undertaking explicitly deferred to a future feature (see Clarifications).
- **SC-003**: This holds across all of the tool's available models, not just the one used in the
  reported example.
- **SC-004**: A user can transcribe a multi-hour recording on a machine with a typical amount of
  memory for a personal computer without memory becoming the reason the run fails, when it
  otherwise would have completed successfully on a shorter recording.

## Assumptions

- The reported measurement (a ~25-minute real recording, default model, default settings) is
  representative of the problem — the concern is the growth pattern relative to duration, not a
  one-off anomaly specific to that file.
- No change to transcription accuracy, timing precision, or supported input formats is in scope —
  this is purely about resource usage.
- The three models this tool offers differ substantially in on-disk size; "reasonable" memory use
  is expected to scale with the chosen model's size, not be a single fixed number applied
  regardless of which model is active.
