# para

Local, offline audio and video transcription powered by [NVIDIA Parakeet](https://huggingface.co/nvidia)
speech models running on-device via [ONNX Runtime](https://onnxruntime.ai/). Single binary, no
daemon, no cloud API — point it at a file, get a transcript.

## Prerequisites

- `ffmpeg` on `PATH` (the only dependency you install by hand — `brew install ffmpeg`)
- Rust 2024 edition (MSRV 1.85+) to build from source
- Network access for the first build (fetches a prebuilt ONNX Runtime archive) and the first use
  of any given model (downloads it to your local cache) — nothing after that

## Build

```bash
task rust:build      # debug build
task rust:release    # optimized single binary: target/release/para
```

The ONNX Runtime is downloaded and **statically linked** at build time — `para` ships as one
binary, no separate shared library to distribute alongside it.

Cross-compiling a `linux/amd64` release from macOS needs a cross-linker toolchain (e.g. the
`cross` tool + Docker) that end users never need to install — it's a maintainer/CI concern for
producing release artifacts, not a runtime dependency of `para` itself.

## First run

```bash
para -i lecture.mp3
```

The first time you use a given model, `para` downloads it to your local cache with a progress bar
on stderr — stdout stays clean throughout. On Apple Silicon, the very first inference call also
prints a one-time note that it's compiling the model for the Neural Engine (can take up to a
minute; every run after that is fast). After that first run, everything is offline.

## Usage

```bash
para -i audio.mp3                          # file in, transcript to stdout
para -i audio.mp3 -o transcript.txt        # file in, transcript to file
cat audio.mp3 | para                       # stdin in, transcript to stdout
para -i lecture.mp4                        # video in — audio extracted automatically
para -i reel.mp4 --format json
para -i reel.mp4 --format srt -o captions.srt
para --list-models
para -i audio.mp3 --refresh-model
```

### Flags

| Flag                             | Default                            | Notes                                                                        |
| -------------------------------- | ----------------------------------- | ----------------------------------------------------------------------------- |
| `-i, --input <PATH>`             | stdin                                | Omit and pipe bytes in instead                                               |
| `-o, --output <PATH>`            | stdout                               |                                                                               |
| `-m, --model <ID>`               | `parakeet-tdt-0.6b-v3`               | See Models below                                                             |
| `-f, --format <text\|json\|srt>` | `text`                               |                                                                               |
| `--device <auto\|coreml\|cpu>`   | `auto`                                | `auto` picks CoreML on Apple Silicon, CPU elsewhere                          |
| `--cache-dir <PATH>`             | OS cache dir                         | Where models are stored/looked up                                           |
| `--list-models`                  | —                                     | Prints every model, its cache state, and the default; exits without transcribing |
| `--refresh-model`                | —                                     | Deletes and re-downloads the selected model's cache before running          |

Environment variable overrides (used when the matching flag isn't passed): `PARA_MODEL`,
`PARA_FORMAT`, `PARA_DEVICE`, `PARA_CACHE_DIR`.

There is deliberately no standalone "remove a cached model" command — only `--refresh-model`,
which always re-fetches afterward.

### Models

| ID                      | Language              | Timing                          | When to use it                                    |
| ------------------------ | --------------------- | -------------------------------- | -------------------------------------------------- |
| `parakeet-tdt-0.6b-v3`   | 25 European languages, auto-detected | Phrase-level segments | Default — best accuracy, broadest language coverage |
| `parakeet-tdt-0.6b-v2`   | English only          | Phrase-level segments            | Same accuracy tier as v3, kept for compatibility   |
| `parakeet-ctc-0.6b`      | English only          | Whole-file only (one segment)    | Fastest tier — single forward pass, no per-word timestamps; noticeably faster than the TDT tier on longer inputs, though the gap is masked by fixed model-load time on very short clips |

`--format json`/`srt` on the CTC tier still produces valid, schema-conformant output — just as one
segment spanning the whole file rather than per-phrase timestamps, since CTC decoding has no
duration/timing head. `para` prints a note to stderr when this applies.

## Output formats

- `text` (default): the transcript and nothing else, one trailing newline.
- `json`: `{"text", "segments": [{"start","end","text"}], "model", "duration_seconds"}` — see
  `specs/001-media-transcription/contracts/output-json-schema.json`.
- `srt`: standard SRT subtitle blocks (comma-separated milliseconds) — see
  `specs/001-media-transcription/contracts/output-srt.md`.

stdout carries only the requested output — no banners, no progress. Everything else (download
progress, the CoreML compile note, per-chunk `"transcribing chunk N of M"` progress on long
inputs, and all errors) goes to stderr.

## Development

```bash
task rust:test           # unit + contract tests (no model download required)
task rust:integration     # + tests that need a real cached model (macOS `say` generates fixtures)
task rust:lint            # clippy + fmt check
```

## License

Apache-2.0

## Author Information

- Justin Lecher <justin@jlec.de>
