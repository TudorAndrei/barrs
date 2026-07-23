# Plan 015: Specify Lua-defined snapshot providers

> **Executor instructions**: This is a design spike, not feature implementation.
> Produce a decision document and executable-risk findings; do not add provider
> support to production code.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- src/config.rs src/daemon.rs src/plugin.rs`

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: MED
- **Depends on**: `plans/008-evaluate-lua-config-once.md`,
  `plans/014-document-lua-handler-contract.md`
- **Category**: direction
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

The retained Lua runtime, interval configuration, and snapshot-shaped plugin
API make custom status sources adjacent to the current architecture. A design
spike is required because slow/erroring Lua must not stall AppKit's 16 ms loop,
and provider state/reload semantics are not yet defined.

## Current state

- `src/config.rs:103-139` allows intervals but restricts plugins to seven enum
  variants.
- `src/config.rs:165-175` retains one Lua runtime with config.
- `src/plugin.rs:24-50` defines `Plugin::snapshot -> serde_json::Value`.
- `src/daemon.rs:599-619` renders a compiled plugin or static label.
- Lua is currently invoked only for handlers at `src/daemon.rs:622-645`.

```rust
// src/plugin.rs:24-27
pub trait Plugin: Send + Sync {
    fn snapshot(&self) -> Result<Value, BarrsError>;
}
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Baseline | `mise run check` | exit 0 |
| Document hygiene | `git diff --check` | no output |

## Scope

**In scope**: create `docs/spikes/lua-snapshot-providers.md`.

**Out of scope**: production Rust/Lua changes, new dependencies, committing a
prototype, promising backward compatibility before review.

## Git workflow

- Branch: `advisor/015-lua-provider-spike`
- Commit: `docs(design): specify lua snapshot providers`

## Steps

1. Document user stories and a minimal candidate item shape, including provider
   function name, interval interaction, and the existing snapshot fields
   (`text`, optional `icon`, structured values).
   **Verify**: every proposed field maps to an existing config/snapshot concept
   or is labeled new.
2. Compare two execution models: synchronous main-thread calls with strict
   budget versus worker execution with a value-only request/result channel.
   Assess `mlua` runtime ownership, state persistence, cancellation, and reload.
   **Verify**: the document selects one recommendation and rejects the other
   with evidence.
3. Define failure placeholder, timeout, overlapping refresh, serialization,
   and security/trust semantics. Split a future implementation into testable
   phases and list unresolved questions.
   **Verify**: document includes API sketch, state machine, tests, compatibility,
   rollout, and explicit go/no-go criteria.

## Test plan

The design document must specify future tests for config parsing, provider
return validation, timeout, overlap/coalescing, state persistence, reload,
placeholder output, and a slow provider that does not block UI work.

## Done criteria

- [x] Decision document exists and is self-contained.
- [x] Main-thread latency and Lua ownership are resolved or marked blockers.
- [x] Future implementation phases have exact source/test targets.
- [x] No production code changed.
- [x] `git status --short` lists only the spike document and plan status update.
- [x] `plans/README.md` row is updated.

## STOP conditions

- Plan 008 did not establish single-runtime ownership.
- The chosen model relies on moving AppKit or Lua across threads unsafely.

## Maintenance notes

If approved, convert the spike's implementation slices into new numbered plans;
do not silently expand this design-only plan.
