# Phase 1 Data Model: Native CoreML Backend and Transcript Polish

No change to any output-facing entity's *shape* — `Transcript`, `Segment`, and the SRT/JSON output
formats are unchanged (FR-009, FR-010). This feature adds new internal entities for the inference
backend, and changes how `Transcript.text` is assembled from existing `Segment` data.

## CoreML Backend (new)

| Concept | Description |
|---|---|
| `CoreMlBundle` | A loaded `.mlmodelc` model (via `objc2-core-ml`'s `MLModel`), analogous to `engine.rs`'s use of `ort::Session` today. One per Preprocessor/Encoder/Decoder/Joint (TDT) or Preprocessor/Encoder (CTC). |
| `CoreMlModelSet` | The group of `CoreMlBundle`s one model needs to run end-to-end — mirrors how `engine.rs` currently threads an encoder `Session` and a decoder/joint `Session` together, but with CoreML's 3-way TDT split (Encoder / Decoder(prediction network) / Joint) instead of ONNX's combined `decoder_joint-model.onnx`. |
| `ModelBackend` | Which of the two inference paths (native CoreML vs. ONNX Runtime) a given model resolves to, decided once at model-registry lookup time based on real conversion availability (research.md §1) — never decided per-request, never silently falls back at runtime if CoreML fails after being selected (Constitution Principle IV). |

`Device::Auto`'s existing dispatch (`src/inference/mod.rs`) now resolves to `ModelBackend::Coreml`
for any model with a real conversion (all three of para's current models) and
`ModelBackend::OnnxCpu` otherwise. `Device::Cpu` and `Device::Coreml` (the pre-existing ONNX Runtime
CoreML execution provider opt-in) are unaffected — they always resolve to `ModelBackend::OnnxCpu`
via the existing `engine.rs` path, exactly as today.

## Sliding-Window Chunking (new, CoreML path only)

| Concept | Description |
|---|---|
| Chunk window | Fixed ~15s span fed to the CoreML encoder (its actual, static input shape — research.md §3), analogous to `engine.rs`'s `CHUNK_SECONDS`/`SINGLE_PASS_THRESHOLD_SECONDS` but a hard model constraint here, not a tunable choice. |
| Stride | Distance between consecutive window starts — smaller than the window size, so consecutive windows overlap in source audio (research.md §3), unlike `engine.rs`'s `chunk_ranges`, whose windows are strictly non-overlapping. |
| Mel-context prepend | A small (~80ms) slice of preceding audio fed to the encoder purely for convolutional context, mirroring FluidAudio's own documented fix for degenerate encoding at a window's first frames. |
| Token-level dedup | Reconciles overlapping windows' decoded tokens by sequence-matching rather than the frame-count-based `trim_frames` approach `engine.rs` uses for its own (non-overlapping-by-design) chunking — chosen because CoreML's windows genuinely overlap in content, not just in encoder-context padding. |

This is a new, separate module (`coreml_chunking.rs`) — it does not replace or modify `engine.rs`'s
existing `chunk_ranges`/`encode_chunked`/`trim_frames`, which remain exactly as 003 left them for
the ONNX Runtime path.

## Transcript Text Assembly (changed)

| Concept | Current | This feature |
|---|---|---|
| Filler words | Every token the model decodes (including "um", "uh", disfluencies) is kept in `Transcript.text` | Removed via a small, self-contained rule (research.md §4) before assembly — the underlying `Segment`s' token data is otherwise unaffected |
| Paragraph structure | All segments joined into one continuous `Transcript.text` string with spaces | Joined with a paragraph break wherever the existing `Segment`-to-`Segment` gap (already computed by `decoder.rs`'s `group_into_segments`/`SEGMENT_GAP_SECONDS`) is large — no new pause-detection logic, just using data that already exists |
| Numbers/acronyms | Rendered exactly as the model's vocabulary produced them (e.g., "A one hundred", "S C P") | A small, rule-based pass normalizes confidently-recognizable spoken-number and acronym patterns (research.md §4) — anything not confidently matched is left unchanged, never guessed |

`Segment.start`/`Segment.end` (used by SRT/JSON output) are untouched by all three of the above —
they operate purely on how `Transcript.text` is built from the same segments, per FR-009.
