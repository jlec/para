# para

Local, offline audio and video transcription powered by [NVIDIA Parakeet](https://huggingface.co/nvidia)
speech models running natively on the Apple Neural Engine via
[FluidAudio](https://github.com/FluidInference/FluidAudio)'s real CoreML model conversions. Single
binary, no daemon, no cloud API — point it at a file, get a transcript, in seconds, using a few
hundred MB of memory.

## Prerequisites

- macOS on Apple Silicon (this tool is not portable to other platforms)
- `ffmpeg` on `PATH` (the only dependency you install by hand — `brew install ffmpeg`)
- Xcode Command Line Tools (`xcode-select --install`) — `para`'s inference backend links
  [FluidAudio](https://github.com/FluidInference/FluidAudio) directly via a small Swift package,
  built at compile time
- Rust 2024 edition (MSRV 1.85+) to build from source
- Network access for the first build (fetches the FluidAudio Swift package) and the first use of
  any given model (downloads it to a local cache) — nothing after that

## Build

```bash
task rust:build      # debug build
task rust:release    # optimized single binary: target/release/para
```

`build.rs` compiles `swift/` (a small Swift package, `ParaBridge`, depending on FluidAudio) and
statically links the result into the `para` binary — `cargo build` handles this automatically, no
separate build step.

## First run

```bash
para -i lecture.mp3
```

The first time you use a given model, `para` downloads it with a progress indicator on stderr —
stdout stays clean throughout. After that first run, everything is offline. Model files are cached
by FluidAudio itself in `~/Library/Application Support/FluidAudio/Models/` — not under `para`'s own
cache directory, and not affected by `--cache-dir`/`PARA_CACHE_DIR`.

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

| Flag                             | Default | Notes                                                                        |
| -------------------------------- | ------- | ----------------------------------------------------------------------------- |
| `-i, --input <PATH>`             | stdin   | Omit and pipe bytes in instead                                               |
| `-o, --output <PATH>`            | stdout  |                                                                               |
| `-m, --model <ID>`               | `parakeet-tdt-0.6b-v3` | See Models below                                              |
| `-f, --format <text\|json\|srt>` | `text`  |                                                                               |
| `--device <auto\|coreml\|cpu>`   | `auto`  | `auto`/`coreml` use the Apple Neural Engine; `cpu` forces CPU-only inference (useful for benchmarking/troubleshooting) |
| `--list-models`                  | —       | Prints every model, its cache state, and the default; exits without transcribing |
| `--refresh-model`                | —       | Deletes and re-downloads the selected model's cached files                  |
| `--no-progress`                  | off     | Suppresses all progress output on stderr; errors are unaffected             |

Environment variable overrides (used when the matching flag isn't passed): `PARA_MODEL`,
`PARA_FORMAT`, `PARA_DEVICE`. `PARA_NO_PROGRESS` (any non-empty value) has the same effect as
`--no-progress`.

There is deliberately no standalone "remove a cached model" command — only `--refresh-model`,
which always re-fetches afterward.

### Models

| ID                    | Language                              | When to use it                                      |
| ---------------------- | -------------------------------------- | ---------------------------------------------------- |
| `parakeet-tdt-0.6b-v3` | 25 European languages, auto-detected   | Default — best accuracy, broadest language coverage |
| `parakeet-tdt-0.6b-v2` | English only                           | Same accuracy tier as v3, kept for compatibility     |

Both models produce phrase/paragraph-level timestamps.

## Output formats

- `text` (default): the transcript, with filler words ("um"/"uh") removed and paragraph breaks
  inserted at natural pauses — one trailing newline, nothing else.
- `json`: `{"text", "segments": [{"start","end","text"}], "model", "duration_seconds"}` — see
  `specs/001-media-transcription/contracts/output-json-schema.json`.
- `srt`: standard SRT subtitle blocks (comma-separated milliseconds) — see
  `specs/001-media-transcription/contracts/output-srt.md`.

stdout carries only the requested output — no banners, no progress. Everything else goes to
stderr: model-download progress, and a brief "loading model"/"transcribing" indicator on every run
— an animated spinner when stderr is an interactive terminal, or a single plain-text line when it
isn't (redirected to a file, `TERM=dumb`/unset, or piped into another program). Pass
`--no-progress` (or set `PARA_NO_PROGRESS`) to suppress all of this; errors are reported either
way.

## Performance

Native CoreML inference (the default) is dramatically faster and lighter than a CPU-bound ONNX
Runtime pipeline: a ~26-minute recording transcribes in around 6 seconds using roughly 200MB of
peak memory, on Apple Silicon.

## Development

```bash
task rust:test           # unit + contract tests (no model download required)
task rust:integration    # + tests that need real cached models
task rust:lint           # clippy + fmt check
```

## License

Apache-2.0

## Author Information

- Justin Lecher <justin@jlec.de>
