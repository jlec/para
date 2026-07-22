# Contract: CLI Interface Additions (Transcription Progress Indicators)

This extends `001-media-transcription`'s `contracts/cli-interface.md`, which remains the contract
of record for every flag not listed here. Only what this feature adds or changes is documented
below — everything else (existing flags, output formats, the JSON/SRT schemas) is unaffected.

## New flag

| Flag | Required | Default | Behavior |
|---|---|---|---|
| `--no-progress` | No | off (progress shown) | Suppresses all progress output on stderr (FR-008). Errors are still reported regardless (FR-009). Has no effect on stdout, which never carried progress output in the first place. |

## New environment variable

| Variable | Overrides |
|---|---|
| `PARA_NO_PROGRESS` | `--no-progress` — any non-empty value is equivalent to passing the flag, matching this project's existing `PARA_*` override convention |

## Revised stream contract (extends spec 001's)

- **stderr** now additionally carries, per run:
  1. A brief indeterminate indicator while reading stdin, when input arrives that way (FR-004) — not
     shown for `-i` file input, whose size is already known.
  2. A brief indeterminate indicator while the model loads (FR-003) — shown for every run,
     regardless of input length or method.
  3. A determinate progress indicator during transcription (FR-002/FR-005/FR-006), replacing spec
     001's bare `"transcribing chunk N of M"` line with an equivalent-or-richer indicator: an
     animated bar with percentage and adaptive ETA when stderr is an interactive terminal;
     otherwise, plain per-chunk milestone lines (research.md §1) — never both, never neither
     (unless `--no-progress`).
- None of the above ever appears when `--no-progress`/`PARA_NO_PROGRESS` is set (FR-008), except
  error messages, which are unaffected by this flag (FR-009).
- **stdout** contract is unchanged: only the transcript, in the requested format, ever (FR-010,
  reaffirming spec 001's Constitution III guarantee).
- **exit code** contract is unchanged: a failure to display progress (e.g., stderr unwritable)
  MUST NOT alter the run's exit code (FR-012).

## Observable behavior by environment (non-exhaustive, illustrative)

| Environment | What appears on stderr |
|---|---|
| Interactive terminal, `-i` file input, long recording | Model-load spinner, then an animated determinate bar (percent + ETA) advancing per chunk |
| Interactive terminal, piped stdin, long recording | An indeterminate byte-read spinner, then the same model-load spinner and determinate bar as above |
| stderr redirected to a file (any input method) | Plain newline-terminated milestone lines only — no animation, no ANSI/color codes |
| `TERM` unset or `TERM=dumb`, even if stderr is a real terminal device | Same plain-milestone behavior as a redirected file (research.md §2) |
| `--no-progress` / `PARA_NO_PROGRESS` set | No progress output of any kind; errors, if any, still appear |

## Test mapping

Each row above corresponds to at least one `tests/contract/` test, per the Engineering Standard
that every observable behavior in this contract has a corresponding test — extending the same
`tests/contract/` suite spec 001 established.
