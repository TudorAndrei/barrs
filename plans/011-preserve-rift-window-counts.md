# Plan 011: Preserve exact Rift window counts across workspace changes

> **Executor instructions**: Confirm Rift payload shapes from existing fixtures
> or the local Rift contract. Stop rather than guessing missing fields.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- src/plugin.rs src/rift.rs`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: `plans/002-characterize-daemon-state-machines.md`
- **Category**: bug
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

When the active workspace changes, cached per-workspace state retains only a
boolean occupancy flag. The new current count is derived as 0 or 1, so
multi-window workspaces expose false `window_count` until a resync/event.

## Current state

- `src/rift.rs:54-59` stores `RiftWorkspace.has_windows: bool`.
- Initial parsing has exact `ParsedWorkspace.window_count` at
  `src/rift.rs:326-375`, but the public snapshot discards per-workspace counts.
- `src/rift.rs:487-492` assigns `usize::from(workspace.has_windows)`.
- `src/plugin.rs:281-289` exposes `window_count` in the Rift layout snapshot.
- Existing workspace-change test at `src/rift.rs:948-979` does not assert a
  multi-window target.

```rust
// src/rift.rs:487-492
snapshot.window_count = snapshot
    .workspaces
    .iter()
    .find(|workspace| workspace.is_current)
    .map(|workspace| usize::from(workspace.has_windows))
    .unwrap_or(snapshot.window_count);
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Rift tests | `cargo test --locked rift::tests` | all pass |
| Plugin tests | `cargo test --locked plugin::tests` | all pass |
| Full gate | `mise run check` | exit 0 |

## Scope

**In scope**: `src/rift.rs`, `src/plugin.rs` only if serialized plugin shape must
be extended compatibly.

**Out of scope**: changing workspace display names, Mach message transport,
Rift dirty-state cleanup.

## Git workflow

- Branch: `advisor/011-rift-window-counts`
- Commit: `fix(rift): preserve workspace window counts`

## Steps

1. Determine whether workspace snapshots/events reliably include exact counts.
   Preferred design: retain `window_count` per `RiftWorkspace`; otherwise return
   `RequiresResync` when a switch cannot derive the exact count.
   **Verify**: document the chosen source in a code comment/test fixture.
2. Update initial snapshot creation and event application so switching to a
   workspace with 2+ windows preserves the exact value. Keep `has_windows`
   consistent for workspace rendering.
   **Verify**: add target counts 0, 1, and 3 to event tests.
3. Verify serialization consumed by `RiftWorkspacesPlugin` and
   `RiftLayoutPlugin` remains backward-compatible.
   **Verify**: full gate passes.

## Test plan

Extend `workspace_changed_event_updates_current_workspace` with target counts
0, 1, and 3; cover absent-count resync and plugin JSON for occupancy/count.

## Done criteria

- [ ] No boolean-to-count conversion remains.
- [ ] Multi-window switch test reports the exact count.
- [ ] Occupancy rendering remains correct.
- [ ] Full gate passes.
- [ ] `git status --short` lists only scoped files and the plan status update.
- [ ] `plans/README.md` row is updated.

## STOP conditions

- Neither event payload nor resync can provide exact counts.
- Fixing the issue requires an undocumented Rift protocol assumption.

## Maintenance notes

Keep exact count and occupancy derived from one source to avoid future drift.
