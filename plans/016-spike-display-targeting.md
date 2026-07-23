# Plan 016: Specify deterministic display targeting

> **Executor instructions**: Design one target display, not simultaneous
> multi-bar support. Do not change AppKit code in this spike.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- src/config.rs src/render.rs README.md`

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: MED
- **Depends on**: `plans/007-confine-appkit-renderer-to-main-thread.md`
- **Category**: direction
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

The bar always follows `CGMainDisplayID`, despite substantial hotplug/rebuild
machinery and notch-specific configuration. A selector for main, built-in, or a
stable display identity would serve multi-display users without multiplying
window/state ownership.

## Current state

- `src/config.rs:36-50` has geometry/notch fields but no display selector.
- `src/render.rs:1470-1480` chooses `CGMainDisplayID`, then main/first fallback.
- `src/render.rs:782-868` already tracks display-reconfiguration callbacks and
  target signature changes.
- `ScreenSignature` at `src/render.rs:1506-1524` captures transient display ID
  and geometry, not a documented persistent identity.

```rust
// src/render.rs:1470-1480
let main_display = CGMainDisplayID();
for index in 0..screens.count() {
    let screen = screens.objectAtIndex(index);
    if screen.CGDirectDisplayID() == main_display {
        return Some(screen);
    }
}
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Baseline | `mise run check` | exit 0 |
| Document hygiene | `git diff --check` | no output |

## Scope

**In scope**: create `docs/spikes/display-targeting.md`.

**Out of scope**: production config/render changes, multiple concurrent bars,
private display APIs, migration implementation.

## Git workflow

- Branch: `advisor/016-display-targeting-spike`
- Commit: `docs(design): specify display targeting`

## Steps

1. Define candidate selectors (`main`, `builtin`, and stable identifier) and
   research which public AppKit/CoreGraphics properties are stable across
   reboot, reconnect, mirroring, and mode changes.
   **Verify**: the document cites the exact APIs/properties selected.
2. Specify deterministic fallback for missing target, mirrored displays,
   no-builtin desktops, target removal, and target return. Reuse the existing
   callback/rebuild lifecycle.
   **Verify**: a transition table covers all named cases.
3. Define backward-compatible Lua config shape, validation errors, test fixtures,
   manual multi-display matrix, and phased implementation boundaries.
   **Verify**: document contains go/no-go criteria and explicitly defers
   multi-bar support.

## Test plan

The design must include pure selector tests and a manual matrix for built-in,
external, mirrored, removed, returned, reordered, and no-match displays.

## Done criteria

- [ ] Selector identity and fallback behavior are unambiguous.
- [ ] Hotplug state transitions and test matrix are specified.
- [ ] Existing configs retain current main-display behavior.
- [ ] No production code changed.
- [ ] `git status --short` lists only the spike document and plan status update.
- [ ] `plans/README.md` row is updated.

## STOP conditions

- No public API provides a sufficiently stable identity.
- Plan 007 did not establish safe AppKit ownership.

## Maintenance notes

Review any future multi-bar proposal as a separate state-model change; it is not
an incremental extension of single-target selection.
