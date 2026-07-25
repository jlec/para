# Quickstart: Validating the Native CoreML Backend and Transcript Polish

Prerequisites: a release build (`task rust:release`) on Apple Silicon macOS, all three models
cached (now including their `.mlmodelc` bundles), and the same long/short sample files and
VoiceInk reference transcript used during 003's and this feature's own investigation.

## US1 — Peak memory is dramatically lower than the ONNX Runtime baseline

```bash
./target/release/para -i long-recording.wav --no-progress &
PID=$!; PEAK=0
while kill -0 $PID 2>/dev/null; do
  RSS=$(ps -o rss= -p $PID | tr -d ' '); [ -n "$RSS" ] && [ "$RSS" -gt "$PEAK" ] && PEAK=$RSS
  sleep 0.3
done
echo "peak: $(echo "scale=2; $PEAK/1048576" | bc) GB"
```

Expected: at least 70% lower than the current ~2.5GB figure (SC-001), for every model, not just
the default (SC-004) — repeat with `--model parakeet-ctc-0.6b` and `--model parakeet-tdt-0.6b-v2`.

## US2 — Transcription completes noticeably faster

```bash
time ./target/release/para -i long-recording.wav --no-progress > /dev/null
```

Expected: at least 50% faster wall-clock time than the current ONNX-Runtime baseline on the same
recording, same machine (SC-002).

## US3 — Transcript reads cleanly

```bash
./target/release/para -i ~/tmp/07-01_Meeting_GPU_Management_Strategy_Forecasting_and_Global-China_HPC_Deployment.mp3 > after.txt
diff <(tr ' ' '\n' < after.txt) <(tr ' ' '\n' < ~/tmp/on-board-in-the-last-of-the-week.md)
grep -ic '\bum\b\|\buh\b' after.txt   # expected: 0
grep -c '^$' after.txt                # expected: >1 (multiple paragraphs, not one block)
```

Expected: no filler words, multiple paragraphs at natural pauses, and spot-checked
numbers/acronyms in conventional written form where confidently recognizable (SC-003) — compare
directly against the real VoiceInk reference transcript at
`~/tmp/on-board-in-the-last-of-the-week.md`, the same one that originally surfaced this gap.

## Regression check — no completeness loss, SRT/JSON timing intact

```bash
./target/release/para -i long-recording.wav --format srt > after.srt
./target/release/para -i long-recording.wav --format json > after.json
```

Expected: word count in the plain-text output is not lower than the pre-this-feature baseline
(SC-005, FR-004); SRT/JSON segment timestamps look sane and are unaffected by the text-polish
changes (FR-009).

## Fallback path — a model without a CoreML conversion (if one is ever added later)

Not exercisable today (all three current models have real conversions — research.md §1), but the
model registry's `ModelBackend` resolution should be spot-checked in code review to confirm a
hypothetical uncovered model would cleanly resolve to the existing ONNX Runtime path rather than
failing, per FR-005.
