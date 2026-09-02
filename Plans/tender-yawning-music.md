# Fix broken CI gate + cut unnecessary macOS Action minutes

## Context

The "getting expensive" runs aren't a macOS-billing problem in the first place — they're a
correctness bug. `para` links a Swift/CoreML bridge (`build.rs` → `swift build` → Apple
frameworks) and only ever compiles on macOS. Two pre-commit hooks that trigger a real compile —
`cargo-check` and `clippy` — run inside the shared `linting` job on `ubuntu-latest`
(`jlec/github-actions/reusable-linting.yml`), so they fail on **every single run**, Dependabot or
not, 100% of the time (confirmed in job `98931116984`: `swift build` dies on
`mach/mach.h` not found on Linux). Because `ci.yaml`'s `rust-build-test`, `dependabot`, and
`ci-generic` jobs all `needs: [linting]`, that one guaranteed-red Ubuntu job has been silently
skipping the rest of the pipeline on every push and PR — the pipeline hasn't produced a real
signal in a while, only noise and burned minutes.

The reusable `linting` workflow lives in the separate `jlec/github-actions` repo and only exposes
a fixed `SKIP:` list (ansible/go/terraform hook IDs) — there's no input to add repo-specific
skips, and editing that shared workflow would affect every repo that calls it. The lever that's
actually ours is `.pre-commit-config.yaml`'s per-hook `stages:` key, which already gates
`commitizen` (`commit-msg`) and `ggshield`/`cruft` (`pre-push`) the same way. Marking a hook
`stages: ["manual"]` means it's skipped by every default invocation — `prek run --all-files` in
CI and a developer's local `prek run` alike — with no CI-vs-local branching logic. That's the
"pre-commit skip for all runs" the request asks for, as opposed to an env-var trick that behaves
differently in CI than locally.

Separately, the actually-billable macOS job (`rust-macos-ci.yaml`, `macos-14`) runs unconditionally
on every `pull_request` and every push to `main` — including Dependabot PRs that only bump a
`.pre-commit-config.yaml` hook version or a `pyproject.toml`/`uv.lock` pin, which can't possibly
change the compiled binary. Recent history (runs `33195397629`, `33195388459`, `33195382105`,
`33148958839`, `33148958497`, ...) shows every one of these trivial-bump PRs still paying for a
full macOS build+test. Dependabot's per-ecosystem grouping (`patterns: ["*"]`) is already as
collapsed as it gets — Dependabot has no cross-ecosystem grouping — so the real lever for
"collapsing the different Dependabot runs" is making the expensive job path-aware, not the
Dependabot config.

## Changes

### 1. Stop the guaranteed-fail compile on Ubuntu — `.pre-commit-config.yaml:152-155`

`fmt` passes today (it's a pure formatter, doesn't invoke `build.rs`) — leave it alone. Only
`cargo-check` and `clippy` actually compile the crate. Move just those two off the default
`pre-commit` stage:

```yaml
  - repo: https://github.com/doublify/pre-commit-rust
    rev: v1.0
    hooks:
      - id: fmt
      - id: cargo-check
        stages: ["manual"]
      - id: clippy
        args: ["--", "-D", "warnings"]
        stages: ["manual"]
```

This fixes the Ubuntu `linting` job (it stops attempting an Apple-only compile) and unblocks
`dependabot`, `ci-generic`, and `ci-release`, all of which `needs: [linting]`. Coverage isn't
lost: `rust-macos-ci.yaml` already runs `cargo fmt --check` / `cargo clippy --all-targets` /
`cargo test` directly (not through prek) on the one platform this crate builds on.

### 2. Remove the now-unblocked but still-impossible Ubuntu Rust job — `.github/workflows/ci.yaml`

Once `linting` goes green, `rust-build-test` (`runs-on: ubuntu-latest`, `needs: [linting]`) will
actually execute for the first time — and fail identically, since it runs `cargo clippy` /
`cargo test` directly and hits the same `swift build` wall. It's pure duplication of
`rust-macos-ci.yaml`, which already covers build/fmt/clippy/test on the only viable runner. Delete
the `rust-build-test` job (`ci.yaml:35-53`) and drop it from `ci-generic`'s `needs:`
(`ci.yaml:77-79`, `[rust-build-test, dependabot, linting]` → `[dependabot, linting]`). No branch
protection references it (`main` is unprotected — confirmed via `gh api
repos/jlec/para/branches/main/protection` → 404), so nothing else depends on this job's name.

### 3. Path-filter the billable macOS job — `.github/workflows/rust-macos-ci.yaml:9-16`

Add `paths:` to both `pull_request:` and `push:` so `macos-14` only spins up when a change could
actually touch the compiled output:

```yaml
on:
  workflow_dispatch:

  pull_request:
    paths:
      - "src/**"
      - "swift/**"
      - "tests/**"
      - "Cargo.toml"
      - "Cargo.lock"
      - "build.rs"
      - "rust-toolchain.toml"
      - ".github/workflows/rust-macos-ci.yaml"

  push:
    branches:
      - main
    tags-ignore:
      - "**"
    paths:
      - "src/**"
      - "swift/**"
      - "tests/**"
      - "Cargo.toml"
      - "Cargo.lock"
      - "build.rs"
      - "rust-toolchain.toml"
      - ".github/workflows/rust-macos-ci.yaml"
```

This is what actually cuts the macOS bill: Dependabot's `pre-commit`, `github-actions`, `pip`, and
`uv` ecosystem PRs (the majority of the 6 weekly groups) stop paying for a macOS runner entirely;
`cargo` and `rust-toolchain` bumps still trigger it, correctly, since those can change what
compiles.

## Verification

- `prek run --all-files --hook-stage manual cargo-check clippy` locally still runs and passes on
  macOS, confirming the hooks aren't broken, just gated.
- `prek run --all-files` locally (default stage) no longer invokes `cargo-check`/`clippy` —
  matches what the Ubuntu `linting` job will now do.
- Push a trivial doc-only or `.pre-commit-config.yaml`-only commit to a branch, open a PR: confirm
  `linting` job goes green and `rust-macos-ci.yaml` does **not** run.
  `gh run list --workflow=rust-macos-ci.yaml -b <branch>` should show nothing for that push.
- Push a `src/**` change on a branch: confirm `rust-macos-ci.yaml` still runs and `ci.yaml`'s
  `linting` → `ci-generic` chain completes without a `rust-build-test` job in the run's job list
  (`gh run view <id> --json jobs`).
- `gh run list --limit 10 --json conclusion` after the next few Dependabot PRs land: failures
  should drop to zero for `linting`, and `gh run list --workflow=rust-macos-ci.yaml --limit 10`
  should show it skipped for non-code bumps.
