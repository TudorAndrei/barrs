# Plan 001: Enforce a reproducible Rust quality baseline

> **Executor instructions**: Follow every step and verification gate. Update this
> plan's row in `plans/README.md` when finished. Do not push or open a PR unless
> the operator asks.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- .github/workflows/ci.yml mise.toml \
> src/daemon.rs src/plugin.rs src/process.rs src/render.rs src/rift.rs`
> If these files have changed, compare the live diagnostics with the current
> state below. Stop on a material mismatch.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: dx
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

CI currently accepts code that fails standard Rust formatting and lint checks.
Pinning the toolchain and exposing one local verification task gives every later
plan a stable, machine-checkable baseline.

## Current state

- `.github/workflows/ci.yml:21-25` runs only `cargo check --all-targets` and
  `cargo test`.
- `mise.toml:2` selects moving `rust = "latest"`.
- The verified local baseline is `rustc 1.97.1` / `cargo 1.97.1` on
  `aarch64-apple-darwin`.
- `cargo fmt --all -- --check` reports drift in `src/process.rs`,
  `src/render.rs`, and `src/rift.rs`.
- `cargo clippy --all-targets -- -D warnings` reports five diagnostics in
  `src/daemon.rs`, `src/plugin.rs`, `src/render.rs`, and `src/rift.rs`.
- Existing CI naming is imperative and simple; match that style.

```yaml
# .github/workflows/ci.yml:21-25
- name: Check all targets
  run: cargo check --all-targets
- name: Run tests
  run: cargo test
```

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Format | `cargo fmt --all -- --check` | exit 0, no diff |
| Lint | `cargo clippy --all-targets -- -D warnings` | exit 0 |
| Check | `cargo check --all-targets --locked` | exit 0 |
| Test | `cargo test --locked` | 89 or more tests pass |

## Scope

**In scope**: `.github/workflows/ci.yml`, `mise.toml`, a new
`rust-toolchain.toml`, and only the Rust files changed by the initial
formatting/Clippy cleanup listed above.

**Out of scope**: dependency upgrades, release automation, behavior changes,
new lints unrelated to the five current diagnostics.

## Git workflow

- Branch: `advisor/001-rust-quality-gates`
- Commit: `chore(ci): enforce rust quality gates`

## Steps

1. Pin Rust `1.97.1` in `rust-toolchain.toml`, including `rustfmt` and `clippy`.
   Keep `mise.toml` consistent with that exact pin and add a `check` task that
   runs format, Clippy, check, and tests in that order.
   **Verify**: `mise run check` reaches the existing formatting failure rather
   than failing to resolve tools.
2. Apply `cargo fmt --all`, then resolve exactly the five current Clippy
   diagnostics without changing behavior.
   **Verify**: format and Clippy commands above both exit 0.
3. Add explicit formatting and Clippy steps to CI and use `--locked` for check
   and test.
   **Verify**: `mise run check` exits 0 and `git diff --check` prints nothing.

## Test plan

No new behavioral test is required. The regression is the CI/task definition:
all four commands must be present and pass locally.

## Done criteria

- [x] The toolchain is pinned in both supported entry points.
- [x] `mise run check` exits 0.
- [x] CI runs format, Clippy, locked check, and locked tests.
- [x] No files outside Scope changed.
- [x] `plans/README.md` row is updated.

## STOP conditions

- Pinning the current compiler breaks a supported target.
- A Clippy fix requires changing public behavior or unsafe invariants.
- More than the five recorded Clippy diagnostics appear before source edits.

## Maintenance notes

Update the Rust pin deliberately, with `mise run check`, rather than returning
to a moving `latest` toolchain. Review future lint-policy changes separately.
