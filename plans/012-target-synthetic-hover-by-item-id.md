# Plan 012: Target synthetic hover events by validated item ID

> **Executor instructions**: Preserve coordinate-based hit testing for native
> pointer discovery; only dispatch targeting changes.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- src/ipc.rs src/render.rs src/daemon.rs`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `plans/003-propagate-daemon-response-errors.md`,
  `plans/006-reconcile-renderer-items-on-reload.md`
- **Category**: bug
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

`barrs item trigger <id> hover-enter` validates the item ID but creates mouse
coordinates `(0,0)`. Renderer hover handling ignores the ID and hit-tests those
coordinates, usually activating nothing or the wrong item.

## Current state

- `src/cli.rs:70-85` exposes hover enter/leave/update as trigger events.
- `src/app.rs:101-110` forwards the requested ID.
- `src/ipc.rs:26-35` assigns default mouse state.
- `src/render.rs:510-519` uses `item_at(x,y)` for enter/update instead of the
  validated `event.item_id`.
- Native pointer generation already hit-tests and attaches the correct ID at
  `src/render.rs:944-999`.

```rust
// src/render.rs:510-514
match event.event {
    EventKind::HoverEnter | EventKind::HoverUpdate => {
        self.active_hover_item = self.item_at(
            event.mouse.x as f64,
            event.mouse.y as f64,
        );
    }
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Hover tests | `cargo test --locked hover` | all hover-named tests pass |
| Full gate | `mise run check` | exit 0 |

## Scope

**In scope**: `src/render.rs`, `src/daemon.rs`, `src/ipc.rs` only if payload
provenance must be represented.

**Out of scope**: changing CLI syntax, moving real native hit testing into the
daemon, hover performance work from Plan 010.

## Git workflow

- Branch: `advisor/012-synthetic-hover-target`
- Commit: `fix(render): honor synthetic hover item targets`

## Steps

1. Make the already validated payload item ID authoritative for hover
   enter/update/leave dispatch. Coordinates remain contextual mouse data.
   **Verify**: item B activates even when coordinates fall on item A or nowhere.
2. Ensure native-generated payloads remain correct because their IDs came from
   AppKit hit testing.
   **Verify**: existing native hover tests pass.
3. Add CLI-style payload tests for enter, update, leave, and unknown IDs.
   Unknown IDs must keep the Plan 003 nonzero error path.
   **Verify**: full gate passes.

## Test plan

Build payloads through `EventPayload::from_trigger`. Cover requested ID with
coordinates over another item, `(0,0)`, enter/update/leave, and unknown ID.

## Done criteria

- [x] Synthetic hover targets the requested ID.
- [x] Native pointer hover behavior is unchanged.
- [x] Unknown IDs are rejected.
- [x] Full gate passes.
- [x] `git status --short` lists only scoped files and the plan status update.
- [x] `plans/README.md` row is updated.

## STOP conditions

- Renderer receives unvalidated external payloads through another path.
- Correctness requires adding a breaking field to `EventPayload`.

## Maintenance notes

Keep “which item” separate from mouse geometry. Coordinates should never
silently override an already resolved item identity.
