# Plan 002: Characterize daemon refresh and Rift state transitions

> **Executor instructions**: Complete the test-seam work without fixing the
> known Rift dirty-state bug reserved for Plan 009. Update `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- Cargo.toml src/daemon.rs`
> Stop if the scheduler or Rift debounce code no longer matches this plan.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: `plans/001-enforce-rust-quality-gates.md`
- **Category**: tests
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

`Daemon` contains two coupled state machines: background refresh/epoch handling
and Rift event/debounce handling. Current coverage uses real sleeps and misses
stale epochs, coalescing, partial failure, and terminal no-work paths, making
later fixes harder to verify safely.

## Current state

- `src/daemon.rs:272-355` creates one `spawn_blocking` refresh, rejects stale
  epochs, applies snapshots, then advances deadlines.
- `src/daemon.rs:368-449` drains Rift events, optionally resyncs, debounces, and
  suppresses equal signatures.
- `src/daemon.rs:1226-1255` sleeps 1.25 seconds and asserts only that at least
  two renders occurred.
- Tests live in each source module under `#[cfg(test)]`; keep that convention.

```rust
// src/daemon.rs:272-275
async fn refresh_due_items(&mut self) -> Result<(), BarrsError> {
    if self.pending_refresh.is_some() {
        return Ok(());
    }
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Focused tests | `cargo test --locked daemon::tests` | all daemon tests pass |
| Full gate | `mise run check` | exit 0 |

## Scope

**In scope**: `src/daemon.rs`; `Cargo.toml` only if Tokio's `test-util` feature
is genuinely required.

**Out of scope**: fixing dirty-state cleanup, changing production intervals,
changing renderer behavior, live Rift/Mach integration tests.

## Git workflow

- Branch: `advisor/002-daemon-state-tests`
- Commit: `test(daemon): characterize refresh and rift state transitions`

## Steps

1. Extract the smallest pure transition helpers or injectable time/backend
   seams needed to test scheduler decisions without wall-clock sleeps. Do not
   alter public APIs.
   **Verify**: existing daemon tests pass unchanged.
2. Add table-driven tests for stale refresh epochs, a refresh already pending,
   partial snapshot failure, deadline advancement, changed Rift signatures,
   resync requests, and debounce-before-deadline behavior.
   **Verify**: focused tests pass without sleeps longer than 50 ms.
3. Add ignored or explicitly failing documentation only for the known
   no-consumer/equal-signature dirty-state cases; do not commit a red test.
   Record those cases in comments for Plan 009's regression tests.
   **Verify**: `mise run check` exits 0.

## Test plan

Model new tests after the private-state tests in `src/daemon.rs:1184+`. Assert
state fields and renderer counts exactly, not “at least” counts.

## Done criteria

- [x] Scheduler transition tests are deterministic.
- [x] No test sleeps for the current 1.25-second scheduler window.
- [x] Known bug behavior is not enshrined as expected behavior.
- [x] `mise run check` exits 0.
- [x] Only scoped files changed.
- [x] `plans/README.md` row is updated.

## STOP conditions

- Determinism requires a public API break.
- A test can only pass by fixing Plan 009 early.
- Tokio feature changes materially enlarge the release binary.

## Maintenance notes

Use these seams for Plans 009 and 011. Do not replace focused transition tests
with broad timing-based integration tests later.
