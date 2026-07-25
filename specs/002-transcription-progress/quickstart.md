# Quickstart: Validating Transcription Progress Indicators

Prerequisites: a release build (`task rust:release`), a cached model (`para --list-models` shows
`Cached`), and two sample files — a long recording (at least a couple of minutes, to make progress
visibly move) and a short one (well under the chunking threshold).

## US1 — Progress during a long transcription, both input methods

```bash
para -i long-recording.wav
cat long-recording.wav | para
```

Expected: in both cases, an animated progress indicator appears on stderr, advances as work
proceeds, and shows an estimated time remaining that adjusts over the run rather than staying
fixed. Redirect stdout to a file in both cases (`> out.txt`) and confirm `out.txt` contains only
the transcript — nothing progress-related ever lands there.

## US2 — Feedback on short input

```bash
time para -i short-clip.wav
```

Expected: a brief "loading model" indicator appears on stderr before the transcript is produced,
visible well within 3 seconds of starting (spec.md SC-005) even though the whole run finishes in a
few seconds.

## US3 — Script-safe when redirected

```bash
para -i long-recording.wav 2> progress.log
cat progress.log
```

Expected: `progress.log` contains only plain, newline-terminated text — no animation/cursor-control
escape sequences, readable in a normal text editor. Also try with `TERM=dumb` in front of the
command while stderr _is_ attached to a real terminal, and confirm the same plain-text behavior
(research.md §2 — an unset or `dumb` `TERM` is treated the same as a non-terminal).

## US4 — Suppressing progress entirely

```bash
para -i long-recording.wav --no-progress 2> suppressed.log
cat suppressed.log   # expected: empty
PARA_NO_PROGRESS=1 para -i long-recording.wav 2> suppressed2.log
cat suppressed2.log   # expected: empty
```

Then confirm errors still surface with suppression on:

```bash
para -i does-not-exist.wav --no-progress
```

Expected: still fails with the usual specific stderr error message and non-zero exit — suppression
affects only progress reporting (FR-009).

## Cross-cutting checks

- Compare wall-clock time for the same input with and without `--no-progress`
  (`time para -i long-recording.wav` vs. `time para -i long-recording.wav --no-progress`);
  confirm no perceptible difference (FR-013/SC-007).
- Run with stderr closed (`para -i long-recording.wav 2>&-`, where the shell supports it) and
  confirm the run still completes normally with a correct transcript and exit code 0 (FR-012).
