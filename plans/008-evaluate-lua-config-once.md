# Plan 008: Evaluate Lua configuration once per load

> **Executor instructions**: Preserve stateful handler behavior and reload reset
> semantics covered by existing daemon tests.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- src/app.rs src/config.rs src/daemon.rs`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `plans/001-enforce-rust-quality-gates.md`
- **Category**: bug
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

The app evaluates Lua to obtain `Config`, then `Daemon::new` evaluates it again
to obtain `Lua`. Side effects run twice, and nondeterministic config output can
pair one config with a different runtime.

## Current state

- `src/app.rs:22` and `src/app.rs:43` call `config::load_config`.
- `src/daemon.rs:57-65` immediately calls `load_config_with_runtime` again but
  stores the previously supplied `Config`.
- `src/config.rs:165-175` already returns the correct atomic pair `(Config,
  Lua)`.
- Existing tests assert Lua state persists between events and resets on reload
  (`src/daemon.rs:1037+`).

```rust
// src/app.rs:43 and src/daemon.rs:57-58
let config = config::load_config(&config_path)?;
pub fn new(config_path: PathBuf, config: Config, renderer: R) -> Result<Self, BarrsError> {
    let (_, lua) = load_config_with_runtime(&config_path)?;
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Config tests | `cargo test --locked lua` | all Lua-named tests pass |
| Full gate | `mise run check` | exit 0 |

## Scope

**In scope**: `src/app.rs`, `src/config.rs`, `src/daemon.rs`.

**Out of scope**: sandboxing Lua, changing config schema, Lua snapshot providers
(Plan 015), handler documentation.

## Git workflow

- Branch: `advisor/008-single-config-evaluation`
- Commit: `fix(config): evaluate lua once per load`

## Steps

1. Make one layer own loading. Preferred shape: app/start/run obtains
   `(Config, Lua)` once and passes both to a constructor, while reload continues
   using the same pair-returning function. Avoid a constructor that silently
   rereads disk.
   **Verify**: `rg 'load_config_with_runtime' src` shows one call per lifecycle
   load path.
2. Add a config fixture that increments an observable counter or returns a
   different value per evaluation; assert startup evaluates it exactly once and
   config/handler runtime agree.
   **Verify**: focused tests pass.
3. Retain handler persistence and reload-reset behavior.
   **Verify**: `mise run check` exits 0.

## Test plan

Use a temporary config with a deterministic evaluation counter. Cover startup,
reload, handler-state persistence between events, and reset after reload,
following the existing `lua_handler_state_persists_between_events` tests.

## Done criteria

- [ ] Startup evaluates the config once.
- [ ] Reload evaluates the new config once.
- [ ] Config and Lua runtime always come from the same evaluation.
- [ ] Existing handler state tests pass.
- [ ] `git status --short` lists only scoped files and the plan status update.
- [ ] `plans/README.md` row is updated.

## STOP conditions

- A public library caller depends on `Daemon::new` rereading disk.
- The observable-count test would execute external commands.

## Maintenance notes

Plan 015 should build on the paired `(Config, Lua)` ownership rather than
introducing a second runtime or evaluation.
