<!--
Sync Impact Report
==================
Version change: (none, template) → 1.0.0
Rationale: Initial ratification — first concrete fill of the constitution template.

Modified principles: N/A (initial adoption)

Added sections:
- Core Principles I–VIII (Single Binary No Daemon, Offline After Setup,
  Stdout Is Sacred, Fail Loud Fail Fast, No Fabricated Data,
  Apple Silicon First-Class Target, Minimal Runtime Dependencies,
  Composability Over Features)
- Engineering Standards (replaces [SECTION_2_NAME])
- Governance

Removed sections:
- [SECTION_3_NAME] / [SECTION_3_CONTENT] — no additional workflow/review
  content was supplied; omitted rather than left as an unfilled placeholder.
  TODO(SECTION_3): add a Development Workflow section if/when review or
  release process rules are defined.

Templates requiring updates:
- ✅ .specify/templates/plan-template.md — Constitution Check section derives
  its gates from this file at runtime; no hardcoded principle names to sync.
- ✅ .specify/templates/spec-template.md — no constitution-specific references.
- ✅ .specify/templates/tasks-template.md — no constitution-specific references.
- ✅ No .specify/templates/commands/*.md directory present.
- ✅ No README.md or docs/quickstart.md present in repo root.

Follow-up TODOs:
- TODO(RATIFICATION_DATE): original adoption date predating this fill is
  unknown; set to the date of this initial ratification.
- TODO(SECTION_3): consider adding a Development Workflow / Review Process
  section once those practices are decided.
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

CoreML acceleration on arm64 macOS MUST work out of the box. It MUST NOT
require manual configuration, extra flags, or separate build steps beyond
what any other supported target needs.

**Rationale**: Apple Silicon is a primary deployment target, not a
secondary optimization; treating it as an afterthought would degrade the
experience for most users of a local-first, on-device tool.

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

**Version**: 1.0.0 | **Ratified**: 2026-07-09 | **Last Amended**: 2026-07-09
