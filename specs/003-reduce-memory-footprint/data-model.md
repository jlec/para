# Phase 1 Data Model: Reduce Transcription Memory Footprint

No new persisted or user-visible entities — this feature changes an internal processing parameter,
not the shape of any data `para` produces or stores. One existing internal concept is redefined.

## Chunking (redefined)

Previously, `src/inference/engine.rs`'s `CHUNK_SECONDS` constant served two roles at once: the
threshold below which an input is processed in a single pass (no chunking at all), and the size of
each chunk once an input exceeds that threshold. This feature splits those into two separate
concerns:

| Concept | Current | This feature |
|---|---|---|
| Single-pass threshold | `CHUNK_SECONDS` (300s) — inputs at or under this are never chunked | Unchanged: `SINGLE_PASS_THRESHOLD_SECONDS` (300s) — same empirically-verified value (`001-media-transcription` research.md §6's ~400s hard ONNX cutoff, with margin), same short/medium-file behavior, no regression |
| Per-chunk size once chunking is needed | Same 300s | New, smaller `CHUNK_SECONDS` (final tuned value: 30s — research.md's Phase 0 15s figure was found during implementation to break TDT correctness; 30s cuts peak memory ~50% on a long recording with no real content loss) — only takes effect for inputs that already exceed the single-pass threshold |

`chunk_ranges` (`src/inference/engine.rs`) is the sole function that reads these two values; its
existing "single range when short enough" / "split into windows otherwise" logic is preserved,
just parameterized by two constants instead of one reused for both purposes.

## Encoder context overlap (new, TDT only — added during implementation)

Shrinking `CHUNK_SECONDS` alone caused the TDT model's transducer to drop content near chunk
boundaries (encoder had no acoustic context there — see research.md's implementation-time
correction). `encode_chunked` (TDT path) now extends each chunk's encoder input by
`CHUNK_OVERLAP_SECONDS` (5.0s) on both sides before running the encoder, then trims exactly that
much off both ends of the encoder output (`trim_frames`) before decoding — so only the frames
covering the chunk's own original range are ever decoded, but the encoder gets real context at every
boundary. `ENCODER_FRAMES_PER_SECOND` (~12.5, measured) converts the overlap duration into a frame
count. `encode_chunked_ctc` (CTC path) is unaffected — CTC was measured to tolerate small chunks with
no overlap at all.

## Relationship to existing entities (spec 001, spec 002)

- `ChunkProgressEvent` (spec 002's data-model.md) already reports per-chunk progress generically —
  it doesn't assume any particular chunk size, so more/smaller chunks on long recordings just means
  more, more-frequent progress updates, not a change to that feature's contract.
- `EncoderOutput`/`CtcOutput` (spec 001) are unchanged in shape — this feature changes how many of
  them exist per run and how large each is, not their structure.
- No change to `Transcript`, `Segment`, or any output format.
