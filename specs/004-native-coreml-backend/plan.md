# Implementation Plan: Native CoreML Backend and Transcript Polish

**Branch**: `004-native-coreml-backend` | **Date**: 2026-07-22 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/004-native-coreml-backend/spec.md`

## Summary

Direct comparison against a real VoiceInk reference transcript surfaced three gaps: peak memory
(~2.5GB vs. VoiceInk's ~500MB), speed (noticeably slower), and output polish (filler words, no
paragraphing, unnormalized numbers/acronyms) — though word-level accuracy itself is already close.
003-reduce-memory-footprint's research exhausted every ONNX Runtime configuration lever; the
remaining memory/speed gap is architectural (general-purpose ONNX Runtime session vs. Apple's
ahead-of-time-planned, Neural-Engine-accelerated native CoreML). This plan replaces the inference
backend with native CoreML — via `objc2-core-ml` calling FluidAudio's real, published `.mlmodelc`
Parakeet conversions, which exist for all three of para's current models — while ONNX Runtime CPU
remains the fallback path for any future model without one. It also fixes the output-polish gap
(filler-word removal, pause-based paragraphing, scoped number/acronym normalization) as a smaller,
independent change to the output layer. Committing to this rewrite required amending Constitution
Principle VI (2.0.0 → 3.0.0, done as part of this planning session) to distinguish genuine native
CoreML from ONNX Runtime's CoreML execution provider, which the constitution had previously (and
correctly, for what it actually measured) ruled out as the default.

## Technical Context

**Language/Version**: Rust, 2024 edition, MSRV 1.85 (unchanged)

**Primary Dependencies**: New: `objc2-core-ml` (131k+ downloads, part of the well-established
`objc2` project — chosen over two far less proven alternatives found on crates.io, see
research.md §2) for native CoreML model loading/inference; `objc2`/`objc2-foundation` as its
required companions. Existing `ort` (ONNX Runtime) remains, unchanged, for `--device coreml`
(the pre-existing CoreML *execution provider* opt-in, untouched) and `--device cpu`, and as the
fallback path for any model without a native CoreML conversion (none currently — all three of
para's models have one, research.md §1). `indicatif`, `console`, `clap`, `anyhow`, `dirs` are
unaffected.

**Storage**: Model cache gains a new artifact shape per model: `.mlmodelc` bundles (Apple's
compiled-model directory format) instead of/alongside `.onnx` files — real per-model file layouts
(Preprocessor/Encoder/Decoder/Joint for TDT; Preprocessor/Encoder for CTC, per research.md §1's
FluidAudio HuggingFace repos) to be enumerated exactly, and their checksums verified against real
sources, during implementation (Constitution Principle V) — not guessed here. Existing
`model/manager.rs` download-and-verify infrastructure is extended for this new artifact shape, not
replaced.

**Testing**: Extends the existing `#[ignore]`-gated real-model integration test pattern
(`engine.rs`'s `encode_chunked_runs_against_the_real_encoder`) for the new CoreML backend. Real
end-to-end measurement (peak RSS, wall-clock time, transcript content) against real recordings is
required for SC-001/SC-002/SC-003/SC-005 per FR-011 and this project's established practice
(001's T027, 002's T027, 003's whole investigation) — proxy or synthetic-only measurement is not
acceptable for the headline claims.

**Target Platform**: darwin/aarch64 is where this feature's new native CoreML backend runs (Apple
Silicon Macs — CoreML.framework isn't present the same way elsewhere). darwin/amd64 and
linux/amd64 continue, unaffected, via the existing ONNX Runtime CPU path — this feature does not
remove or degrade support for those targets, it adds a better path specifically for the target the
user has confirmed matters most (Constitution Principle VI, amended).

**Project Type**: Single-project CLI binary (unchanged).

**Performance Goals**: SC-001 (peak memory reduced ≥70% from the current ~2.5GB) and SC-002 (wall-
clock time reduced ≥50%) — both real, measured targets per FR-011, not assumptions; research.md
notes FluidAudio's own published numbers are about their Swift application, not a guarantee for
para's independent Rust reimplementation of the same model files.

**Constraints**:

- FR-004/FR-005: no correctness/completeness regression from the backend swap; any model without a
  real CoreML conversion must keep working via ONNX Runtime CPU, with no silent runtime fallback
  from CoreML to ONNX if CoreML itself fails (that would violate Principle IV, Fail Loud, Fail
  Fast) — the ONNX path is chosen at model-registry/config time (does this model have a real
  conversion or not), never as a runtime catch-and-retry.
- FR-009: SRT/JSON segment timing must be untouched by the filler-removal/paragraph-break changes,
  which apply only to how `Transcript.text` (plain-text output) is assembled from the same
  `Segment` data those formats already use.
- FR-010: no new CLI flags. `Device::Auto` (the existing default) internally starts choosing the
  native CoreML backend for any model that has one; `Device::Coreml` (ORT's CoreML execution
  provider) and `Device::Cpu` (ONNX Runtime CPU) keep their exact current meaning, unchanged —
  matching the constitution's amended Principle VI precisely.
- `objc2-core-ml` provides raw-but-safe framework bindings (`MLModel`, `MLMultiArray`,
  `MLDictionaryFeatureProvider`), not a high-level "session" API — a real, scoped Rust wrapper
  module must be built (research.md §2), analogous to what `engine.rs` currently provides for
  `ort::Session`.
- The CoreML encoder's fixed ~15s input window requires a sliding-window chunking scheme more
  refined than 003's frame-trim approach: stride-based overlap, a small mel-context prepend, and
  token-level deduplication across windows (research.md §3, informed by FluidAudio's own published
  design) — this is real, non-trivial algorithm work, not a config change.
- FR-008's number/acronym normalization is deliberately scoped down from FluidAudio's own full ITN
  system (which depends on a native NeMo text-normalization library) to a small, self-contained,
  rule-based normalizer with no new runtime dependency (research.md §4) — ambiguous cases are left
  alone rather than guessed.

**Scale/Scope**: No change to the "no artificial duration cap" assumption from spec 001 — the
sliding-window design must handle arbitrarily long recordings, same as today.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

**Note**: Principle VI required amendment (2.0.0 → 3.0.0) before this gate could pass — completed
during this planning session (see `.specify/memory/constitution.md`'s Sync Impact Report and
research.md §5). The check below evaluates against the amended constitution.

| Principle | Check | Status |
|---|---|---|
| I. Single Binary, No Daemon | Unaffected — still one invocation, no persistent state, no CoreML "warm daemon" the way VoiceInk runs. | PASS |
| II. Offline After Setup | New `.mlmodelc` bundles are downloaded on first use exactly like today's `.onnx` files, through the same existing download-and-verify path; no other network access introduced. | PASS |
| III. Stdout Is Sacred | Unaffected — filler-removal/paragraphing changes what text goes to stdout, not the fact that only the transcript goes there. | PASS |
| IV. Fail Loud, Fail Fast | CoreML load/prediction failures must surface immediately with a clear error, non-zero exit — no silent runtime fallback to ONNX Runtime if CoreML fails after being selected (Constraints, above). | PASS — explicit design constraint, must be honored during implementation |
| V. No Fabricated Data | New model checksums/URLs (the `.mlmodelc` bundle files) must be verified against real HuggingFace sources during implementation, never placeholdered — same discipline as every existing model entry in `model/registry.rs`. | PASS — implementation obligation, not yet executed |
| VI. Apple Silicon First-Class | This feature is the direct implementation of the just-amended principle: native CoreML is now the default inference path for any model with a real conversion (all three of para's models qualify). | PASS |
| VII. Minimal Runtime Dependencies | `objc2-core-ml` links CoreML.framework, already present on every macOS install — no new user-facing install, same shape as `ort` linking ONNX Runtime today. The scoped-down ITN (research.md §4) specifically avoids adding a new native runtime dependency a fuller ITN would have required. | PASS |
| VIII. Composability Over Features | No new CLI flags; filler-removal/paragraphing apply unconditionally (spec.md Assumptions) — no new toggle surface, no server/GUI/plugin creep. | PASS |
| Engineering Standards | `objc2-core-ml` is a well-maintained crate for the CoreML-binding piece (satisfies "prefer well-maintained crates for correctness-sensitive work"); the sliding-window/token-dedup logic is necessarily hand-rolled since no existing crate solves Parakeet-specific chunked TDT/CTC inference — this is inherent to the problem, not a case of reinventing something already solved. Every new error path (CoreML load failure, prediction failure, chunking edge cases) needs a test; library code must not panic. | PASS — implementation obligation for the last two clauses |

No unjustified violations — Complexity Tracking is intentionally empty. The one real complexity
addition (a hand-rolled sliding-window/dedup scheme) is inherent to the problem this feature exists
to solve, not an avoidable design choice, so it isn't logged as a violation requiring justification.

## Project Structure

### Documentation (this feature)

```text
specs/004-native-coreml-backend/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output — CoreML model coverage, objc2-core-ml choice,
│                         # sliding-window design, transcript-polish scoping, constitution amendment
├── data-model.md         # Phase 1 output — new CoreML backend entities, output-polish entities
├── quickstart.md        # Phase 1 output
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

No `contracts/` directory — FR-010 keeps the CLI surface (flags, model selection, output formats)
unchanged; there is no new external interface for this feature to document a contract for.

### Source Code (repository root)

```text
para/
└── src/
    ├── inference/
    │   ├── mod.rs           # Device enum unchanged in shape; Auto's dispatch logic gains a
    │   │                     # "does this model have a native CoreML conversion" check
    │   ├── coreml.rs         # NEW — objc2-core-ml wrapper: loads .mlmodelc bundles, runs
    │   │                     # Preprocessor/Encoder/Decoder/Joint predictions, analogous to
    │   │                     # engine.rs's build_session_from_file/run_encoder for ONNX
    │   ├── coreml_chunking.rs # NEW — the sliding-window scheme (stride, mel-context prepend,
    │   │                     # end-alignment, token-level dedup) informed by research.md §3;
    │   │                     # separate from engine.rs's chunk_ranges (ONNX path, unaffected)
    │   ├── engine.rs         # Unchanged — remains the ONNX Runtime path (fallback + explicit
    │   │                     # --device cpu/coreml opt-ins)
    │   └── decoder.rs        # DecoderState/autoregressive threading reused as-is by the new
    │                         # CoreML decode path (003 confirmed this part was already correct)
    ├── model/
    │   ├── registry.rs       # New per-model CoreML file entries (real file names/URLs/checksums
    │   │                     # to be verified during implementation, not guessed here)
    │   └── manager.rs        # Extended (not replaced) to handle `.mlmodelc` bundle downloads
    └── output/
        └── text.rs           # Filler-word removal + paragraph breaks at existing Segment gaps +
                              # scoped number/acronym normalization — the only output-format file
                              # touched; srt.rs/json.rs untouched (FR-009)
```

**Structure Decision**: New files are additive (`coreml.rs`, `coreml_chunking.rs`) rather than
rewriting `engine.rs` in place — the existing ONNX Runtime path stays exactly as it is for its
continuing roles (explicit `--device cpu`/`--device coreml` opt-ins, and the fallback for any
future model without a CoreML conversion), so nothing already working and tested is put at risk
by this change. `Device::Auto`'s dispatch is the only shared touch-point between the two backends.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No violations. Table intentionally empty.
