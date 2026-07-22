# Feature Specification: Transcription Progress Indicators

**Feature Branch**: `002-transcription-progress`

**Created**: 2026-07-16

**Status**: Draft

**Input**: User description: "I would like to see progress bars for both the standard in as well as the input of flag (both -i file input and stdin piped input). Investigate the options here for feasibility from a UX perspective — consult a UX expert if useful — and start planning the implementation."

## Context

This feature revises a decision made in spec `001-media-transcription`: that spec's FR-023
clarification explicitly decided "no percentage or progress bar is required" for chunked inputs,
with a minimal `"transcribing chunk N of M"` stderr line judged sufficient. This spec supersedes
that judgment — progress reporting now applies uniformly, with real percentage/ETA feedback,
regardless of whether an input needs chunked processing.

A UX consult (see below) resolved the framing question at the center of the user's request: file
(`-i`) input and piped stdin input are **not** actually different once the input is staged and
its audio duration is known — both go through the same duration-probing step before transcription
begins. The only place the two input methods genuinely differ is the brief period before that:
reading a file with a known size vs. reading an unbounded pipe with no declared length. The design
below treats this as two phases (getting the input onto disk, then transcribing it), not two
input-source-specific behaviors.

## Clarifications

### Session 2026-07-16

- Q: Should there be an explicit constraint on how much overhead progress reporting is allowed to
  add to transcription's wall-clock time? → A: Qualitative only — progress reporting must not be
  perceptibly slower than the same run without it; no specific numeric threshold.
- Q: What's the exact time threshold for the "quick feedback" claim (previously "a couple of
  seconds")? → A: 3 seconds.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - See progress during a long transcription (Priority: P1)

A user runs para against a long recording (or pipes one in) and wants to know, at a glance and
without reading raw log lines, roughly how much of the job is done and how much longer it will
take — for both a file given via `-i` and one piped through stdin.

**Why this priority**: This is the entire reason the feature exists. A long-running command with
no feedback reads as broken, especially for a tool whose recordings can run for hours.

**Independent Test**: Run para against a recording long enough to take at least several tens of
seconds, both via `-i <file>` and via `cat file | para`; confirm a visibly updating progress
indicator appears on stderr in both cases, advances as work completes, and standard output still contains
only the final transcript.

**Acceptance Scenarios**:

1. **Given** a user runs para with `-i` against a long audio file, **When** transcription is in
   progress, **Then** stderr shows a progress indicator reflecting the fraction of the recording's
   audio duration processed so far, updating as work proceeds.
2. **Given** a user pipes a long audio file's bytes into para via stdin, **When** transcription is
   in progress, **Then** stderr shows the same kind of progress indicator as the `-i` case, once
   the piped input has been read and its duration determined.
3. **Given** either input method, **When** the run completes, **Then** standard output contains only the
   transcript — the progress indicator never wrote to standard output at any point.
4. **Given** a progress indicator is showing an estimated time remaining, **When** processing
   speed varies during the run, **Then** the estimate is based on the run's own observed progress
   rather than a fixed assumption, and is presented as an estimate rather than a guarantee.

---

### User Story 2 - Get feedback immediately, even on short input (Priority: P2)

A user runs para against a short clip. Even though the whole job finishes quickly, they still want
confirmation the tool is actively working (rather than apparently hanging) during the brief model
loading step every run requires.

**Why this priority**: Builds on User Story 1 by removing the one remaining silent window (startup
before any audio has been processed). Without it, short-input runs would still have a few seconds
of the tool doing real work with zero visible feedback.

**Independent Test**: Run para against a short clip (well under the chunking threshold); confirm a
brief, visible indication of activity (e.g., "loading model") appears on stderr before the
transcript appears, even though the whole run completes in a few seconds.

**Acceptance Scenarios**:

1. **Given** a user runs para against any input, **When** the model is loading, **Then** stderr
   shows an indicator that the tool is working, before any transcription progress begins.
2. **Given** a short clip that completes in a few seconds, **When** the run finishes, **Then** the
   user has seen some visible activity indicator during the run, not silence followed by an
   instant result.

---

### User Story 3 - Progress output stays script-safe when redirected (Priority: P3)

A user redirects para's stderr to a log file, or runs it inside another program that captures
output, rather than watching it in an interactive terminal. They need the captured output to
remain readable plain text, not full of redraw/cursor-control characters meant for a live display.

**Why this priority**: Protects the tool's core scriptability promise (Constitution III, "Stdout Is
Sacred") from being undermined by a feature that only makes sense visually. Lower priority than
US1/US2 because it's a safety net for a case the interactive experience doesn't need to worry
about, not the primary experience itself.

**Independent Test**: Run para with stderr redirected to a file (non-interactive); confirm the
captured file contains plain, newline-terminated progress milestones with no animation/redraw
control characters, and that standard output is still exactly the transcript.

**Acceptance Scenarios**:

1. **Given** stderr is not connected to an interactive terminal, **When** para runs, **Then**
   progress is reported as plain newline-terminated text milestones instead of an animated
   indicator.
2. **Given** stderr is redirected to a file, **When** the run completes, **Then** the file contains
   no unreadable control characters, and standard output still contains only the transcript.

---

### User Story 4 - Suppress progress output entirely (Priority: P4)

A user running para inside an automated pipeline wants no progress chatter on stderr at all, not
even the plain-text fallback.

**Why this priority**: A natural, narrowly-scoped complement to US3 for users who want zero stderr
output rather than a reduced form of it — valuable but the narrowest of the four stories.

**Independent Test**: Run para with the suppression option set; confirm no progress output appears
on stderr (errors, if any, still do), and standard output still contains only the transcript.

**Acceptance Scenarios**:

1. **Given** a user passes the progress-suppression option, **When** para runs, **Then** no
   progress indicator or milestone text is written to stderr.
2. **Given** progress output is suppressed, **When** an error occurs, **Then** the error is still
   reported to stderr — suppression affects only progress reporting, never error reporting.

---

### Edge Cases

- What happens when the input's audio duration cannot be determined at all (e.g., a file that
  passes initial format checks but ffmpeg cannot report a duration for)? Progress reporting must
  degrade (e.g., to an indeterminate indicator) without failing the transcription itself, since
  duration is a progress-reporting concern, not a transcription-correctness one.
- What happens when stderr itself is closed or cannot be written to (e.g., a script explicitly
  closes file descriptor 2)? Progress reporting must not crash the run or affect its exit code —
  transcription success/failure is independent of whether progress could be displayed.
- What happens for an input so short that model loading takes longer than the transcription
  itself? The user must still see the model-loading indicator; the transcription-progress
  indicator may complete almost immediately after it begins.
- What happens when `NO_COLOR` is set or the terminal type indicates no styling support (e.g.
  `TERM=dumb`)? Progress output must remain readable without relying on color to convey meaning.
- What happens when both stdin input and the progress-suppression option (User Story 4) are used
  together? Suppression applies regardless of input method — no progress output either way.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: System MUST display a progress indicator on stderr while transcription is underway,
  for input given via `-i` and for input piped via stdin alike, without altering standard output's content.
- **FR-002**: Once an input's total audio duration is known, the progress indicator MUST reflect
  the fraction of that duration processed so far, not a raw byte count or a fixed chunk-count
  alone.
- **FR-003**: System MUST show a progress indicator during model loading, before transcription
  begins, for every run regardless of input length, so a run never appears to hang at startup.
- **FR-004**: For stdin input, since the total amount of data to read is not known in advance, the
  system MUST show an indeterminate progress indicator while input is being read, distinct from
  the determinate, duration-based indicator that follows once reading completes and duration is
  known.
- **FR-005**: The progress indicator MUST include an estimated time remaining, computed from the
  run's own observed processing speed rather than a fixed per-model assumption, and presented as
  an estimate rather than a guaranteed figure.
- **FR-006**: Progress updates MUST occur at least once per processing chunk (per the existing
  chunking behavior for long inputs); finer-than-chunk update granularity is not required.
- **FR-007**: When stderr is not connected to an interactive terminal, System MUST emit plain,
  newline-terminated progress milestones instead of an animated/redrawing indicator, so redirected
  or captured output contains no unreadable control characters.
- **FR-008**: Users MUST be able to suppress all progress output via an explicit, documented
  option, independent of whether stderr is a terminal.
- **FR-009**: Progress-suppression MUST NOT suppress error reporting — errors are still written to
  stderr regardless of whether progress output is suppressed.
- **FR-010**: standard output MUST continue to contain only the transcript in the requested output format,
  unaffected by any progress-reporting behavior introduced by this feature (reaffirms the existing
  "Stdout Is Sacred" guarantee under this new behavior).
- **FR-011**: This feature supersedes spec `001-media-transcription`'s FR-023 clarification that
  "no percentage or progress bar is required" — progress reporting now applies uniformly whether
  or not an input requires chunked processing.
- **FR-012**: A failure to display progress (e.g., an unwritable stderr) MUST NOT cause the
  transcription itself to fail or change its exit code.
- **FR-013**: Progress reporting MUST NOT be perceptibly slower than running the same transcription
  without it — the reporting mechanism itself must not introduce noticeable overhead to the run's
  wall-clock time.

### Key Entities

- **Progress Phase**: The stage a run is in from the user's perspective — acquiring input (reading
  a file or stdin), loading the model, or transcribing — each with its own indicator behavior.
- **Progress Indicator**: What's shown for a given phase; has a determinacy (determinate, when a
  total is known, vs. indeterminate, when it isn't), a unit of measure (audio-seconds processed,
  for the transcription phase), and a destination (always stderr, never standard output).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: A user running para against a recording long enough to take noticeable time sees a
  progress indicator that updates as work proceeds, for both `-i` and piped-stdin input, without
  needing to interpret raw log lines to know the job is progressing.
- **SC-002**: A user piping input via stdin never sees a progress indicator imply a false total —
  the indeterminate reading phase never displays a percentage it cannot actually know.
- **SC-003**: 100% of runs with stderr redirected to a non-terminal destination produce output free
  of animation/redraw control characters, remaining plain, readable text.
- **SC-004**: A user can fully suppress progress output with a single documented option, verified
  by no progress-related output appearing on stderr when it's set.
- **SC-005**: A user running para against a short input (previously silent until the transcript
  appeared) sees a visible activity indicator within 3 seconds of starting, before the transcript
  is produced.
- **SC-006**: In no observed run does any progress-reporting content appear on standard output.
- **SC-007**: A user cannot perceive a difference in total transcription time between a run with
  progress reporting and an equivalent run with it suppressed (User Story 4).

## Assumptions

- Progress granularity for the transcription phase is per-processing-chunk (the existing ≤300s
  chunking boundary), not per-audio-frame — chosen over finer-grained intra-chunk reporting to
  avoid adding instrumentation to the decode loop itself, at the cost of the indicator appearing to
  pause for up to the length of one chunk on the slower of the two transcription models.
- The estimated-time-remaining figure is computed adaptively from the run's own measured progress
  rate, not a hardcoded per-model constant, and is expected to fluctuate somewhat on inputs whose
  actual speech content is uneven, since one of the two transcription models' processing cost
  scales with spoken content rather than strictly with audio duration.
- The distinction between `-i` file input and piped stdin input matters only during the initial
  input-acquisition phase (a known file size vs. an unbounded stream); once an input's audio
  duration is determined, the two input methods are treated identically for progress purposes.
- The progress-suppression option (User Story 4) is a small, closely-related addition to this
  feature's core scope — a single flag alongside the existing `--list-models`/`--refresh-model`
  flag pattern — rather than a broader "quiet mode" affecting other output.
- Existing terminal-compatibility conventions (respecting `NO_COLOR`, `TERM=dumb`, and gating all
  behavior on stderr's terminal status rather than standard output's) are assumed to apply, consistent with
  this being a stderr-only, pipeline-safe tool by constitution.
