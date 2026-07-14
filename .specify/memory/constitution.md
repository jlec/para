<!--
Sync Impact Report
==================
Version change: 1.0.0 → 2.0.0
Rationale: Principle VI redefined in a backward-incompatible way (MAJOR per
this document's own versioning policy) — CoreML is no longer the default
`Device::Auto` execution path.

Modified principles:
- VI. Apple Silicon Is a First-Class Target — previously mandated CoreML as
  the default, non-opt-in execution provider on arm64 macOS. Redefined:
  Apple Silicon support itself remains mandatory and first-class, but the
  *default* execution provider is now whichever one is actually measured to
  perform best for the models this tool ships (currently CPU — CoreML
  measured zero speedup for the NVIDIA Parakeet family and required a
  stability workaround for a real ONNX Runtime crash with these models'
  external-data storage, research.md §15). CoreML remains available via the
  explicit `--device coreml` flag, unchanged from before.

Added sections: none

Removed sections: none

Templates requiring updates:
- ✅ specs/001-media-transcription/plan.md — Constitution Check's Principle VI
  row updated to match (already done alongside this amendment).
- ✅ .specify/templates/plan-template.md — Constitution Check section derives
  its gates from this file at runtime; no hardcoded principle names to sync.
- ✅ .specify/templates/spec-template.md — no constitution-specific references.
- ✅ .specify/templates/tasks-template.md — no constitution-specific references.

Follow-up TODOs: none
-->

# para Constitution

## Core Principles

### I. Single Binary, No Daemon

para MUST be invoked once per transcription and exit when the job completes.
There MUST be no background process, no server mode, and no persistent state
carried between invocations. Every run starts cold and ends clean.

**Rationale**: A daemon or server mode introduces lifecycle management,
inter-process state, and failure modes (stale processes, port conflicts,
stuck locks) that contradict a tool meant to be dropped into scripts and
pipelines without ceremony.

### II. Offline After Setup

Once a model is cached locally, para MUST run with no network access.
Network calls are permitted only to download a model on its first use, or
when the user explicitly passes `--refresh-model`. No other code path may
open a network connection.

**Rationale**: Local-first is a core value proposition. Silent network
calls during normal operation would break offline workflows, leak usage
metadata, and undermine trust that the tool does only what it says.

### III. Stdout Is Sacred

Stdout MUST carry only the transcript. All progress output, warnings, and
errors MUST go to stderr, with no exceptions.

**Rationale**: This is what makes para pipeable and scriptable — output can
be piped directly into other tools (`para audio.wav | tee transcript.txt`)
without post-processing to strip incidental noise.

### IV. Fail Loud, Fail Fast

There MUST be no silent fallbacks. If a checksum doesn't match, if ffmpeg is
missing, if the model can't load, para MUST error out immediately with a
clear message and a non-zero exit code. para MUST NEVER guess and proceed.

**Rationale**: Silent degradation (e.g., falling back to a different model
or skipping a verification step) produces transcripts a user cannot trust
and failures that surface far from their root cause.

### V. No Fabricated Data

Model checksums, download URLs, and tensor shapes MUST be verified against
real sources. They MUST NEVER be placeholdered, guessed, or invented.

**Rationale**: A wrong checksum or URL doesn't just fail once — it breaks
the tool for every user on first run, since these values gate the model
download and load path that Principle IV enforces.

### VI. Apple Silicon Is a First-Class Target

Apple Silicon MUST be a fully-supported target requiring no manual
configuration, extra flags, or separate build steps beyond what any other
supported target needs. CoreML acceleration MUST remain available as an
explicit, documented option (`--device coreml`) for anyone who wants to use
it, but MUST NOT be required by default: `--device auto`'s default execution
path is whichever provider is actually measured to perform best for the
models this tool ships, re-verified whenever that changes.

**Rationale**: Apple Silicon is a primary deployment target, not a secondary
optimization — but "first-class support" means the tool runs its best on
that hardware, not that a specific execution provider is mandatory
regardless of whether it helps. Measured directly against this project's
NVIDIA Parakeet models (research.md §15): CoreML produced no measurable
speedup over the CPU execution provider, and required a stability workaround
for a real ONNX Runtime crash with these models' external-data storage.
Defaulting to it anyway would add real complexity and risk for zero user
benefit, which itself would degrade the Apple Silicon experience this
principle exists to protect.

### VII. Minimal Runtime Dependencies

ffmpeg is the only external dependency the user must install themselves.
Every other runtime dependency MUST be statically linked or fetched
automatically at build time.

**Rationale**: Every additional required install is friction that works
against a single-binary tool meant to be easy to adopt and easy to trust.

### VIII. Composability Over Features

para is a Unix-style tool: one job, done well, that plays nicely in a
pipeline. Scope creep toward a server, a GUI, or a plugin system MUST be
resisted.

**Rationale**: Feature sprawl is how single-purpose CLI tools turn into
platforms that are harder to reason about, harder to keep offline, and
harder to keep fast.

## Engineering Standards

- Every error path MUST have a test.
- Library code MUST NOT panic; errors propagate via `Result` and are
  handled only at the top level (the binary's entry point).
- Well-maintained crates MUST be preferred over hand-rolled implementations
  for anything security- or correctness-sensitive, including checksums,
  tokenization, and DSP.

## Governance

This constitution supersedes all other project practices. Where a proposed
change conflicts with a principle here, the principle wins unless this
document is amended first.

**Amendment procedure**: Amendments are proposed as a diff to this file
alongside the reasoning for the change. On merge, the Sync Impact Report at
the top of this file MUST be updated, dependent templates in
`.specify/templates/` MUST be checked for consistency, and the version and
date fields below MUST be updated.

**Versioning policy**: This constitution follows semantic versioning:

- **MAJOR** — a principle is removed or redefined in a backward-incompatible
  way (e.g., relaxing Principle II's offline requirement).
- **MINOR** — a new principle or materially expanded section is added.
- **PATCH** — wording, clarification, or non-semantic fixes.

**Compliance review**: Any change to para's behavior MUST be checked against
these principles before merge. A violation MUST be either fixed or justified
in writing (e.g., in a plan's Complexity Tracking table) before it can land.

**Version**: 2.0.0 | **Ratified**: 2026-07-09 | **Last Amended**: 2026-07-14
