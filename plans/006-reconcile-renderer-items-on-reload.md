# Plan 006: Remove obsolete renderer items during reload

> **Executor instructions**: Treat reload as a complete next-state
> reconciliation, not a series of unrelated item updates.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- src/daemon.rs src/render.rs`

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: `plans/001-enforce-rust-quality-gates.md`,
  `plans/002-characterize-daemon-state-machines.md`
- **Category**: bug
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

Reload clears daemon snapshots but never removes snapshots retained by the
renderer. Removing or renaming an item therefore leaves the old native layer
visible and interactive until process restart.

## Current state

- `src/daemon.rs:222-241` replaces config and clears only
  `DaemonState::item_states`; it also initializes the renderer twice through
  `reload` and `refresh_all_items`.
- `src/render.rs:347-353` stores renderer-owned items separately.
- `src/render.rs:368-385` replaces only a matching ID.
- `src/render.rs:1749-1756` updates configuration/layout without clearing or
  reconciling items.
- `diff_host_scene` already emits `RemoveItemLayer` for absent next layers at
  `src/render.rs:2025-2036`; reuse that path.

```rust
// src/render.rs:1749-1755
fn initialize(&mut self, config: &Config) -> Result<(), BarrsError> {
    self.state.bar_height = BAR_HEIGHT;
    self.config = Some(config.clone());
    self.host.initialize(config)?;
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Renderer tests | `cargo test --locked render::tests` | all pass |
| Reload tests | `cargo test --locked daemon::tests::reload` | matching tests pass |
| Full gate | `mise run check` | exit 0 |

## Scope

**In scope**: `src/render.rs`, `src/daemon.rs`.

**Out of scope**: batching all refresh publications, renderer module splitting,
changing config syntax, redesigning hover presentation.

## Git workflow

- Branch: `advisor/006-reload-reconciliation`
- Commit: `fix(render): reconcile items during reload`

## Steps

1. Define explicit renderer reconfiguration semantics: initialization with a
   complete config must reconcile retained item IDs, clear hover state for
   removed IDs, and publish native layer removals. Avoid a blanket reset if it
   causes unnecessary window destruction.
   **Verify**: a renderer test initializes items A/B, reconfigures with B/C, and
   observes A removed, B retained/updated, C added.
2. Refactor `reload`/`refresh_all_items` so renderer initialization happens
   exactly once per reload and the complete next snapshot set is applied
   transactionally. Do not leave the state half-mutated if initialization or
   snapshot creation fails.
   **Verify**: a failing renderer test leaves the prior config/items usable.
3. Add a daemon reload test that rewrites the Lua config with a removed/renamed
   item and asserts both dump-state and renderer scene contain only next IDs.
   **Verify**: focused tests and `mise run check` pass.

## Test plan

Model renderer assertions after `render::tests::host_scene_diff_emits_remove_and_hover_hide`
and daemon setup after `daemon_survives_reload_with_broken_config`. Cover
remove, rename, retained update, removed hover, and failed renderer initialize.

## Done criteria

- [ ] Removed and renamed items disappear after reload.
- [ ] Removed hovered items dismiss their hover panel.
- [ ] Renderer initialization occurs once per reload.
- [ ] Failed reload retains the previous working state.
- [ ] Full quality gate passes.
- [ ] `git status --short` lists only scoped files and the plan status update.
- [ ] `plans/README.md` row is updated.

## STOP conditions

- Reconciliation requires destroying the AppKit window.
- Transactionality requires making AppKit objects cloneable.
- Plan 007 has already changed renderer ownership without updating this plan.

## Maintenance notes

Any future batch-render API must preserve the complete-next-state semantics
established here.
