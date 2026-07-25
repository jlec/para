# Feature Specification: Native CoreML Backend and Transcript Polish

**Feature Branch**: `004-native-coreml-backend`

**Created**: 2026-07-22

**Status**: Draft

**Input**: User description: "Replace the ONNX Runtime CPU inference pipeline with a native CoreML backend (using FluidAudio's real, published Parakeet CoreML models) to close the memory and speed gap versus VoiceInk (currently ~2.5GB and much slower than VoiceInk's ~500MB warm footprint), since prior investigation (003-reduce-memory-footprint research.md) ruled out every ONNX Runtime configuration lever and found the gap is architectural. Also fold in output quality/formatting improvements found by comparing against a real VoiceInk reference transcript: suppress filler words (um/uh) and add paragraph breaks at existing pause-based segment boundaries, plus basic number/acronym normalization. The user has confirmed para's target platform is macOS on Apple Silicon only."

## Context

A direct comparison against a real VoiceInk reference transcript (same source recording, same underlying Parakeet model family) found three gaps:

1. **Memory**: para peaks around 2.5GB on a long recording (after 003-reduce-memory-footprint's chunk-size fix, itself a ~50% cut from a 5.79GB baseline); VoiceInk's incremental footprint is closer to 500MB. 003's research exhausted every ONNX Runtime configuration lever (arena allocator, memory pattern, weight precision, ORT's own CoreML execution provider) without closing this gap — it concluded the remaining gap is architectural: VoiceInk runs Parakeet through native CoreML with Apple's own ahead-of-time memory planning, not through ONNX Runtime's general-purpose session.
2. **Speed**: para takes noticeably longer than VoiceInk to transcribe the same recording, consistent with running on general-purpose CPU/ONNX Runtime rather than the Apple Neural Engine via native CoreML.
3. **Output polish**: word-for-word comparison found para's actual transcription accuracy is close to VoiceInk's (differences are ~2-3% of words, mostly minor). The visible quality gap is presentation: para keeps every filler word ("um", "uh") the model hears, outputs one unbroken block of text with no paragraph breaks, and doesn't normalize spoken numbers/acronyms ("A one hundred" vs "A100"). VoiceInk visibly does all three.

This feature commits to the native-CoreML rewrite that 003 explicitly deferred, now that the user has confirmed para's target platform is macOS on Apple Silicon only — removing the dual-backend-maintenance cost that previously made this rewrite look disproportionate to its benefit — and folds in the transcript-polish fixes discovered during the same investigation.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Transcription uses a small, bounded amount of memory (Priority: P1)

A user transcribes a recording and expects memory use to stay small and predictable, in the same range as other well-behaved local tools on their Mac — not multiple gigabytes for what is, functionally, a single audio-to-text conversion.

**Why this priority**: This is the primary reason this feature exists — the 2.5GB current footprint remains far higher than a comparable native tool needs, even after 003's fix, because the ONNX Runtime path has a large, fixed architectural floor.

**Independent Test**: Transcribe the same long recording used in prior investigations before and after this change; confirm peak memory drops substantially and is now a small, low multiple of the model's on-disk size rather than several times more.

**Acceptance Scenarios**:

1. **Given** a user transcribes a long recording, **When** the run completes, **Then** peak memory usage is dramatically lower than the current ONNX-Runtime-based footprint, and much closer to a comparable native macOS transcription tool's footprint than to para's own prior numbers.
2. **Given** a user transcribes recordings with each of the tool's available models, **When** each run completes, **Then** every model shows this same, dramatically lower footprint — not just the default model.

---

### User Story 2 - Transcription completes noticeably faster (Priority: P1)

A user transcribing a recording expects it to finish quickly, taking advantage of the Mac's dedicated ML hardware rather than running slower, general-purpose computation.

**Why this priority**: Speed and memory both stem from the same architectural gap (general-purpose ONNX Runtime vs. native, hardware-accelerated CoreML) and are addressed by the same underlying change — this is equally central to the feature's purpose.

**Independent Test**: Transcribe the same recording before and after this change on the same machine; confirm wall-clock time drops substantially, without any loss of transcription completeness.

**Acceptance Scenarios**:

1. **Given** a user transcribes a recording, **When** the run completes, **Then** it finishes noticeably faster than the current implementation on the same hardware.
2. **Given** a user transcribes recordings of different lengths, **When** each completes, **Then** the speed improvement holds across short and long recordings alike, not just one case.

---

### User Story 3 - Transcript text reads cleanly, without filler words or run-on blocks (Priority: P2)

A user reading a finished transcript expects it to read like a cleaned-up document — organized into natural paragraphs, without meaningless filler sounds cluttering every sentence, and with numbers and technical acronyms written the way a person would write them.

**Why this priority**: This closes the visible "quality" gap users actually perceive, even though it's independent of the memory/speed architecture change — it's a real, valued improvement in its own right, bundled into this feature at the user's direction.

**Independent Test**: Transcribe a recording containing natural speech disfluencies and multiple pauses; confirm the output has no filler words, is broken into multiple paragraphs at natural pauses, and common spoken numbers/acronyms appear in conventional written form.

**Acceptance Scenarios**:

1. **Given** a recording where the speaker says "um" and "uh" repeatedly, **When** transcription completes, **Then** the output text contains none of these filler words.
2. **Given** a recording with multiple natural pauses between topics, **When** transcription completes, **Then** the output text is broken into separate paragraphs at those pauses rather than being one continuous block.
3. **Given** a recording where a speaker says a number or acronym letter-by-letter or spelled out (e.g., "A one hundred", "S C P"), **When** transcription completes, **Then** the output uses the conventional written form where one can be determined with confidence (e.g., "A100", "SCP").

---

### Edge Cases

- What happens on the very first run after this change, if the CoreML model needs an on-device compile step? This MUST behave consistently with para's existing first-run CoreML notice (already implemented for the opt-in `--device coreml` path) — a clear one-time message, not a silent long pause.
- What happens for a model that doesn't have a real, published CoreML conversion available? The existing ONNX Runtime path MUST remain available as a fallback for that specific model, so no currently-supported model becomes unusable — but this is scoped per-model, not an excuse to skip the change for models that do have a conversion available.
- What happens when filler-word removal or paragraph-break logic would otherwise remove content that changes the transcript's meaning (e.g., a legitimate word that resembles a filler)? The system MUST only remove tokens that are unambiguously non-lexical disfluencies, never guess on ambiguous cases.
- What happens to the SRT and JSON output formats, which carry timing information? Paragraph/filler changes to the plain-text output MUST NOT corrupt segment timing in SRT/JSON outputs — those formats' existing per-segment structure is unaffected by this feature.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: System MUST run transcription inference through a native CoreML backend on Apple Silicon Macs, for at least the default model, rather than exclusively through the existing ONNX Runtime pipeline.
- **FR-002**: Peak memory usage during transcription MUST be dramatically reduced from the current ONNX-Runtime-based baseline — substantially closer to a comparable native macOS tool's footprint than to para's prior numbers.
- **FR-003**: Transcription wall-clock time MUST be substantially reduced from the current baseline on the same hardware.
- **FR-004**: This change MUST NOT reduce transcription completeness or correctness relative to the current implementation — real spoken content must not be dropped or degraded to achieve the memory/speed improvement.
- **FR-005**: Every model for which a real, published CoreML conversion exists MUST use the native CoreML backend; any model without one MUST continue to function correctly via the existing ONNX Runtime path, with no user-visible failure.
- **FR-006**: Output text MUST have non-lexical filler words (e.g., "um", "uh") removed.
- **FR-007**: Output text MUST be broken into paragraphs at natural pauses in the recording, using the pause information the system already computes for segment timing.
- **FR-008**: Output text MUST render common spoken-form numbers and well-known technical acronyms in their conventional written form where this can be determined with confidence, without inventing or guessing values that weren't actually said.
- **FR-009**: SRT and JSON output formats' segment timing MUST remain accurate and unaffected by the filler-removal and paragraph-formatting changes, which apply to human-readable text presentation only.
- **FR-010**: The CLI surface (flags, model selection, output formats) MUST remain unchanged for existing users — this is an internal backend and output-formatting change, not a new interface.
- **FR-011**: Any claimed memory, speed, or quality improvement MUST be verified against real, complete transcriptions of real recordings — not a proxy, partial measurement, or assumption.

### Key Entities

- **Inference Backend**: The component responsible for running the acoustic model and producing recognized tokens; now has two implementations (native CoreML, existing ONNX Runtime) selected per-model based on CoreML conversion availability.
- **Transcript Text**: The human-readable output; gains paragraph structure and loses filler-word tokens compared to today, while the underlying `Segment` timing data (used for SRT/JSON) is unaffected.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Peak memory for the default model on a long recording is reduced by at least 70% from the current ~2.5GB figure (003's post-fix baseline) — a substantially larger cut than 003 achieved, reflecting the architectural nature of this change.
- **SC-002**: Wall-clock transcription time for the default model on a long recording is reduced by at least 50% from the current baseline.
- **SC-003**: A transcript of a recording with natural disfluencies contains no filler words, is organized into multiple paragraphs reflecting the recording's natural pauses, and commonly-recognized spoken numbers/acronyms appear in conventional written form — verified by direct comparison against a real reference recording, not a synthetic example.
- **SC-004**: These improvements hold across every model the tool offers that has an available CoreML conversion, not only the default model.
- **SC-005**: No regression in transcription completeness: a transcript produced after this change contains no less real spoken content than one produced before it, for the same recording.

## Assumptions

- FluidAudio's published CoreML Parakeet models (confirmed to exist for the default TDT model during 003's research) are a legitimate, real starting point for at least one model's conversion; whether equivalent conversions exist or can be produced for this tool's other models is a Phase 0 research question for planning, not assumed here.
- Filler-word removal and paragraph formatting apply unconditionally to the plain-text output (no new flag to toggle them off) — matching VoiceInk's own default behavior and this project's preference for a simple, opinionated CLI surface (Constitution Principle VIII, Composability Over Features) unless Phase 0 research surfaces a reason a flag is actually needed.
- Exact parity with VoiceInk's absolute ~500MB figure is not required — VoiceInk is a warm, persistently-running process measuring only incremental growth, while para remains a single-invocation, no-daemon tool (Constitution Principle I) that cannot pre-warm the way VoiceInk does; this spec targets a dramatic, real reduction (SC-001) rather than committing to matching a figure produced by a fundamentally different architecture, following the same reasoning 003 already established for this exact comparison.
- The existing chunking/progress-reporting infrastructure (specs 001-003) is assumed to be adaptable to whatever windowing the chosen native CoreML models require, rather than needing to be discarded.
