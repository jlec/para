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

**Decision (registry size) — corrected again during implementation**: Ship the 3 models that
actually exist and are verified real on `istupakov`'s HuggingFace profile (checked directly, not
inferred): `parakeet-tdt-0.6b-v3` (default), `parakeet-tdt-0.6b-v2`, and `parakeet-ctc-0.6b`.

This reverses the immediately preceding version of this decision, which planned to drop
`parakeet-tdt-0.6b-v2` in favor of "two CTC alternates" — a plan-time guess made before checking
whether a second CTC-family Parakeet ONNX export actually exists. It doesn't: `istupakov`'s profile
lists exactly one CTC variant (`parakeet-ctc-0.6b-onnx`) alongside `parakeet-rnnt-0.6b-onnx` (a
different decode architecture, not part of this tool's TDT/CTC `ModelKind` design) and no
`ctc-1.1b` of any kind — that entry in the original prior-draft spec (`locate via HuggingFace
search at implementation time`) does not resolve to a real repo. Rather than invent one or bolt on
an unrelated architecture, this reverts to the verified 3: two TDT variants (differing mainly in
language coverage — v3 multilingual, v2 English-only — rather than speed, since they share the
same architecture and size) plus the one real CTC variant, which is the genuinely faster,
lower-accuracy tier (single forward pass, no autoregressive decode). This still satisfies FR-008's
"at least three... trading processing speed against transcription accuracy."

**Alternatives considered**: A second, invented CTC model — rejected outright, this is exactly the
kind of fabrication Constitution Principle V prohibits. `parakeet-rnnt-0.6b-onnx` as a 4th/replacement
entry — rejected for v1; RNNT is a third decode architecture this tool doesn't implement, and adding
it widens scope (a third decoder implementation) beyond what FR-008 requires.

**Exact file names, tensor names, and checksums are explicitly NOT resolved here.** Per
Constitution Principle V, checksums must be computed from the actual downloaded files and tensor
names read from the actual ONNX graphs — both are implementation-time (Phase 3/tasks) steps, not
plan-time guesses. Hardcoding a plausible-looking SHA256 or tensor name now would itself be the
kind of fabrication Principle V prohibits.

## 4. Tokenizer crate feature set

> **⚠ SUPERSEDED 2026-07-10 — see "10. ONNX-native preprocessing and vocab-based decoding" below.**
> The premise of this entire section (that a HuggingFace `tokenizer.json` exists and the
> `tokenizers` crate is the right tool) turned out to be wrong once the real model files were
> checked: there is no `tokenizer.json`, only a plain `vocab.txt`. Left in place, unedited, as a
> record of what was believed at the time and why — see §10 for what actually ships and what
> replaces this decision.

**Decision (corrected during implementation — see below)**: Use the `tokenizers` crate's actual
default features (`progressbar`, `onig`, `esaxx_fast`) as resolved by `cargo add tokenizers`
against the real crate manifest. Do not pass `default-features = false`.

**Correction**: The original version of this section (written during planning, before any real
build was attempted) claimed `onig` was an opt-in, not-recommended feature and planned to drop it
via `default-features = false, features = ["onig"]` — inverted from what was intended (that
line would have _kept_ onig while dropping `progressbar`/`esaxx_fast`, the opposite of the stated
goal). Worse, the underlying premise was wrong: inspecting `tokenizers` 0.23.1's actual
`Cargo.toml` (`cargo add` then reading the resolved manifest) shows `onig` is part of `default =
["progressbar", "onig", "esaxx_fast"]` for _this_ crate — the "legacy, not-recommended" finding
from the original web search was real, but for a different crate (`kitoken`'s `regex-onig`
feature), not `tokenizers` itself. This is exactly the failure mode Constitution Principle V
warns about: a plausible-sounding claim that wasn't checked against the actual dependency once it
was addable, and it slipped through this document's own review because it matched what the prior
draft spec assumed.

**Rationale for keeping the default (including `onig`) now**: Whether Parakeet's tokenizer.json
actually needs `onig`'s regex engine depends on its pretokenizer type, which is only knowable by
inspecting the real downloaded `tokenizer.json` (Metaspace pretokenizers, as Unigram/SentencePiece
models typically use, don't need it; some BPE pretokenizers with lookaround regex patterns do).
That inspection hasn't happened yet — no model has been downloaded. Guessing `default-features =
false` again without that inspection would repeat the same mistake in the opposite direction.
`cargo build` with full defaults (including `onig`) succeeds in this environment (`onig_sys` builds
via `cc`/`pkg-config` at compile time — a build-time native-toolchain dependency, not a runtime
one, so Constitution Principle VII's _runtime_-dependency clause isn't implicated either way).

**Alternatives considered**: `default-features = false, features = ["esaxx_fast", "progressbar"]`
(drop `onig`) — deferred, not rejected: revisit once the real `tokenizer.json` is downloaded (task
T030/T022-adjacent work) and its pretokenizer type can be read directly, rather than assumed.

## 5. Mel spectrogram parameters

> **⚠ SUPERSEDED 2026-07-10 — see "10. ONNX-native preprocessing and vocab-based decoding" below.**
> The premise of this section (that mel-spectrogram extraction must be hand-implemented in Rust
> and its parameters verified against NeMo source) turned out to be avoidable: the model repo ships
> its own ONNX graph that does this extraction. Left in place, unedited, as a record of what was
> believed at the time and why — see §10 for what actually ships and what replaces this decision.

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
without empirical grounding. What _is_ fixed here, because it's a clarified requirement rather
than a technical constant: whenever chunking is used, para emits `"transcribing chunk N of M"` to
stderr per chunk (spec.md FR-023), and single-pass inputs must not be forced to chunk just to
produce progress output.

## 7. Download retry and backoff (FR-022)

**Decision**: 3 attempts total, exponential backoff between attempts (e.g., 1s, 2s), then fail with
a specific error and non-zero exit if the final attempt fails. No fallback to a different cached
model at any point in the retry sequence.

**Rationale**: Spec.md's clarification fixes the _behavior_ (bounded retries, then loud failure,
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
because the retry loop needs to match on error _kind_, which a type-erased `anyhow::Error` makes
awkward.

## 9. Cross-compilation tooling (flagged per explicit request)

**Finding**: Building `linux/amd64` release binaries from a macOS development machine requires a
cross-linker toolchain (e.g., the `cross` tool + Docker, or an equivalent cross-compilation
toolchain) that is **not** part of what an end user installs to _run_ para — it's a maintainer/CI
concern for producing release artifacts, not a runtime dependency covered by Principle VII (which
is scoped to what "the user must install themselves" to use the tool). It's called out here, and
must be called out again in the README (this repo drives Rust builds through the existing
`Taskfile.yml`'s `rust:*` tasks, not a separate Makefile), so it's never silently assumed to be
present on a contributor's machine. No action needed against Principle VII since it doesn't touch
the end-user runtime footprint.

**Finding**: The `ort` crate's `download-binaries` feature needs network access at _build_ time
(fetching the ORT binary the first time `para` is compiled from source), which is a different
phase than the runtime network access Constitution Principle II governs. This is normal for a
Rust project (equivalent to `cargo build` fetching crates.io dependencies) and not a Principle II
violation, but it does mean an air-gapped _build_ environment needs the `ORT_DYLIB_PATH` escape
hatch (§2) to point at a manually-provided ONNX Runtime library. Document this explicitly in the
README rather than assuming every build environment has outbound network access.

## 10. ONNX-native preprocessing and vocab-based decoding (found 2026-07-10, during Foundational implementation)

**Context**: While starting T010 (model registry) and T015 (mel extraction), I fetched the real
file listing for `istupakov/parakeet-tdt-0.6b-v3-onnx` and `istupakov/parakeet-ctc-0.6b-onnx` from
the HuggingFace API directly (not a scraped/summarized page) and read the reference Python
implementation (`istupakov/onnx-asr`, the same author's package these ONNX exports are built for)
on GitHub via `gh api`. Both superseded §4 and §5 above.

**Finding 1 — no `tokenizer.json`**: Each model repo ships `config.json` and `vocab.txt`, not a
HuggingFace-format `tokenizer.json`. `vocab.txt` is a plain newline-delimited list of SentencePiece
pieces (one per line, line number = token id). Decoding is: look up each token id's line, join the
pieces, replace the SentencePiece `▁` marker with a space. The `tokenizers` crate (which expects
`tokenizer.json`) has no file to load here — it was never applicable to this model family, and §4's
entire premise (an "onig" feature question) was moot from the start, not just the feature choice.

**Decision**: Drop the `tokenizers` crate dependency entirely (`cargo remove tokenizers`, already
done). Implement vocab lookup + `▁`→space decoding directly — a few dozen lines, no dependency,
in the model resource loading path (was T018, now rewritten in tasks.md).

**Finding 2 — mel-spectrogram extraction ships as its own ONNX graph**: Each model repo also
includes a preprocessor file (`nemo128.onnx` for the TDT v3 repo). Reading `onnx_asr`'s
`preprocessors/preprocessor.py`, `OnnxPreprocessor` loads this file into its own
`onnxruntime.InferenceSession` and runs it directly on raw waveform samples: inputs
`waveforms`/`waveforms_lens`, outputs `features`/`features_lens`. There is no hand-computed
FFT/mel-filterbank math anywhere in the reference implementation for this preprocessor path — the
model author ships the feature extraction as a portable ONNX graph precisely so consumers don't
have to reimplement and verify NeMo's DSP parameters themselves.

**Decision**: Run the bundled preprocessor `.onnx` file as a second `ort` session (raw waveform
samples in, mel-feature tensor out) instead of hand-implementing mel extraction with `rustfft`.
Drop `rustfft` (its only purpose was this DSP) and `ndarray` (added for the same reason; `ort`'s
own tensor/`Value` construction from `(shape, data)` doesn't require it, and nothing else in this
codebase needs N-dimensional array operations — reconsider only if the encoder/decoder tensor
plumbing turns out to genuinely need it once T022/T030 are implemented against real tensor shapes).
Both removed via `cargo remove rustfft ndarray tokenizers`.

**Rationale for the pivot overall**: This eliminates the single largest correctness risk this plan
carried — Principle V already flagged that wrong mel parameters would produce a model that runs
without erroring but transcribes garbage, and the only mitigation available at planning time was
"verify carefully before implementing." Verification is no longer needed for the DSP math itself,
because there's no DSP math left to get wrong — the model author's own graph does it. This is a
strictly better outcome than the original plan, reached by checking the actual downloadable files
instead of continuing to plan around an assumption.

**What changes downstream**: `src/inference/engine.rs` now orchestrates up to 3 ONNX sessions per
model (preprocessor → encoder → decoder-joint for TDT; preprocessor → encoder only for CTC) instead
of 1 session plus hand-rolled DSP feeding it. `src/inference/mel.rs` keeps its name (minimal diff)
but its job changes from "compute mel features" to "load and run the bundled preprocessor ONNX
graph." None of `contracts/` (CLI surface, JSON schema, SRT format) changes — this is purely an
internal pipeline detail, invisible at the spec/output level. Tensor names/shapes for the
preprocessor graph are, like the encoder/decoder, explicitly **not** resolved here — read from the
real downloaded `.onnx` file at implementation time (same Principle V discipline as §3).

**Addendum, same day — where the preprocessor graph actually comes from**: Checking each model's
own HuggingFace repo file listing shows the preprocessor file is _not_ reliably present per model:
`parakeet-tdt-0.6b-v3`/`-v2` both bundle `nemo128.onnx`, but `parakeet-ctc-0.6b` bundles no
preprocessor at all, and no repo in this family bundles the 80-feature preprocessor CTC actually
needs (`config.json`'s `features_size` field: 128 for both TDT variants, 80 for CTC — read via
`_NemoConformer._preprocessor_name` in `onnx_asr/models/nemo.py`, which resolves to
`f"nemo{features_size}"`). The one place both `nemo128.onnx` and `nemo80.onnx` are reliably and
directly available is the versioned `onnx-asr` PyPI wheel itself (`onnx_asr/preprocessors/data/`),
where the Python reference implementation loads them from as bundled package data.

**Decision**: Vendor both files directly into the `para` source tree (`assets/preprocessors/`,
loaded via `include_bytes!`) rather than downloading either at runtime. Both are tiny (~139KB and
~87KB — confirmed by extracting the real `onnx_asr-0.11.0-py3-none-any.whl` and checking file
sizes), MIT-licensed (`onnx-asr`, copyright Ilya Stupakov — license text and real SHA-256 checksums
recorded in `assets/preprocessors/NOTICE.md`), and shared across the whole model family rather than
being per-model unique weights. This is a strictly better fit for Constitution Principle II than
even the normal "download on first use" path: these two files need no network access ever, not even
once, and Principle VII's "statically linked or fetched automatically at build time" is satisfied
by literal static linking (compiled into the binary) rather than a build-time fetch.

**Alternatives considered**: Downloading the PyPI wheel at runtime and extracting the needed file —
rejected; it would add a zip-reading dependency and a second, inconsistent download mechanism
(archive-extraction vs. every other model file's direct-binary-download) for two files that are
smaller than this section of research.md. Fetching `nemo128.onnx` directly from the TDT HF repos
(reusing the normal model-file download path) while leaving `nemo80.onnx` unsolved — rejected, it
doesn't actually solve the CTC case and produces two different sourcing strategies across models
for no benefit. Dropping CTC support to avoid the problem — rejected, violates FR-008's
at-least-three-models requirement.

**Consequence for `data-model.md`**: the preprocessor is no longer one of `ModelOption`'s
downloaded/cached `files` — it's selected at compile time by `ModelKind`/`features_size` from the
two vendored assets, never entering the download-and-cache flow at all.

**Correction, 2026-07-11 — vendoring reversed in favor of a build-time fetch**: The decision above
committed the two `.onnx` files directly into git (`assets/preprocessors/`). This conflicted with
the repo's own pre-existing `forbid-binary` pre-commit policy (only one prior carve-out existed,
for `docs/images/`), which is a deliberate constraint, not an oversight — reverted rather than
carved out a second exception for it.

**Decision**: `build.rs` downloads the same `onnx-asr` PyPI wheel at *build* time (not runtime),
verifies the wheel's SHA-256 and each extracted file's SHA-256 against the same real values
recorded above, writes `nemo128.onnx`/`nemo80.onnx` into `OUT_DIR`, and `src/inference/mel.rs`
embeds them from there via `include_bytes!(concat!(env!("OUT_DIR"), "/nemo128.onnx"))`. This keeps
every property the vendoring decision was for — zero runtime network calls, no separate
cache-state tracking for these two files, one static binary at the end — while keeping the git
repository itself free of committed binaries. It's a direct instance of Constitution Principle
VII's "fetched automatically at build time" clause, the same mechanism the `ort` crate itself
already uses (via its `download-binaries` feature) to obtain the ONNX Runtime library — so this
isn't a new pattern for the build, just the same one applied to a second artifact.

`assets/preprocessors/` (the vendored files, `NOTICE.md`, `LICENSE`) is removed entirely; the
provenance/license/checksum record that lived in `NOTICE.md` is now inlined as doc comments and
constants directly in `build.rs`, next to the values it actually checks against.

**Alternatives reconsidered**: Keep vendoring and add a `forbid-binary` exclusion for these two
files — rejected on reflection; a project-wide "no binaries" policy is exactly the kind of
constraint that shouldn't grow silent exceptions for convenience. Download at first *use* (runtime)
via the model manager's existing download/cache machinery — viable, but build-time is strictly
better here: it guarantees the files exist before the binary can even be produced (no first-run
network dependency at all, not even a one-time one), and avoids adding these two files to the
per-model cache-state bookkeeping that section 10's original decision already took pains to keep
them out of.

**Correction, 2026-07-11 — `vocab.txt`'s actual line format, checked while implementing T018**:
§10's Finding 1 described `vocab.txt` as "a plain newline-delimited list of SentencePiece pieces
(one per line, line number = token id)" — downloading the real file
(`istupakov/parakeet-tdt-0.6b-v3-onnx/vocab.txt`, 8193 lines) shows each line is actually
`<piece> <id>` (space-separated, trailing numeric id), not a bare piece. The id happens to equal
the 0-based line number for all 8193 lines (verified directly, no mismatches), so the *decoding*
logic §10 described (look up by id, join, `▁`→space) is still correct — but loading code that
naively treated the whole line as the piece would embed a trailing `" <n>"` in every piece. Also
worth recording: the file includes non-speech control tokens (`<unk>`, `<pad>`, per-language
`<|xx|>` tags, `<|timestamp|>`, etc.) and a blank token `<blk>` at the final id (8192) — the CTC/TDT
blank used by the decode loop (T022/T030), not part of emitted text.

**Decision**: `src/inference/decoder.rs`'s vocab loader splits each line on the last space to
recover `(piece, id)`, asserts `id == line_number` (fail loud on a malformed file rather than
silently misaligning the table), and locates the blank token by name (`<blk>`) rather than
hardcoding its numeric id, since nothing in the reference implementation guarantees blank is always
the last entry for every model in the registry.

## Summary of changes from the prior technical spec

| Area                                                                 | Prior spec                                                                                      | Resolved here                                                                                                                                                                                                                                               | Why                                                                                                                                                                                  |
| -------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| ORT linking                                                          | `download-binaries` + `copy-dylibs` implied near-single-binary                                  | `load-dynamic` + `ORT_DYLIB_PATH`, dylib shipped alongside the binary in releases                                                                                                                                                                           | `copy-dylibs` is a dev-convenience for `cargo run`, not a distribution strategy; verified against `ort`'s own linking docs                                                           |
| `ort` API code samples                                               | 1.x-style `Environment`/`ExecutionProvider::CoreML(...)`                                        | Re-derive from docs.rs for the pinned version at implementation time                                                                                                                                                                                        | Prior sample is 1.x API; 2.x has moved the execution-provider module at least once across RCs                                                                                        |
| `tokenizers` features                                                | `default-features = false, features = ["onig"]`                                                 | Full defaults kept (`onig` included) — corrected mid-implementation after finding `onig` is actually a _default_ feature of this crate, not opt-in as first assumed; dropping it is deferred until the real tokenizer.json's pretokenizer type is inspected | Avoid repeating an unverified claim in the opposite direction (§4)                                                                                                                   |
| Model management scope                                               | Implicit only in the narrative goals                                                            | Listing + `--refresh-model` explicitly in scope; standalone remove explicitly out of scope                                                                                                                                                                  | Matches spec.md clarification session, not assumed                                                                                                                                   |
| Registry size                                                        | 4 models (`tdt-v3`, `tdt-v2`, `ctc-0.6b`, `ctc-1.1b`)                                           | 3 models — `tdt-v3`, `tdt-v2`, `ctc-0.6b`; drops the invented `ctc-1.1b`                                                                                                                                                                                    | `ctc-1.1b` doesn't exist on `istupakov`'s HuggingFace profile (verified directly); `tdt-v2` does, so it's kept rather than replaced with a second nonexistent CTC model (§3)         |
| Download failure behavior                                            | Single generic "download failed" error, no retry described                                      | Bounded retries (3, exponential backoff) then loud failure, never a silent model fallback                                                                                                                                                                   | Matches spec.md FR-022 clarification                                                                                                                                                 |
| Progress reporting                                                   | Progress bar (`indicatif`) implied for all downloads; no mention of transcription-time progress | Download progress via `indicatif` (stderr) unchanged; added explicit per-chunk `"transcribing chunk N of M"` stderr output for inputs requiring chunked encoding, none required for single-pass inputs                                                      | Matches spec.md FR-023 clarification, which the prior spec predates                                                                                                                  |
| Mel spectrogram parameters, tensor names, checksums, chunk threshold | Presented as settled implementation detail                                                      | Explicitly deferred to implementation-time verification                                                                                                                                                                                                     | Constitution Principle V — must be verified against real sources, not asserted at plan time                                                                                          |
| Mel extraction & tokenizer approach                                  | Hand-rolled `rustfft` DSP + `tokenizers` crate w/ `tokenizer.json`                              | Run the model's own bundled preprocessor `.onnx` graph via `ort`; decode via plain `vocab.txt` lookup, no crate                                                                                                                                             | Neither `tokenizer.json` nor a DSP-verification burden actually exists once the real model files were checked (§10) — `rustfft`, `ndarray`, `tokenizers` all removed from Cargo.toml |
| Preprocessor graph sourcing                                          | (not addressed in prior draft)                                                                  | `build.rs` downloads + checksum-verifies the `onnx-asr` PyPI wheel at build time, embeds via `include_bytes!` from `OUT_DIR` — not committed to git                                                                                                          | Repo's `forbid-binary` policy rules out vendoring the files directly (§10 addendum, 2026-07-11)                                                                                      |
