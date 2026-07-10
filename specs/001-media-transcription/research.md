# Phase 0 Research: Local Audio & Video Transcription

Each item below resolves one unknown from the Technical Context, or reconciles a decision in the
user-supplied prior technical spec against `spec.md`'s clarifications and `constitution.md`.
Where a prior-spec decision aligned, it's carried forward with a note. Where it was silent, thin,
or out of step, it's resolved here in favor of the clarified spec and flagged.

## 1. Language and ONNX binding strategy

**Decision**: Rust, 2024 edition, using the `ort` crate for ONNX Runtime bindings.

**Rationale**: Confirmed against `ort`'s current docs (docs.rs/ort, 2.0 RC series) — the crate is
live, actively developed, and its `Session::builder()?.commit_from_file(...)` pattern (the prior
spec's usage) matches the current 2.x API shape. The 1.x-style `Environment` struct and the
`ExecutionProvider::CoreML(...)` enum-variant call shown in the prior spec are from the 1.x API;
2.x restructures execution providers into their own module (named `ep` as of the latest docs.rs
snapshot checked, but this has moved between RCs). **Flag**: do not hand-transcribe the prior
spec's exact `ort` code — re-derive the CoreML/CPU execution-provider construction calls from the
docs.rs page for whatever exact `ort` version `cargo add` resolves at implementation time.

**Alternatives considered**: Go with cgo bindings to ONNX Runtime or a Python sidecar process —
rejected per the user's own rationale (simpler binding story) and because a Python sidecar would
violate Constitution Principle I (no daemon/subprocess architecture) and Principle VII (Python
runtime as an extra manual dependency).

## 2. ONNX Runtime linking strategy (single-binary implications)

**Decision**: Use `ort`'s `load-dynamic` feature with the `ORT_DYLIB_PATH` escape hatch, and ship
the resolved `libonnxruntime.dylib`/`.so` alongside the `para` binary in release archives.

**Rationale**: This is a correction to the prior spec, not a carry-forward. The prior spec assumed
`download-binaries` + `copy-dylibs` yields something close to a single static binary; verified
against `ort`'s own docs (`ort.pyke.io/setup/linking`, via search) this is not quite right:
- Static linking is only available where the execution provider itself supports a static build;
  prebuilt CoreML-capable ONNX Runtime binaries are distributed as shared libraries, not static
  archives.
- `copy-dylibs` copies the shared library into the Cargo target/`OUT_DIR` so `cargo build`/`cargo
  run` work out of the box for a developer building from source — it does not fuse the dylib into
  the executable. The docs explicitly recommend `load-dynamic` + `ORT_DYLIB_PATH` for anyone
  distributing a binary, precisely because it makes the dependency on a co-located shared library
  explicit and controllable instead of relying on target-folder side effects.

Constitution Principle VII allows this: "every other runtime dependency MUST be statically linked
**or fetched automatically at build time**." The ONNX Runtime shared library is fetched
automatically at build time (via `ort`'s downloader) — the user never installs it by hand. The one
consequence to flag explicitly (as the user asked): **a prebuilt `para` release is two files, not
one** (the executable plus its `libonnxruntime.*` dylib, or a fixed relative layout the binary
looks up via `ORT_DYLIB_PATH`/rpath). This is a packaging detail for release tooling
(`make release-all` / GitHub releases), not a violation — building from source via `cargo build
--release` remains a single command with no manual library install.

**Alternatives considered**: Full static linking — rejected, not supported by the prebuilt
CoreML-capable ORT binaries `ort` fetches. Vendoring a source build of ONNX Runtime for true static
linking — rejected as disproportionate build-time complexity for a v1 CLI; revisit only if the
two-file distribution proves to be a real adoption blocker.

## 3. Primary model and registry

**Decision**: `parakeet-tdt-0.6b-v3` (multilingual, auto language detection, word-level timestamps)
as the default model, sourced from `istupakov/parakeet-tdt-0.6b-v3-onnx` on HuggingFace, plus at
least two lighter/English-only CTC alternatives, satisfying spec.md FR-008's "at least three
model options."

**Rationale**: Confirmed via web search that `istupakov/parakeet-tdt-0.6b-v3-onnx` is a real,
actively maintained HuggingFace repository — an ONNX export of NVIDIA's Parakeet TDT 0.6B v3 for
the `onnx-asr` project, 25-European-language multilingual support, CC-BY-4.0 licensed. This is not
a fabricated repo reference.

**Flag — known tradeoff to carry into data-model.md**: per the prior spec, CTC-family models
decode without word/segment-level timing (one greedy pass, no duration head), so they produce a
single `Segment` spanning the whole file rather than phrase-level segments. This technically
satisfies FR-005/FR-006 (an ordered, non-overlapping set of one is still ordered and
non-overlapping) but delivers a low-resolution subtitle/structured-output experience on the
faster/CTC tiers. This is disclosed to the user via `--list-models` descriptions rather than
hidden, and is recorded as a `timing_granularity` attribute on `ModelOption` in data-model.md.

**Decision (registry size)**: Ship 3 models at launch — `parakeet-tdt-0.6b-v3` (default) plus two
CTC alternates — not the prior draft's 4 (which also included `parakeet-tdt-0.6b-v2`). This
satisfies FR-008's "at least three" without committing to a second TDT variant that exists in the
prior draft only for backward-compatibility reasons that don't apply to a v1 tool with no prior
release. **Alternatives considered**: carry `parakeet-tdt-0.6b-v2` forward as a 4th entry —
rejected for v1; nothing in spec.md or the clarification session calls for a second multilingual
tier, and an extra ~670MB download-on-first-use for a model with no compatibility need to serve is
scope the tool doesn't require yet. Revisit if a concrete reason to keep v2 available surfaces
(e.g., a regression in v3 for a specific language).

**Exact file names, tensor names, and checksums are explicitly NOT resolved here.** Per
Constitution Principle V, checksums must be computed from the actual downloaded files and tensor
names read from the actual ONNX graphs — both are implementation-time (Phase 3/tasks) steps, not
plan-time guesses. Hardcoding a plausible-looking SHA256 or tensor name now would itself be the
kind of fabrication Principle V prohibits.

## 4. Tokenizer crate feature set

**Decision**: Use the `tokenizers` crate with default features; drop the prior spec's
`default-features = false, features = ["onig"]`.

**Rationale**: Verified via search that `onig` (Oniguruma regex engine) is a legacy,
not-recommended option in HuggingFace-family tokenizer crates — it exists for pretokenizer regex
behavior some older models need, adds a native C library dependency, and has worse runtime
performance than the default pure-Rust regex path. A SentencePiece/Unigram tokenizer.json (what
Parakeet ships) uses a `Metaspace` pretokenizer, not the legacy regex split `onig` exists for.
Dropping it removes a native-library build dependency that added nothing for this model family —
directly in the spirit of Constitution Principle VII and the Engineering Standards' "prefer
well-maintained crates... for tokenization."

**Alternatives considered**: Keep `onig` for safety — rejected; it's an unjustified dependency
addition with no identified model requirement driving it.

## 5. Mel spectrogram parameters

**Decision**: Deferred to implementation-time verification against the NeMo
`AudioToMelSpectrogramPreprocessor` config actually shipped with `parakeet-tdt-0.6b-v3`; the
parameter table in the prior spec (16kHz, 25ms window / 10ms hop, 512 FFT, 80 mel bins, HTK scale,
0.97 pre-emphasis, 1e-5 log floor) is plausible and consistent with typical NeMo Conformer
preprocessing configs, but this plan does not treat it as verified fact.

**Rationale**: I do not have a way to fetch and inspect the actual NeMo config or ONNX graph in
this planning phase. Committing to specific DSP constants without checking them against the real
model config would be exactly the kind of invented technical detail Principle V is meant to
prevent — wrong mel parameters produce a model that runs without erroring but transcribes garbage,
which is a worse failure mode than a build error because it violates Principle IV's "never guess
and proceed" in spirit even though no exception is thrown. Verification is a concrete task in
tasks.md, not a plan-time decision.

## 6. Chunking threshold for long inputs (FR-023)

**Decision**: No fixed chunk-length threshold or overlap strategy is committed to in this plan.
The threshold above which an input requires chunked (multi-pass) encoding must be determined
empirically against the actual ONNX encoder's memory/latency behavior during implementation.

**Rationale**: Same principle as §5 — a specific number (e.g., "20 minutes") would be invented
without empirical grounding. What *is* fixed here, because it's a clarified requirement rather
than a technical constant: whenever chunking is used, para emits `"transcribing chunk N of M"` to
stderr per chunk (spec.md FR-023), and single-pass inputs must not be forced to chunk just to
produce progress output.

## 7. Download retry and backoff (FR-022)

**Decision**: 3 attempts total, exponential backoff between attempts (e.g., 1s, 2s), then fail with
a specific error and non-zero exit if the final attempt fails. No fallback to a different cached
model at any point in the retry sequence.

**Rationale**: Spec.md's clarification fixes the *behavior* (bounded retries, then loud failure,
never silent substitution) but leaves the exact bound to implementation. Three attempts with short
exponential backoff is a conventional default for a CLI tool (long enough to survive a transient
blip, short enough that a genuinely offline user isn't stuck waiting minutes before getting the
clear error they need to act on).

**Alternatives considered**: Unbounded retry with backoff cap — rejected, contradicts "fail fast"
(Principle IV) for the common case of a user who is simply offline and needs to know that
immediately-ish, not after an indefinite hang.

## 8. Error handling strategy

**Decision**: `anyhow` for propagation and the top-level `main` handler (single `eprintln!` +
`std::process::exit` site); `thiserror` for a small typed error enum inside the model manager,
specifically to let the retry loop (FR-022) distinguish a retryable transport error from a
terminal one (e.g., checksum mismatch on a fully-downloaded file is not something a retry fixes by
itself — it should trigger a clean re-download, not a silent partial-content retry).

**Rationale**: Matches Constitution Engineering Standards ("no panics in library code; errors
propagate via `Result` and are handled at the top level only") and is the one place in the prior
spec's design where a typed error actually earns its keep — everywhere else a single `anyhow::Error`
per function is sufficient and simpler.

**Alternatives considered**: `thiserror` everywhere — rejected as unnecessary ceremony for a
single-binary (non-library) crate; `anyhow` everywhere including the model manager — rejected
because the retry loop needs to match on error *kind*, which a type-erased `anyhow::Error` makes
awkward.

## 9. Cross-compilation tooling (flagged per explicit request)

**Finding**: Building `linux/amd64` release binaries from a macOS development machine requires a
cross-linker toolchain (e.g., the `cross` tool + Docker, or an equivalent cross-compilation
toolchain) that is **not** part of what an end user installs to *run* para — it's a maintainer/CI
concern for producing release artifacts, not a runtime dependency covered by Principle VII (which
is scoped to what "the user must install themselves" to use the tool). It's called out here, and
must be called out again in the Makefile/README, so it's never silently assumed to be present on
a contributor's machine. No action needed against Principle VII since it doesn't touch the
end-user runtime footprint.

**Finding**: The `ort` crate's `download-binaries` feature needs network access at *build* time
(fetching the ORT binary the first time `para` is compiled from source), which is a different
phase than the runtime network access Constitution Principle II governs. This is normal for a
Rust project (equivalent to `cargo build` fetching crates.io dependencies) and not a Principle II
violation, but it does mean an air-gapped *build* environment needs the `ORT_DYLIB_PATH` escape
hatch (§2) to point at a manually-provided ONNX Runtime library. Document this explicitly in the
README rather than assuming every build environment has outbound network access.

## Summary of changes from the prior technical spec

| Area | Prior spec | Resolved here | Why |
|---|---|---|---|
| ORT linking | `download-binaries` + `copy-dylibs` implied near-single-binary | `load-dynamic` + `ORT_DYLIB_PATH`, dylib shipped alongside the binary in releases | `copy-dylibs` is a dev-convenience for `cargo run`, not a distribution strategy; verified against `ort`'s own linking docs |
| `ort` API code samples | 1.x-style `Environment`/`ExecutionProvider::CoreML(...)` | Re-derive from docs.rs for the pinned version at implementation time | Prior sample is 1.x API; 2.x has moved the execution-provider module at least once across RCs |
| `tokenizers` features | `default-features = false, features = ["onig"]` | Default features, no `onig` | `onig` is a legacy, not-recommended, native-dependency option with no identified need for this model's tokenizer.json |
| Model management scope | Implicit only in the narrative goals | Listing + `--refresh-model` explicitly in scope; standalone remove explicitly out of scope | Matches spec.md clarification session, not assumed |
| Registry size | 4 models (`tdt-v3`, `tdt-v2`, `ctc-0.6b`, `ctc-1.1b`) | 3 models — drops `tdt-v2` | No spec.md or clarification requirement calls for a second multilingual tier; FR-008 only needs 3 (§3) |
| Download failure behavior | Single generic "download failed" error, no retry described | Bounded retries (3, exponential backoff) then loud failure, never a silent model fallback | Matches spec.md FR-022 clarification |
| Progress reporting | Progress bar (`indicatif`) implied for all downloads; no mention of transcription-time progress | Download progress via `indicatif` (stderr) unchanged; added explicit per-chunk `"transcribing chunk N of M"` stderr output for inputs requiring chunked encoding, none required for single-pass inputs | Matches spec.md FR-023 clarification, which the prior spec predates |
| Mel spectrogram parameters, tensor names, checksums, chunk threshold | Presented as settled implementation detail | Explicitly deferred to implementation-time verification | Constitution Principle V — must be verified against real sources, not asserted at plan time |
