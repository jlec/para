# Quickstart: Validating Local Audio & Video Transcription

Runnable validation scenarios proving the feature works end-to-end, one per user story plus the
cross-cutting success criteria. Uses the CLI contract in `contracts/cli-interface.md` and the
output contracts in `contracts/output-json-schema.json` / `contracts/output-srt.md`.

## Prerequisites

- Rust toolchain via `rustup`, 2024-edition-capable (MSRV 1.85+)
- `ffmpeg` on `PATH`
- Network access for the *first* build (`ort` fetches the ONNX Runtime binary) and the *first* use
  of any given model (downloads it to the local cache)
- A short sample audio file and a short sample video file with speech, in different common formats
  (e.g., `sample.wav`, `sample.mp4`)

## Build

```bash
cargo build --release
```

Expected: succeeds with only network access used being the one-time ONNX Runtime fetch; produces
`target/release/para` plus a co-located ONNX Runtime shared library (research.md §2).

## US1 — Plain-text transcript (P1)

```bash
./target/release/para -i sample.wav
./target/release/para -i sample.mp4
./target/release/para -i sample.wav -o transcript.txt && cat transcript.txt
cat sample.wav | ./target/release/para
```

Expected: each command prints (or writes) a plain-text transcript of the spoken content; the video
case requires no manual audio extraction step; the file-redirect case's file contains transcript
text only; the piped case produces the same transcript as the file-path case.

## US2 — Model selection (P2)

```bash
./target/release/para --list-models
time ./target/release/para -i sample.wav --model parakeet-ctc-0.6b >/dev/null
time ./target/release/para -i sample.wav --model parakeet-tdt-0.6b-v3 >/dev/null
./target/release/para -i sample.wav --model does-not-exist
echo $?
```

Expected: `--list-models` shows every registered model with cache state and a default marker; the
CTC-tier run's wall time is measurably lower than the TDT-tier run's; the invalid model name exits
non-zero with a message listing valid IDs, and does not attempt a transcription.

## US3 — Structured timed output (P3)

```bash
./target/release/para -i sample.wav --format json | tee out.json
python3 -c "import json,sys; d=json.load(open('out.json')); assert d['segments']; [s['end']>s['start'] for s in d['segments']]"
```

Expected: valid JSON conforming to `contracts/output-json-schema.json`; every segment has
`end > start`; no non-JSON text appears in the piped stream.

## US4 — Subtitle output (P4)

```bash
./target/release/para -i sample.mp4 --format srt -o captions.srt
cat captions.srt
```

Expected: output matches `contracts/output-srt.md` — sequential block numbers, comma-separated
milliseconds, ordered non-overlapping time ranges.

## Cross-cutting: offline operation (SC-003)

```bash
# after every model used above is already cached:
sudo ifconfig en0 down   # or your platform's equivalent network-disable step
./target/release/para -i sample.wav
sudo ifconfig en0 up
```

Expected: transcription completes normally with no network available, using the already-cached
model.

## Cross-cutting: pipeline safety (SC-006)

```bash
./target/release/para -i sample.wav | wc -l
./target/release/para -i sample.wav --model does-not-exist 2>/dev/null | wc -c
```

Expected: the first command's line count reflects only transcript content; the second prints
nothing to stdout for a failing run (stderr is separately non-empty, discarded here by
`2>/dev/null`) — nothing non-transcript ever reaches stdout.

## Cross-cutting: refresh a model (clarified scope)

```bash
./target/release/para -i sample.wav --refresh-model
```

Expected: the selected model's cache is deleted and re-downloaded before the run proceeds, with
progress on stderr; there is no separate "remove model" command to check for, because that
standalone capability is explicitly out of scope (FR-021).
