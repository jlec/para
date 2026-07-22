# Phase 1 Data Model: Transcription Progress Indicators

This feature adds no persisted or cross-invocation data — everything here is in-process runtime
state for a single `para` invocation, alongside spec 001's existing `InputMedia`/`Transcript` types
(unmodified by this feature; `InputMedia.duration_secs` is *consumed*, not changed).

## ProgressPhase

Which stage of a run the user should see distinct feedback for.

| Value | Entered when | Exited when |
|---|---|---|
| `AcquiringInput` | A run begins; for stdin, spans the read-to-temp-file loop; for `-i`, effectively instantaneous | Input is staged on disk and its format/duration have been probed |
| `LoadingModel` | Immediately after input acquisition | Every ONNX session this run needs (preprocessor, encoder, and for TDT models the decoder-joint network) has been built |
| `Transcribing` | Immediately after model loading | The last chunk's decode completes |

Phases are sequential and non-overlapping for a single run — this feature does not introduce any
concurrency between them.

## ProgressIndicator

What's actually shown for the current phase; constructed once per run and reused/reconfigured
across phase transitions rather than one instance per phase.

| Field | Type | Notes |
|---|---|---|
| `mode` | enum `Interactive` \| `Suppressed` | `Suppressed` when either the run's stderr is not an interactive terminal (research.md §3), or the user passed the suppression flag (FR-008) — both collapse to the same no-animated-output behavior, but only the terminal case still emits plain milestone lines (FR-007); the explicit-suppression case emits nothing (FR-008/SC-004) |
| `determinacy` | enum `Determinate` \| `Indeterminate` | `Indeterminate` only during `AcquiringInput` for stdin (total byte count unknown) and during `LoadingModel` (no meaningful "total" to a model load); `Determinate` during `Transcribing` once total audio duration is known, and during `AcquiringInput` for `-i` (file size known via stat, though this phase is near-instantaneous for file input per research.md) |
| `unit` | enum `Bytes` \| `AudioMilliseconds` \| `None` | `Bytes` for the indeterminate stdin-read counter; `AudioMilliseconds` for the `Transcribing` phase (research.md §5); `None` for a bare spinner (`LoadingModel`) |
| `total` | `Option<u64>` | Known upfront for `Transcribing` (`duration_secs * 1000`, rounded); absent for the two indeterminate cases |

## ChunkProgressEvent

One unit of forward progress during the `Transcribing` phase — corresponds exactly to one
completed chunk from the existing `encode_chunked`/`encode_chunked_ctc` chunking behavior (spec
001, FR-023); this feature does not introduce any finer-grained event (per the resolved Assumption
on per-chunk-only granularity).

| Field | Type | Notes |
|---|---|---|
| `chunk_index` | `usize` | 1-based, matches the existing `"chunk N of M"` wording |
| `total_chunks` | `usize` | Known upfront once total duration and the existing chunk-length threshold are both known |
| `chunk_duration_ms` | `u64` | How far to advance the determinate bar's position |

## Relationship to existing entities (spec 001)

- Consumes `InputMedia.duration_secs` (already computed before transcription begins, for both
  input methods) as `Transcribing`'s known total — no new probing logic.
- Does not modify `Transcript`, `Segment`, `ModelOption`, or `OutputArtifact` in any way; this
  feature is purely an additional, stderr-only observation of an existing pipeline's progress, not
  a change to what that pipeline produces.
