# Plan 010: Avoid publishing unchanged hover scenes

> **Executor instructions**: Continue delivering configured `hover_update` Lua
> callbacks unless the product contract is changed explicitly.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- src/daemon.rs src/render.rs`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `plans/006-reconcile-renderer-items-on-reload.md`
- **Category**: perf
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

A stationary pointer emits `HoverUpdate` around 62 times per second. Each event
publishes a full scene and calls AppKit `display`/`updateWindows` even when the
active hover target and presentation have not changed.

## Current state

- `src/render.rs:944-968` emits `HoverUpdate` whenever pointer target is
  unchanged.
- `src/daemon.rs:357-364` dispatches every renderer event.
- `src/render.rs:510-521` mutates hover target without reporting whether it
  changed.
- `src/render.rs:1774-1777` always calls `publish_scene`.
- `src/render.rs:1023-1039` updates AppKit even for an empty command diff.

```rust
// src/render.rs:960-968
(Some(previous), Some(next)) if previous == next => {
    vec![synthetic_event_payload(next, EventKind::HoverUpdate, location.x, location.y)]
}
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Renderer tests | `cargo test --locked render::tests` | all pass |
| Daemon hover tests | `cargo test --locked hover` | all hover-named tests pass |
| Full gate | `mise run check` | exit 0 |

## Scope

**In scope**: `src/render.rs`, `src/daemon.rs`.

**Out of scope**: changing the 16 ms event interval, removing hover-update
handlers, redesigning hover UI.

## Git workflow

- Branch: `advisor/010-hover-publication`
- Commit: `perf(render): skip unchanged hover publications`

## Steps

1. Make `NativeSurfaceState::handle_event` report whether presentation state
   changed. Publish only when target/panel content changes.
   **Verify**: repeated same-target updates produce one host presentation.
2. In `AppKitHost::apply_commands`, skip `window.display()` and
   `app.updateWindows()` when the diff is empty.
   **Verify**: mock command-count tests cover empty and nonempty diffs.
3. Assert Lua `hover_update` handlers still run at the intended polling cadence
   while redundant renderer publication is gated.
   **Verify**: focused tests and `mise run check` pass.

## Test plan

Extend MockNativeHost counting tests for repeated same-target update, target
change, leave, changed panel content, empty host diff, and Lua hover-update
delivery.

## Done criteria

- [ ] Same-target updates do not republish a scene.
- [ ] Empty command lists do not force AppKit updates.
- [ ] Hover enter/leave and Lua update handlers still work.
- [ ] Full gate passes.
- [ ] `git status --short` lists only scoped files and the plan status update.
- [ ] `plans/README.md` row is updated.

## STOP conditions

- Gating renderer updates suppresses Lua handler delivery.
- Panel contents can change without snapshot/presentation state changing.

## Maintenance notes

If hover handlers later mutate snapshots, introduce an explicit invalidation
signal rather than returning to unconditional 62 Hz publication.
