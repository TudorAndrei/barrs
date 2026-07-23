# Plan 007: Restore AppKit main-thread confinement

> **Executor instructions**: Treat unsafe `Send`/`Sync` removal as a soundness
> change. Do not replace it with another unchecked wrapper.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- src/main.rs src/daemon.rs src/render.rs`

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: `plans/001-enforce-rust-quality-gates.md`,
  `plans/006-reconcile-renderer-items-on-reload.md`
- **Category**: security
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

`Renderer` and `NativeHost` promise cross-thread safety, forcing
`AppKitHost`—which owns `NSApplication`, `NSWindow`, `NSView`, and
`NSPanel`—to use unsafe `Send` and `Sync`. The current binary stays on a
current-thread runtime, but the public type contract permits invalid callers.

## Current state

- `src/render.rs:65` declares `pub trait Renderer: Send + Sync`.
- `src/render.rs:560` repeats the bounds for `NativeHost`.
- `src/render.rs:623-643` owns retained AppKit objects.
- `src/render.rs:675-678` unconditionally implements unsafe `Send` and `Sync`.
- `src/render.rs:1197-1200` acknowledges the invariant with
  `MainThreadMarker`.
- `src/main.rs:5` uses a current-thread Tokio runtime.

```rust
// src/render.rs:675-678
unsafe impl Send for AppKitHost {}
unsafe impl Sync for AppKitHost {}
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Compile | `cargo check --all-targets --locked` | exit 0 |
| Renderer tests | `cargo test --locked` | all tests pass |
| Full gate | `mise run check` | exit 0 |

## Scope

**In scope**: `src/render.rs`, `src/daemon.rs`, `src/main.rs`; daemon tests that
spawn non-Send futures.

**Out of scope**: multi-thread runtime migration, renderer module extraction,
new platform renderers, replacing AppKit.

## Git workflow

- Branch: `advisor/007-appkit-main-thread`
- Commit: `refactor(render): confine appkit host to main thread`

## Steps

1. Confirm which generic/trait bounds actually require `Send + Sync`. Remove
   unnecessary bounds from `Renderer` and `NativeHost`, then delete both unsafe
   implementations.
   **Verify**: no `unsafe impl Send for AppKitHost` or `unsafe impl Sync for
   AppKitHost` remains.
2. Adapt current-thread tests using `LocalSet`, direct future polling, or
   non-spawned orchestration. Do not move AppKit ownership into a worker merely
   to satisfy tests.
   **Verify**: daemon and renderer tests compile and pass.
3. Ensure every AppKit entry point either accepts/obtains
   `MainThreadMarker` before access, including event pumping and Drop-sensitive
   callback cleanup. If Drop cannot prove main-thread execution, isolate
   registration in a main-thread-owned guard.
   **Verify**: strict Clippy, check, and tests pass.

## Test plan

Retain existing mock-host behavior. Add compile-time assertions where useful:
value-only scene/command types may remain `Send`; `AppKitHost` and the native
renderer must not be promised as cross-thread safe.

## Done criteria

- [ ] AppKit ownership has no unsafe `Send`/`Sync`.
- [ ] Stateful UI operations remain on the main thread.
- [ ] Current-thread daemon behavior remains unchanged.
- [ ] `mise run check` exits 0.
- [ ] `git status --short` lists only scoped files and the plan status update.
- [ ] `plans/README.md` row is updated.

## STOP conditions

- A dependency requires exposing `Renderer: Send + Sync` publicly.
- Callback removal cannot be made safe without a larger actor design.
- The fix would switch production to a multi-thread runtime.

## Maintenance notes

If background rendering is ever needed, send value-only `HostCommand`s through
a channel to a main-thread actor; never make retained AppKit objects movable.
