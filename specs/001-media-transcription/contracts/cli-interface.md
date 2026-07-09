# Contract: CLI Interface

This is the observable contract for `para`'s command-line surface — what a script or another
program can rely on. It is derived from spec.md's Functional Requirements; flag names below are a
plan-level naming choice (not a spec.md requirement), open to revision in tasks.md/implementation
as long as the behaviors they name stay intact.

## Invocation shapes

```bash
para -i audio.mp3                          # file in, transcript to stdout
para -i audio.mp3 -o transcript.txt        # file in, transcript to file
cat audio.mp3 | para                       # stdin in, transcript to stdout
para -i lecture.m4a --model parakeet-ctc-1.1b
para -i reel.mp4 --format json
para -i reel.mp4 --format srt -o captions.srt
para --list-models
para -i audio.mp3 --refresh-model
```

## Flags

| Flag | Required | Default | Behavior |
|---|---|---|---|
| `-i, --input <PATH>` | No | stdin | If omitted and stdin is a TTY, exit 1 immediately with a usage error (nothing to transcribe) — FR-002 |
| `-o, --output <PATH>` | No | stdout | FR-011 |
| `-m, --model <ID>` | No | the registry's designated default | Unknown ID → exit non-zero, error lists valid options, no run attempted — FR-010 |
| `-f, --format <text\|json\|srt>` | No | `text` | FR-007 |
| `--list-models` | No | — | Prints available models and their cache state, then exits 0 without transcribing — FR-019 |
| `--refresh-model` | No | — | Forces the selected model's cache to be deleted and re-downloaded before the run proceeds — FR-020 |

Standalone model removal (a command whose only effect is deleting a cached model without
re-fetching it) is explicitly **not** part of this contract — FR-021.

## Stream contract

- **stdout**: contains *only* the transcript in the requested format. Nothing else is ever written
  there — no banners, no progress, no warnings (FR-012, Constitution Principle III).
- **stderr**: everything else — download progress, the CoreML first-compile notice, per-chunk
  progress (`"transcribing chunk N of M"`, FR-023, emitted only when the input required chunked
  encoding), and all error messages.
- **exit code**: `0` only on a fully-produced, complete transcript in the requested format. Any
  other outcome is a non-zero exit with a stderr message — never a partial write treated as
  success (FR-014).

## Error conditions (non-exhaustive, illustrative of the contract)

| Condition | stdout | stderr | Exit code |
|---|---|---|---|
| ffmpeg not on PATH | empty | specific, actionable message | non-zero |
| Input file missing/unreadable/no audio track | empty | specific message naming the problem | non-zero |
| Unknown `--model` value | empty | message + list of valid IDs | non-zero |
| Model not cached, download exhausts retries (FR-022) | empty | specific message; never a silent switch to a different cached model | non-zero |
| Successful run | transcript only | none, or progress messages only (never mixed with transcript content) | `0` |

## Test mapping

Every row above corresponds to at least one `tests/contract/` test asserting the stdout/stderr/exit
code split, per the Engineering Standard that every error path has a test.
