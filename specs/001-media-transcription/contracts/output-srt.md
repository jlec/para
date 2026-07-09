# Contract: SRT Subtitle Output (--format srt)

Satisfies FR-006: a subtitle file with correctly ordered, non-overlapping timed captions usable by
common video players.

## Block format

```
<index>
<start> --> <end>
<segment text>
<blank line>
```

- `<index>`: 1-based, sequential, no gaps.
- `<start>` / `<end>`: `HH:MM:SS,mmm` — **the millisecond separator is a comma, not a period.**
  This is the format the SRT spec (and the players that consume it) require; a period there
  causes some tools to reject the file silently rather than error, which is exactly the kind of
  quiet failure Constitution Principle IV exists to prevent at the tool's own boundary — para must
  get this right rather than pass the problem downstream.
- Blocks are separated by exactly one blank line; the file ends after the last block's blank line.

## Timestamp formatting rule

```
total_ms = round(seconds * 1000)
ms = total_ms % 1000
s  = (total_ms / 1000) % 60
m  = (total_ms / 60_000) % 60
h  = total_ms / 3_600_000
format: "{h:02}:{m:02}:{s:02},{ms:03}"
```

Example: `3661.5` seconds → `01:01:01,500`.

## Single-segment (whole-file-timing) models

A model with `timing_granularity: WholeFile` (research.md §3 / data-model.md `ModelOption`)
produces exactly one block spanning `00:00:00,000` to the file's full duration. This still
satisfies "correctly ordered, non-overlapping" (US4 acceptance scenario 2) with a set of one.

## Test mapping

- Correct block numbering across ≥2 segments
- Comma (not period) millisecond separator, verified explicitly
- Blank line between blocks, none trailing after the last block beyond the one required
- Single-segment fallback produces exactly one well-formed block spanning the full duration
