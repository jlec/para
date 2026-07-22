# Quickstart: Validating the Memory Footprint Reduction

Prerequisites: a release build (`task rust:release`), a cached default model, and two sample files
— a short clip (under 5 minutes) and a long recording (tens of minutes; the original report used a
~25-minute file).

## US1 — Long recording doesn't use dramatically more memory than a short one

```bash
# macOS: watch peak memory with Activity Monitor, or poll RSS directly:
./target/release/para -i short-clip.wav --no-progress > /dev/null &
PID=$!; while kill -0 $PID 2>/dev/null; do ps -o rss= -p $PID; sleep 1; done

./target/release/para -i long-recording.wav --no-progress > /dev/null &
PID=$!; while kill -0 $PID 2>/dev/null; do ps -o rss= -p $PID; sleep 1; done
```

Expected: the long recording's peak RSS is within roughly the same range as the short clip's, not
several times higher (SC-001) — and, watching the values over time, memory should plateau rather
than climb continuously through the run.

## US2 — Memory scales with the chosen model, not a fixed number

```bash
./target/release/para -i short-clip.wav --model parakeet-ctc-0.6b --no-progress   # smallest model
./target/release/para -i short-clip.wav --model parakeet-tdt-0.6b-v3 --no-progress # largest model
```

Expected: peak memory for the smaller CTC model is noticeably lower than for the larger TDT model,
each within a small, consistent multiple of that model's on-disk cache size (SC-003).

## Regression check — output is unaffected

```bash
para -i long-recording.wav > after.txt
git stash  # revert to pre-fix code temporarily
cargo build --release
./target/release/para -i long-recording.wav > before.txt
git stash pop
diff before.txt after.txt   # expected: identical
```

Expected: transcript text is byte-for-byte identical before and after — this is a resource-usage
change only (FR-004), never a correctness change. (Minor differences exactly at old chunk
boundaries that no longer fall at the same place are possible and acceptable, per
`001-media-transcription` research.md §17's documented chunk-boundary caveat — but the two runs
should not diverge meaningfully.)

## The original reported case

```bash
./target/release/para -i ~/tmp/07-01_Meeting_GPU_Management_Strategy_Forecasting_and_Global-China_HPC_Deployment.mp3 --no-progress
```

Expected: peak memory at least 40% lower than the ~5.79GB baseline measured during this feature's
research (SC-002), transcript output unchanged from before the fix.
