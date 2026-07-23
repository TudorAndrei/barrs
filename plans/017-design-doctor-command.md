# Plan 017: Design a redacted `barrs doctor` command

> **Executor instructions**: This is a product/CLI design plan. Do not implement
> the command until its output contract and redaction policy are reviewed.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- README.md src/app.rs src/cli.rs src/ipc.rs src/render.rs src/rift.rs`

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: LOW
- **Depends on**: `plans/003-propagate-daemon-response-errors.md`,
  `plans/005-make-startup-single-instance-and-ready.md`
- **Category**: direction
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

Troubleshooting currently requires combining status, config validation,
dump-state, Rift backend inspection, service logs, and a hidden display-debug
environment variable. A single read-only diagnostic surface could make failures
actionable, but it needs stable machine output and explicit privacy boundaries.

## Current state

- `README.md:170-181` lists separate `status`, `validate-config`,
  `dump-state`, and `rift backend` commands.
- `src/cli.rs` models commands with Clap subcommands and value enums.
- `src/ipc.rs:87-119` defines request/response types.
- `src/render.rs:1604-1644` logs detailed display paths/geometry behind
  `BARRS_DEBUG_DISPLAYS`.
- Foreground and Homebrew/launchd modes have different logging behavior.

```rust
// src/cli.rs:15-27 (abridged)
pub enum Command {
    Start(StartArgs),
    Stop(SocketArgs),
    ValidateConfig(ConfigArgs),
    DumpState(SocketArgs),
    Rift(RiftArgs),
}
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Baseline | `mise run check` | exit 0 |
| CLI reference | `cargo run --locked -- --help` | exit 0 |
| Document hygiene | `git diff --check` | no output |

## Scope

**In scope**: create `docs/spikes/doctor-command.md`.

**Out of scope**: implementing CLI/IPC changes, invoking launchctl in production,
uploading reports, reading or printing Lua source, secrets, or full user paths.

## Git workflow

- Branch: `advisor/017-doctor-design`
- Commit: `docs(design): specify doctor diagnostics`

## Steps

1. Inventory reusable local and daemon-assisted probes: binary version, config
   existence/validation, socket ownership/responsiveness, daemon status, Rift
   backend, target display summary, and optional launchd/Homebrew hints.
   **Verify**: every probe cites its existing symbol/command or is labeled new.
2. Define human and `--json` schemas with stable check IDs, pass/warn/fail
   status, actionable message, and optional details. Specify exit status for
   healthy, warnings-only, and failures.
   **Verify**: example outputs validate against the documented JSON shape.
3. Define redaction: home paths, item state, environment values, config content,
   and service logs must be omitted or normalized by default. Specify an
   explicit opt-in verbose mode if needed.
   **Verify**: privacy checklist and tests are included.
4. Break future implementation into CLI-only probes, daemon protocol additions,
   optional service integration, and documentation.
   **Verify**: each phase has source/test targets and STOP conditions.

## Test plan

The design must specify golden human/JSON outputs, exit-status cases,
unavailable daemon/Rift/service behavior, and redaction tests containing
representative home paths and sensitive-looking environment/config data.

## Done criteria

- [x] Diagnostic and JSON contracts are complete.
- [x] Exit statuses align with Plan 003.
- [x] Redaction rules prevent accidental sensitive output.
- [x] Foreground and service installs are both supported.
- [x] No production code changed.
- [x] `git status --short` lists only the spike document and plan status update.
- [x] `plans/README.md` row is updated.

## STOP conditions

- A proposed default probe reads configuration content or arbitrary logs.
- Stable output requires exposing internal AppKit objects.

## Maintenance notes

Treat JSON check IDs and field meanings as public automation API once
implemented.
