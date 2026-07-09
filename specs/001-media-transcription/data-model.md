# Phase 1 Data Model: Local Audio & Video Transcription

Entities are derived from spec.md's Key Entities section, with implementation-level fields added
where the technical design (research.md) requires them. This is an in-process CLI, not a service —
these are Rust types passed between modules within a single invocation, not persisted records.

## InputMedia

Represents the audio or video source for one run.

| Field | Type | Notes |
|---|---|---|
| `source` | `Path` \| stdin stream | File path argument, or a `tempfile::NamedTempFile` staged from stdin (research.md §"ffmpeg cannot seek arbitrary stdin streams") |
| `format_hint` | enum (WAV/MP3/M4A/MKV/FLAC/OGG/Unknown) | Magic-byte detection; diagnostics only per FR-001 — never blocks a format ffmpeg can otherwise handle |
| `has_audio_track` | `bool` | Determined by ffmpeg's own probing; absence is a rejection per FR-015 / edge case "no audio track at all" |
| `duration_secs` | `f64` | From ffmpeg probe; used to decide single-pass vs. chunked encoding (FR-023) |

**Validation rules**: Reject (FR-015) if the file doesn't exist, is zero-length/truncated, ffmpeg
can't probe it, or probing finds no usable audio track. No size/duration cap is enforced (spec.md
Assumptions).

## Transcript

The result of one transcription run.

| Field | Type | Notes |
|---|---|---|
| `text` | `String` | Full plain-text transcript (FR-004) |
| `segments` | `Vec<Segment>` | Populated for structured/subtitle output (FR-005/FR-006); for CTC-tier models this is a single segment spanning `0..duration_secs` (research.md §3) |
| `model` | `String` | Model ID actually used — always echoed, never silently substituted (FR-009, FR-010) |
| `duration_secs` | `f64` | Carried from `InputMedia` for output metadata |

## Segment

One timed unit of the transcript.

| Field | Type | Notes |
|---|---|---|
| `start` | `f64` (seconds) | |
| `end` | `f64` (seconds) | Must be > `start`; segments must be ordered and non-overlapping (US4 acceptance scenario 2) |
| `text` | `String` | Segment-level (phrase/sentence) text per spec.md Assumptions — not word-level |

## ModelOption (registry entry)

One selectable model, known statically at build time (FR-008, FR-009, FR-019).

| Field | Type | Notes |
|---|---|---|
| `id` | `String` | Stable CLI-facing identifier, e.g. `parakeet-tdt-0.6b-v3` |
| `description` | `String` | Shown in `--list-models` output; must disclose language support and timing granularity |
| `kind` | enum `TDT` \| `CTC` | Determines decoder path (research.md §3) and `timing_granularity` |
| `timing_granularity` | enum `Segment` \| `WholeFile` | `TDT` → `Segment`; `CTC` → `WholeFile` (single segment) — surfaced to the user so choosing a fast/CTC model for subtitle output is an informed tradeoff, not a silent quality cliff |
| `is_default` | `bool` | Exactly one entry MUST be `true` (FR-009) |
| `files` | `Vec<ModelFile>` | What must be present/verified in the cache |
| `cache_state` | enum `NotCached` \| `Cached` \| `Downloading` | Runtime-computed, not stored; drives `--list-models` display (FR-019) and whether a run triggers a download |

## ModelFile

One file belonging to a `ModelOption`'s cache directory.

| Field | Type | Notes |
|---|---|---|
| `name` | `String` | Filename on disk |
| `source_url` | `String` | Resolved download URL (HuggingFace `resolve/main/<filename>` pattern) |
| `sha256` | `String` | **Computed from the real downloaded file at implementation time** — never a placeholder (Constitution Principle V; research.md §3) |
| `size_bytes` | `u64` | For progress-bar totals |

**State transitions**: `NotCached → Downloading → Cached` on success; `Downloading → NotCached` on
any failure (no partially-downloaded file is ever left in place — atomic rename on success only,
per FR-022). `--refresh-model` forces `Cached → Downloading` (delete then re-fetch) regardless of
current checksum state (FR-020).

## OutputArtifact

What a run produces.

| Field | Type | Notes |
|---|---|---|
| `format` | enum `Text` \| `Json` \| `Srt` | User-selected (FR-007) |
| `destination` | `Stdout` \| `File(PathBuf)` | Stdout is the default; file is explicit (FR-011) |

No additional entity is needed for "progress reporting" (FR-023) — it is stderr output emitted
during processing, not a artifact with its own state.
