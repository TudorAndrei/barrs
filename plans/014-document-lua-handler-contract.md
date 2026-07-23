# Plan 014: Document the complete Lua handler contract

> **Executor instructions**: Documentation must describe behavior proven by
> source/tests; do not invent cancellation or return-value semantics.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- README.md barrs.lua`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `plans/003-propagate-daemon-response-errors.md`,
  `plans/012-target-synthetic-hover-by-item-id.md`
- **Category**: docs
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

Handlers are advertised without their six names, argument shape, or return
semantics. The sample `open_clock(ctx)` merely returns `true`, but the daemon
calls handlers for `()` and ignores that value.

## Current state

- `README.md:102-111` only says “click and hover handlers”.
- `src/config.rs:149-163` supports click, right-click, scroll, hover-enter,
  hover-leave, and hover-update.
- `src/ipc.rs:17-85` defines context fields: item ID, event, timestamp, mouse,
  and modifiers.
- `src/daemon.rs:622-645` serializes the payload and calls the function for
  `()`.
- `barrs.lua:1-3` names `open_clock` but performs no action.

```rust
// src/daemon.rs:639-645
let func: mlua::Function = globals
    .get(handler_name.as_str())
    .map_err(|_| BarrsError::InvalidConfig(format!("missing handler {handler_name}")))?;
let ctx = lua.to_value(payload)?;
func.call::<()>(ctx)?;
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Config tests | `cargo test --locked lua` | all Lua-named tests pass |
| Full gate | `mise run check` | exit 0 |
| Diff hygiene | `git diff --check` | no output |

## Scope

**In scope**: `README.md`, `barrs.lua`; existing config/daemon tests only if
needed to make the example executable.

**Out of scope**: adding new handler types, shell helpers, Lua providers, or
changing return semantics.

## Git workflow

- Branch: `advisor/014-lua-handler-docs`
- Commit: `docs(config): define lua handler contract`

## Steps

1. Add a compact handler reference listing all slots and their matching event
   names. Document the complete context table, including nullable
   button/scroll fields and modifier booleans.
   **Verify**: every `ItemHandlers` field appears in the README.
2. State explicitly that return values are ignored and runtime errors are
   returned/logged as daemon request failures.
   **Verify**: wording matches `func.call::<()>(ctx)`.
3. Replace or rename the misleading `open_clock` sample with a deterministic,
   meaningful example that exercises context without launching external apps.
   Ensure the bundled config remains valid.
   **Verify**: config and Lua daemon tests pass; full gate passes.

## Test plan

Load the bundled `barrs.lua` through `load_config`; invoke its sample handler
with a serialized click context and assert it completes without external side
effects.

## Done criteria

- [ ] All handler slots and context fields are documented.
- [ ] Return/error semantics are accurate.
- [ ] Bundled sample behavior matches its name.
- [ ] Full quality gate passes.
- [ ] `git status --short` lists only scoped files and the plan status update.
- [ ] `plans/README.md` row is updated.

## STOP conditions

- A meaningful sample would require platform-specific side effects.
- Plans 003/012 changed the contract without updating their plan/index.

## Maintenance notes

Treat the handler reference as public API documentation; update it alongside
future `EventPayload` or `ItemHandlers` changes.
