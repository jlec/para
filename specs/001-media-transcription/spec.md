# Feature Specification: Local Audio & Video Transcription

**Feature Branch**: `001-media-transcription`

**Created**: 2026-07-09

**Status**: Draft

**Input**: User description: "para transcribes audio and video files to text, running entirely on the user's machine. A user has an audio or video file (a recording, a voice memo, a downloaded clip) and wants a text transcript. They run para, pointing it at the file or piping the file's contents in, and get plain text back, either printed out or written to a file. The tool must: accept common audio and video formats without the user needing to convert anything first; work without an internet connection, once initial setup is done; produce output usable in three ways: as plain readable text, as structured data with timing information, and as subtitle files; let the user choose between a few different transcription models, trading off speed against accuracy; behave predictably in scripts and pipelines — clean success or clear failure, nothing in between. Out of scope for this version: real-time transcription, identifying different speakers, any kind of graphical interface."

## Clarifications

### Session 2026-07-09

- Q: Is explicit model management (listing, pre-downloading, removing cached models) part of this feature's scope? → A: Listing available/cached models is in scope. Forcing a fresh re-download of a model (discarding and re-fetching its cached copy, e.g. to repair a bad cache) is in scope. A standalone "remove a cached model" command is not in scope.
- Q: When a user selects a model that isn't cached yet and the required download can't complete (no network, interrupted, unreachable server), what must happen? → A: The system retries the download a bounded number of times with backoff; regardless of retries, the end state is always a clear, specific error and a non-zero exit — never a silent fallback to a different model, and never a partially-downloaded model left in the cache.
- Q: Must para emit progress indication to stderr during a long transcription run? → A: Required only when the input is long enough to need chunked processing (above the single-pass processing threshold) — a minimal per-chunk signal such as "transcribing chunk N of M" is sufficient; no percentage or progress bar is required. Inputs short enough to transcribe in a single pass need no interim progress output.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Get a plain-text transcript from a file (Priority: P1)

A user has a recording, voice memo, or downloaded clip and wants to read what
was said. They point para at the file and receive a plain-text transcript,
either printed to their terminal or saved to a file, with no manual
conversion step beforehand.

**Why this priority**: This is the entire reason the tool exists. Without a
correct, readable transcript from a single command, nothing else matters.

**Independent Test**: Run para against a sample audio file and a sample
video file in different common formats; confirm a readable text transcript
is produced from each without any prior manual conversion, and that it can
be redirected to a file.

**Acceptance Scenarios**:

1. **Given** a common-format audio file, **When** the user runs para against
   it, **Then** a plain-text transcript of the spoken content is printed.
2. **Given** a common-format video file, **When** the user runs para against
   it, **Then** the spoken audio in the video is transcribed to plain text
   without the user extracting or converting audio first.
3. **Given** a successful transcription, **When** the user redirects the
   output to a file, **Then** the file contains only the transcript text.
4. **Given** an audio file's raw bytes are piped into para on standard
   input, **When** the run completes, **Then** the same transcript is
   produced as if the file path had been given directly.

---

### User Story 2 - Choose a model to balance speed and accuracy (Priority: P2)

A user wants control over how long transcription takes versus how accurate
it is — a quick draft transcript for a long recording, or the most accurate
possible transcript for an important clip.

**Why this priority**: Builds directly on the core transcription flow (US1)
and is the main way a user tunes the tool to their situation, but the tool
delivers value even with just a sensible default model.

**Independent Test**: Run the same input file through each available model
option and confirm each completes successfully, and that the fastest option
finishes noticeably sooner than the most accurate option.

**Acceptance Scenarios**:

1. **Given** no model is specified, **When** the user runs para, **Then** a
   sensible default model is used and clearly identified in any status
   output.
2. **Given** the user requests the fastest available model, **When**
   transcription runs, **Then** it completes faster than the same input
   would with the most accurate model.
3. **Given** the user requests the most accurate available model, **When**
   transcription runs, **Then** the resulting transcript is produced using
   that model rather than silently substituting a different one.
4. **Given** the user requests a model option that does not exist, **When**
   para runs, **Then** it fails immediately with a clear error listing the
   valid options.

---

### User Story 3 - Get timed, structured output for further processing (Priority: P3)

A user wants the transcript broken into timed segments as structured data,
so another program or script can consume exactly what was said and when.

**Why this priority**: Extends the core transcript (US1) with information
needed for downstream tooling, but is not required for the simplest
"read the transcript" use case.

**Independent Test**: Run para against a sample file requesting structured
timed output and confirm the result is machine-parseable and each segment
carries start/end timing alongside its text.

**Acceptance Scenarios**:

1. **Given** a user requests structured timed output, **When** transcription
   completes, **Then** the output contains the transcript broken into
   segments, each with a start time, end time, and text.
2. **Given** structured timed output is requested, **When** the run
   succeeds, **Then** the output is well-formed and machine-parseable with
   no extraneous text mixed in.

---

### User Story 4 - Get a subtitle file for a video (Priority: P4)

A user has a video clip and wants a subtitle file they can load into a
video player alongside it.

**Why this priority**: A natural extension of timed output (US3) for a
specific, common use case; valuable but the narrowest of the four stories.

**Independent Test**: Run para against a sample video file requesting
subtitle output and confirm the result is a timestamped caption file that
loads correctly in a common video player.

**Acceptance Scenarios**:

1. **Given** a user requests subtitle output, **When** transcription
   completes, **Then** the output is a timestamped caption file usable by
   common video players.
2. **Given** subtitle output is requested, **When** the run succeeds,
   **Then** captions are ordered correctly and their time ranges do not
   overlap.

---

### Edge Cases

- What happens when the input file has no audio track at all (e.g., a
  silent video, or a video with a broken audio stream)?
- What happens when the input file's format is not recognized or supported?
- What happens when the input file is empty (zero bytes) or truncated?
- What happens when piped standard-input data doesn't contain a complete or
  valid media stream?
- What happens when the audio contains no detectable speech (e.g., music
  only, silence)?
- What happens when the destination for file output can't be written to
  (e.g., no disk space, no write permission)?
- What happens when the input recording is unusually long (e.g., several
  hours)?
- What happens when a model is not yet cached and its download cannot
  complete (no network, connection drops mid-download, server unreachable)?
- What happens when an input is long enough that it must be processed in
  multiple chunks rather than a single pass?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST accept common audio and video file formats as
  input without requiring the user to convert or extract anything first.
- **FR-002**: System MUST accept input either as a file path argument or as
  raw file data piped via standard input.
- **FR-003**: System MUST extract and transcribe the spoken audio from a
  video input automatically, with no separate manual step.
- **FR-004**: System MUST produce a plain, readable text transcript as the
  default output.
- **FR-005**: System MUST be able to produce output as structured data in
  which the transcript is broken into segments, each carrying a start time,
  end time, and text.
- **FR-006**: System MUST be able to produce output as a subtitle file with
  correctly ordered, non-overlapping timed captions usable by common video
  players.
- **FR-007**: Users MUST be able to select which of the three output forms
  (plain text, structured timed data, subtitle file) a given run produces.
- **FR-008**: Users MUST be able to choose from at least three transcription
  model options that trade processing speed against transcription accuracy.
- **FR-009**: System MUST use a clearly defined default model when the user
  does not specify one.
- **FR-010**: System MUST reject an unrecognized model selection immediately
  with a clear error and a list of valid options, rather than falling back
  to a different model silently.
- **FR-011**: System MUST write the transcript to standard output by
  default, or to a file when the user specifies a destination.
- **FR-012**: System MUST NOT mix status messages, warnings, or errors into
  the transcript output stream.
- **FR-013**: System MUST be able to complete a transcription with no
  network connection available, provided the required model is already set
  up locally.
- **FR-014**: System MUST indicate the outcome of a run unambiguously: a
  completed run is either a full success or a reported failure, never a
  partial or silently degraded result.
- **FR-015**: System MUST reject an input file that is unsupported,
  unreadable, corrupted, or contains no usable audio with a clear, specific
  error, rather than producing empty, partial, or fabricated transcript
  text.
- **FR-016**: System MUST NOT provide real-time or in-progress transcription
  of an ongoing recording; only complete, already-recorded input is
  supported.
- **FR-017**: System MUST NOT attempt to identify or distinguish between
  multiple speakers within a recording.
- **FR-018**: System MUST NOT provide a graphical user interface; all
  interaction is via command invocation.
- **FR-019**: Users MUST be able to list the available model options and
  see which of them are already cached locally.
- **FR-020**: Users MUST be able to force a fresh re-download of a given
  model's cached files (discarding and re-fetching them), for cases such as
  repairing a corrupted local cache.
- **FR-021**: System MUST NOT provide a standalone command whose only
  purpose is to remove a cached model without also re-fetching it.
- **FR-022**: When a required model is not yet cached and its download
  cannot complete, System MUST retry a bounded number of times with
  backoff, then fail with a clear, specific error and a non-zero exit if
  all attempts are exhausted. System MUST NOT silently fall back to a
  different model, and MUST NOT leave a partially-downloaded model in the
  cache.
- **FR-023**: When an input is long enough to require chunked processing
  (above the single-pass processing threshold), System MUST emit a minimal
  per-chunk progress signal (e.g., "transcribing chunk N of M") to stderr
  as each chunk completes. Inputs short enough for a single pass are NOT
  required to produce any interim progress output.

### Key Entities

- **Input Media**: The audio or video file or byte stream supplied by the
  user; relevant attributes are its format, duration, and whether it
  contains a usable audio track.
- **Transcript**: The textual result of transcription; composed of the full
  text and, when timed output is requested, an ordered list of segments
  each with a start time, end time, and text.
- **Model Option**: One of the selectable transcription models; has a name,
  a relative position on the speed/accuracy tradeoff, and a local cache
  state (not yet downloaded / cached) that the user can inspect and force
  to refresh.
- **Output Artifact**: The file or stream produced by a run; has a form
  (plain text, structured timed data, or subtitles) and a destination
  (standard output or a file).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can go from a supported audio or video file to a
  finished plain-text transcript using a single command, with no manual
  conversion step.
- **SC-002**: 100% of runs against unsupported, corrupted, or audio-less
  input files end in a clear error and a failing exit status rather than
  incomplete or fabricated output.
- **SC-003**: Once a model is set up, a user can complete transcriptions
  with no network connection available, with no loss of functionality.
- **SC-004**: From the same input file, a user can obtain all three output
  forms (plain text, structured timed data, subtitles) without needing to
  re-supply or re-process the input differently.
- **SC-005**: Across the available model options, the fastest option
  completes a given input measurably faster than the most accurate option.
- **SC-006**: Transcript output piped directly into another command-line
  tool works correctly, with no non-transcript text ever appearing in that
  stream.

## Assumptions

- "Common audio and video formats" refers to the broad range of formats
  supported by widely used, general-purpose media tooling, not an
  exhaustive enumerated list.
- Automatic detection of the spoken language is assumed as a baseline
  capability, consistent with modern offline transcription tools, rather
  than the tool being limited to a single fixed language.
- "Structured data with timing information" means segment-level (phrase or
  sentence level) start/end timestamps, not word-by-word timestamps.
- Subtitle output follows the widely adopted timestamped-caption file
  convention used by common video players.
- "A few different models" means at least three selectable options spanning
  the speed/accuracy tradeoff, with one designated as the default.
- There is no artificial cap on input duration or size beyond what the
  user's own hardware can process.
- The default output destination is standard output; writing to a file is
  an explicit choice by the user.
