# Phase 0 Research: Transcription Progress Indicators

All findings below were verified directly against the real, pinned crate source in this project's
local cargo registry cache (`~/.cargo/registry/src/.../indicatif-0.18.6`,
`~/.cargo/registry/src/.../console-0.16.4`) — not assumed from training-data familiarity with
older or different indicatif versions (Constitution Principle V).

## 1. indicatif's default non-terminal behavior does NOT satisfy FR-007

**Question**: Can indicatif's own terminal-detection be relied on to produce spec.md FR-007's
required "plain, newline-terminated progress milestones" when stderr isn't an interactive
terminal?

**Finding**: No. `ProgressDrawTarget::term()` (the code path behind the default
`ProgressDrawTarget::stderr()`, confirmed as indicatif's actual default draw target for a new
`ProgressBar`) contains:

```rust
pub fn term(term: Term, refresh_rate: u8) -> Self {
    if !term.is_term() || is_dumb() {
        return Self::hidden();
    }
    ...
}
```

(`indicatif-0.18.6/src/draw_target.rs`). When stderr is not a real terminal, or `TERM` is
`dumb`/unset (see finding 2), indicatif doesn't degrade to plain text — it goes fully **hidden**,
producing zero output. That's the right behavior for indicatif's own purpose (never leak escape
codes into a redirected file) but it's silence, not the plain milestone lines FR-007 requires.

**Decision**: Do not rely on indicatif's automatic hiding. Implement an explicit
`is_interactive() -> bool` check (finding 3) at the point progress reporting begins, and branch:

- Interactive: build a real, animated `indicatif::ProgressBar` targeting stderr.
- Non-interactive: skip indicatif entirely; emit plain `eprintln!` milestone lines directly — the
  same mechanism `src/inference/engine.rs`'s `encode_chunked`/`encode_chunked_ctc` already use
  unconditionally today for `"transcribing chunk N of M"` (spec 001, FR-023). That existing line
  already _is_ a valid non-interactive milestone and needs no new mechanism, only extending to the
  other phases (model loading, stdin reading) this feature adds.

## 2. `console::is_dumb()`'s Unix default is easy to get wrong by hand

**Question**: What exactly counts as a "dumb" terminal, precisely enough to reimplement or reuse
correctly?

**Finding**: `console-0.16.4/src/term.rs`:

```rust
pub fn is_dumb() -> bool {
    #[cfg(windows)]
    let default = false;
    #[cfg(not(windows))]
    let default = true;

    match env::var("TERM") {
        Ok(term) => term == "dumb",
        Err(_) => default,
    }
}
```

On Unix, an **unset** `TERM` counts as dumb by default (`default = true`), not just an explicit
`TERM=dumb`. This is a real, easy-to-miss detail — a hand-rolled reimplementation using only
`std::io::IsTerminal` would miss the `TERM`-unset case entirely and could render an animated bar
in an environment indicatif itself would have suppressed.

**Decision**: Reuse `console`'s own `Term::stderr().is_term()` and `console::is_dumb()` directly
rather than reimplementing terminal/dumb detection with `std::io::IsTerminal` alone. `console` is
already present in `Cargo.lock` as indicatif's transitive dependency at this exact version —
promoting it to a direct dependency adds zero new entries to the dependency tree (Constitution
Principle VII: no new supply-chain surface).

## 3. The canonical "is interactive" check for this feature

**Decision**: A single shared predicate, used everywhere this feature needs to decide
animated-bar vs. plain-milestone behavior:

```rust
fn is_interactive() -> bool {
    console::Term::stderr().is_term() && !console::is_dumb()
}
```

This also directly satisfies the spec's `NO_COLOR`/`TERM=dumb` edge case: a plain-milestone path
never emits ANSI/color codes in the first place, so `NO_COLOR` needs no separate handling — it's
moot once the interactive/non-interactive branch is decided this way.

## 4. Adaptive ETA is a real indicatif feature, not something to hand-roll

**Question**: Does indicatif compute ETA from actually-observed progress (per spec.md's
Clarifications — FR-005 explicitly rules out a fixed per-model assumption), or would a fixed-rate
estimate need to be written from scratch?

**Finding**: `ProgressBar`/`ProgressState::eta()` (`indicatif-0.18.6/src/state.rs`) computes ETA
from `self.est.steps_per_second(Instant::now())` — a real, measured, exponentially-smoothed rate
estimator based on actual position updates over elapsed wall-clock time, not a fixed constant. The
`{eta}`/`{eta_precise}` template placeholders (`src/style.rs`) render this directly. No custom ETA
math is needed.

**Decision**: Use indicatif's built-in `eta()`/`{eta}` support as-is for FR-005. Feeding it
accurate, frequent position updates (per finding 5) is the only real requirement on the
implementation side.

## 5. Progress unit: audio-milliseconds, not audio-seconds, as an integer

**Question**: `ProgressBar`'s position/length are `u64`. Spec.md's FR-002 measures progress as a
fraction of audio-seconds processed — audio duration is an `f64` in the existing codebase
(`duration_secs`, per `001-media-transcription`'s data-model.md). What integer unit preserves
useful precision without any new dependency?

**Decision**: Scale to milliseconds (`(duration_secs * 1000.0).round() as u64`) for both
`set_length` and each `inc`/`set_position` call. Millisecond granularity is far finer than the
chunk-level update granularity this feature actually uses (spec.md's resolved Assumption: updates
occur per processing chunk, up to every 300s), so no precision is lost in practice; it simply
avoids the awkwardness of forcing seconds-only integer resolution on a value that's naturally
fractional.

## 6. Where existing code already does the work this feature extends

Verified directly against the current source (`src/inference/engine.rs`, `src/audio.rs`):

- `encode_chunked`/`encode_chunked_ctc` already know total chunk count and print a bare
  `eprintln!("transcribing chunk {i} of {total}")` per chunk, unconditionally (spec 001, FR-023).
  This feature extends that call site with the progress-phase abstraction — interactive runs get a
  real bar, non-interactive runs keep an equivalent plain line.
- `InputMedia`'s `duration_secs` (from `audio.rs`'s ffmpeg probe) is already computed before
  transcription begins for both `-i` and staged-stdin input alike — confirming the UX consult's
  central finding (spec.md's Context section) that both input methods converge on identical,
  known-duration progress-reporting behavior once staging completes. No new duration-probing logic
  is needed; this feature only needs to _consume_ the value that already exists.
- **Correction (found during implementation)**: this section originally claimed stdin staging
  (`audio.rs`'s temp-file writer) "reads in a loop already" — checked against the real code, it was
  actually a single `read_to_end` call with no loop at all. Rewritten as an explicit 64KB-chunked
  read loop so the byte-counter spinner (FR-004) has an incremental hook to update from; this is a
  new (small) read path, not a pre-existing one as assumed here.
- Model-session construction (`engine.rs`'s `build_session_from_file`/`build_session_from_memory`)
  is already a single, identifiable set of call sites, all reachable from `main.rs`'s `run()` before
  any chunk of audio is processed — the natural place to start/stop the model-loading spinner
  (FR-003).

## Summary of decisions

| Question                 | Decision                                                                                                                        |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| Non-interactive fallback | Explicit manual branch, not indicatif's auto-hide; plain `eprintln!`, extending the existing FR-023 pattern                     |
| Terminal/dumb detection  | Reuse `console::Term::stderr().is_term()` + `console::is_dumb()` directly (already a transitive dependency, promoted to direct) |
| ETA computation          | indicatif's built-in `eta()`/`{eta}` — real measured-rate estimator, no hand-rolled math                                        |
| Progress unit            | Audio-milliseconds as `u64`, converted from the existing `f64` `duration_secs`                                                  |
| New dependency footprint | `console` promoted from transitive to direct at its already-pinned version — zero new dependency-tree entries                   |
