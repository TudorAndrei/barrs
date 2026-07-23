# Plan 009: Clear Rift debounce state when no render is needed

> **Executor instructions**: Use the deterministic seams from Plan 002 and add
> regression tests before changing terminal branches.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- src/daemon.rs`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `plans/002-characterize-daemon-state-machines.md`
- **Category**: perf
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

After certain Rift events, `rift_dirty` remains true forever when there are no
Rift items or when the final signature equals the last rendered signature. The
16 ms event tick then repeats scans, clones, signatures, and locks indefinitely.

## Current state

- `src/daemon.rs:377-402` marks Rift state dirty and schedules a deadline.
- `src/daemon.rs:414-431` returns for no consumer without clearing state.
- `src/daemon.rs:433-439` returns for equal signature without clearing state.
- Only `src/daemon.rs:445-448` clears dirty/deadline after a rendered refresh.
- Subscription is created unconditionally at `src/daemon.rs:67`.

```rust
// src/daemon.rs:426-439
if item_ids.is_empty() {
    return Ok(());
}
if last_signature == Some(next_signature) {
    return Ok(());
}
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Focused tests | `cargo test --locked daemon::tests::rift` | matching tests pass |
| Full gate | `mise run check` | exit 0 |

## Scope

**In scope**: `src/daemon.rs`.

**Out of scope**: Rift protocol parsing, window counts, changing debounce
duration, display refresh performance.

## Git workflow

- Branch: `advisor/009-rift-dirty-state`
- Commit: `fix(rift): finish no-op debounce cycles`

## Steps

1. Add regression tests for a no-Rift-item configuration and an equal final
   signature. Each must assert `rift_dirty == false` and deadline cleared after
   the terminal cycle.
   **Verify**: tests fail on pre-fix code for the intended assertion only.
2. Clear dirty/deadline on both no-work terminal paths. Subscribe only when
   configured Rift items need events, and re-evaluate that decision on reload.
   **Verify**: a later changed event still refreshes and updates signature.
3. Run the full gate.
   **Verify**: `mise run check` exits 0.

## Test plan

Use Plan 002's pure transitions to test no consumers, equal signature, changed
signature, failed resync, later recovery, and subscription changes on reload.

## Done criteria

- [x] No-consumer and equal-signature cycles terminate.
- [x] A later real change is not suppressed.
- [x] Reload adds/removes subscriptions with Rift consumers.
- [x] Full gate passes.
- [x] `git status --short` lists only scoped files and the plan status update.
- [x] `plans/README.md` row is updated.

## STOP conditions

- Plan 002's seams were not implemented.
- Subscription ownership cannot change safely on reload.

## Maintenance notes

Every branch that decides “no render” must also define whether the debounce
cycle is complete; test that invariant when adding Rift events.
