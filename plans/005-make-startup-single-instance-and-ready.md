# Plan 005: Make startup single-instance and readiness-aware

> **Executor instructions**: Do not add PID-file-only ownership; PIDs can be
> reused. Preserve foreground debug behavior.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- src/app.rs src/daemon.rs src/error.rs src/ipc.rs`

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: `plans/003-propagate-daemon-response-errors.md`,
  `plans/004-harden-ipc-framing-and-concurrency.md`
- **Category**: bug
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

Startup currently removes any existing socket before binding, so a second
instance steals the live daemon's endpoint and leaves an unreachable duplicate
bar. Detached release startup also prints success before the child validates
configuration, initializes AppKit, or owns a responsive socket.

## Current state

- `src/daemon.rs:82-90` renders first, unconditionally calls
  `cleanup_socket`, then binds.
- `src/daemon.rs:533-547` deletes every existing socket without probing it.
- `src/app.rs:30-38` spawns a detached child and immediately prints “started”.
- `src/app.rs:144-173` discards child stdout/stderr and retains no readiness
  channel.
- Debug `start` runs in the foreground by design (`src/app.rs:21-29`).

```rust
// src/daemon.rs:89-90
cleanup_socket(&socket_path)?;
let listener = UnixListener::bind(&socket_path)?;
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Focused tests | `cargo test --locked daemon::tests` | all daemon lifecycle tests pass |
| Version | `cargo run --locked -- --version` | `barrs 0.2.3` |
| Full gate | `mise run check` | exit 0 |

## Scope

**In scope**: `src/app.rs`, `src/daemon.rs`, `src/error.rs`, `src/ipc.rs`.

**Out of scope**: launchd redesign, Homebrew formula changes, multi-user socket
authentication, release tagging.

## Git workflow

- Branch: `advisor/005-startup-lifecycle`
- Commit: `fix(daemon): enforce single-instance ready startup`

## Steps

1. Replace unconditional cleanup with stale-socket reclamation: if the path is a
   socket, perform a bounded Ping using Plan 004's responsive IPC. A Pong means
   “already running” and must never unlink. Connection-refused/stale endpoints
   may be removed. Preserve refusal for non-socket paths.
   **Verify**: tests cover live socket, stale socket, regular file, and two
   concurrent bind attempts.
2. Ensure ownership is established before expensive initial rendering so a
   losing instance cannot display a duplicate bar.
   **Verify**: the second daemon fails before renderer initialization in a
   counting-renderer test.
3. Add a bounded parent-child readiness/error handshake for detached `start`.
   The parent prints success only after config load, renderer initialization,
   and socket ownership. Preserve the child's startup error for stderr/nonzero
   exit.
   **Verify**: invalid config and live-daemon tests return nonzero; valid noop
   startup reports ready.
4. Keep debug `start` foreground behavior and graceful Stop socket cleanup.
   **Verify**: `mise run check` exits 0.

## Test plan

Extend the existing socket cleanup tests with a live listener/Ping server,
stale endpoint, regular file, concurrent starts, invalid Lua child, renderer
initialization failure, readiness timeout, and graceful Stop cleanup.

## Done criteria

- [x] A live socket is never unlinked.
- [x] Concurrent startup yields one initialized renderer.
- [x] Detached success means the new daemon is responsive.
- [x] Startup failures reach the parent and return nonzero.
- [x] Foreground development behavior remains intact.
- [x] `git status --short` lists only scoped files and the plan status update.
- [x] `plans/README.md` row is updated.

## STOP conditions

- Reliable readiness would require changing the public wire schema incompatibly.
- The handshake can deadlock if the parent exits.
- A solution relies only on PID liveness.

## Maintenance notes

Startup probes depend on Plan 004's bounded, nonblocking IPC. Review socket-path
override behavior and ownership together whenever the protocol changes.
