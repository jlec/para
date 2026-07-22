# Implementation Plan: Reduce Transcription Memory Footprint

**Branch**: `003-reduce-memory-footprint` | **Date**: 2026-07-18 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/003-reduce-memory-footprint/spec.md`

## Summary

Peak memory during a long transcription grows with recording duration (measured: ~5.79GB on the
reported 25-minute example) rather than staying flat. Four ONNX Runtime configuration levers were
tested and ruled out (arena allocator, memory-pattern optimization, weight precision via int8,
CoreML execution provider) — none explains or fixes the growth. Direct inspection of FluidAudio's
real, published CoreML model (the same Parakeet architecture VoiceInk uses) found the actual likely
cause: its encoder runs on a **fixed 15-second window**, not a dynamic chunk of up to 300s the way
`para`'s does. Testing this directly — lowering the per-chunk processing window within the existing
ONNX Runtime architecture, no new backend — cut peak memory ~50% on a long recording with zero
change to transcription output. This plan implements that fix: separating "does this input need
chunking at all" (unchanged, 300s threshold) from "how large is each chunk once chunking starts"
(new, smaller value). A native-CoreML backend remains a real, now better-informed option but is
explicitly out of scope here — see spec.md's Clarifications.

## Technical Context

**Language/Version**: Rust, 2024 edition, MSRV 1.85 (unchanged — extends the existing `para` crate)

**Primary Dependencies**: No new dependencies. This is a parameter/logic change in
`src/inference/engine.rs` (`chunk_ranges` and the constants it reads) — `ort`, `indicatif`,
`console`, and every other existing dependency are unaffected.

**Storage**: Unchanged — no persisted state.

**Testing**: Extends the existing unit tests for `chunk_ranges` (`src/inference/engine.rs`) to cover
the two-threshold behavior; a new, real-measurement verification (not just unit tests) is required
per FR-005 — peak memory on a real long recording, before and after, following this project's
established practice (`002-transcription-progress`'s T027 did the same for its own performance
claim).

**Target Platform**: Unchanged — darwin/arm64 (primary), darwin/amd64, linux/amd64. This fix is
platform-independent (pure chunking-logic change); it doesn't touch execution-provider selection at
all, so it applies identically everywhere `para` runs today.

**Project Type**: Single-project CLI binary (unchanged).

**Performance Goals**: SC-002's 40%+ peak-memory reduction on a long recording (research.md: ~50%
measured on the case tested). No transcription-speed regression is acceptable as a side effect —
more, smaller chunks means more `session.run()` calls, each with its own fixed overhead; Phase 1
below includes verifying this doesn't meaningfully slow down long-recording transcription (FR-005
demands real, not assumed, verification either way).

**Constraints**:

- FR-004: output (text, timing, format) must be byte-for-byte unaffected by this change — this is a
  resource-usage fix only.
- Short/medium recordings (under the existing 300s single-pass threshold) must see no regression —
  research.md's Phase 0 found that naively lowering the chunk size for *all* inputs slightly hurts
  short files (extra per-call overhead with no growth problem to offset). The two-threshold design
  exists specifically to avoid this.
- The exact smaller chunk-size value needs real tuning, not just adopting the 15s value tested in
  Phase 0 research uncritically — see Phase 0 below.

**Scale/Scope**: No change to spec 001's "no artificial duration cap" assumption; this feature
makes long recordings *more* practical to run (lower peak memory), not less.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Principle | Check | Status |
|---|---|---|
| I. Single Binary, No Daemon | No change — still one invocation, no persistent state. | PASS |
| II. Offline After Setup | No network I/O introduced. | PASS |
| III. Stdout Is Sacred | Unaffected — this change is entirely in the encode/decode pipeline, nowhere near output writing. | PASS |
| IV. Fail Loud, Fail Fast | Unaffected — no new failure modes; chunk-count changes don't change error handling. | PASS |
| V. No Fabricated Data | Every claim in this plan (the growth pattern, the four ruled-out levers, the chunk-size fix, FluidAudio's actual model signature) was verified against a real build or a real downloaded model file, not assumed — research.md documents each measurement. FR-005 requires the same discipline for the final implementation. | PASS |
| VI. Apple Silicon First-Class | Unaffected by this specific fix (platform-independent). The deferred native-CoreML discussion (spec.md Clarifications) is relevant to this principle's future evolution but is explicitly out of this feature's scope. | PASS / N/A |
| VII. Minimal Runtime Dependencies | No new dependency. | PASS |
| VIII. Composability Over Features | No new flags, no new surface — an internal tuning fix. | PASS |
| Engineering Standards | `chunk_ranges`'s existing unit tests are extended to cover the new two-threshold logic; no new fallible operations are introduced (no new `Result`-returning code paths). | PASS |

No violations requiring justification — Complexity Tracking is intentionally empty.

## Project Structure

### Documentation (this feature)

```text
specs/003-reduce-memory-footprint/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output — four ruled-out levers, the chunk-size finding
├── data-model.md        # Phase 1 output — the redefined chunking concept
├── quickstart.md        # Phase 1 output
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

No `contracts/` directory — this feature has no external interface (no new CLI flags, no output
format change), so per the plan template's own guidance, that artifact is skipped.

### Source Code (repository root)

```text
para/
└── src/
    └── inference/
        └── engine.rs      # `chunk_ranges` gains a second constant; `SINGLE_PASS_THRESHOLD_SECONDS`
                            # (300s, unchanged value) governs whether chunking happens at all,
                            # `CHUNK_SECONDS` (new, smaller value — tuned in Phase 1, informed by
                            # research.md's 15s finding) governs each chunk's size once it does.
                            # `encode_chunked`/`encode_chunked_ctc` and their existing progress-
                            # reporting (spec 002) and chunk-boundary decoder-state threading
                            # (spec 001 research.md §17) are unaffected — they already iterate
                            # `chunk_ranges`'s output generically, regardless of how many chunks
                            # or how large each one is.
```

**Structure Decision**: No new files, no new module boundary — this is a small, contained change to
one existing function's parameterization in `src/inference/engine.rs`. Consistent with the
principle of surgical, minimal-footprint changes: the fix is real and measured, but it doesn't
justify restructuring anything around it.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No violations. Table intentionally empty.
