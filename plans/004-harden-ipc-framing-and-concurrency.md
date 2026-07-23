# Plan 004: Bound IPC frames without blocking the daemon loop

> **Executor instructions**: Keep request execution serialized on the daemon
> thread; only framing/parsing may run concurrently.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- Cargo.toml src/daemon.rs src/ipc.rs`

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: `plans/001-enforce-rust-quality-gates.md`,
  `plans/002-characterize-daemon-state-machines.md`
- **Category**: security
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

The daemon awaits each accepted socket inline and uses unbounded `read_line`.
A silent client freezes all 16 ms renderer/Rift work for two seconds, while a
fast client can allocate an arbitrarily large request buffer.

## Current state

- `src/daemon.rs:94-130` multiplexes accept, renderer events, Rift events, and
  refreshes in one `tokio::select!`.
- `src/daemon.rs:107-110` awaits `handle_connection` inline.
- `src/daemon.rs:140-162` reads into an unbounded `String` with a two-second
  timeout.
- `src/daemon.rs:1011-1034` proves recovery only after sleeping 2.5 seconds.
- `src/ipc.rs:121-136` defines newline-delimited request/response framing.

```rust
// src/daemon.rs:107-110, 140-145 (abridged)
accept_result = listener.accept() => {
    Ok((stream, _)) => self.handle_connection(stream).await
}
let mut line = String::new();
reader.read_line(&mut line)
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Focused tests | `cargo test --locked daemon::tests` | all daemon IPC tests pass |
| Full gate | `mise run check` | exit 0 |

## Scope

**In scope**: `src/daemon.rs`, `src/ipc.rs`; `Cargo.toml` only for required Tokio
features.

**Out of scope**: authentication, changing JSON schemas, parallel Lua/AppKit
execution, multiple requests per connection.

## Git workflow

- Branch: `advisor/004-ipc-hardening`
- Commit: `fix(ipc): bound frames and isolate slow clients`

## Steps

1. Define a shared, generous request-frame byte limit near the IPC protocol.
   Read at most limit+1 bytes and distinguish timeout, EOF, malformed UTF-8/JSON,
   and oversized input. Return a bounded `Response::Error` where possible.
   **Verify**: exact-limit and over-limit tests pass; daemon stays responsive.
2. Move socket reading/parsing into per-connection tasks. Send parsed requests
   plus response channels into the main daemon loop, which alone calls
   `handle_request`, Lua, renderer, reload, and stop logic.
   **Verify**: code contains no AppKit/Lua state inside spawned connection tasks.
3. Replace the current silent-client test with one that holds a connection open
   while a second ping and a scheduled renderer refresh complete well below the
   two-second timeout.
   **Verify**: focused tests pass repeatedly (run them three times).
4. Add cleanup behavior for abandoned response receivers and ensure Stop sends
   its response before breaking the loop.
   **Verify**: `mise run check` exits 0.

## Test plan

Model connection tests after `daemon_survives_silent_client`. Add exact-limit,
limit+1, partial-frame timeout, abandoned client, concurrent ping, scheduled
refresh, malformed JSON, and Stop-response cases.

## Done criteria

- [x] Frames larger than the limit never allocate beyond the bounded buffer.
- [x] One silent client does not delay ping or scheduled refresh.
- [x] Stateful request execution remains serialized.
- [x] Stop/reload/error response tests pass.
- [x] `mise run check` exits 0.
- [x] `git status --short` lists only scoped files and the plan status update.
- [x] `plans/README.md` row is updated.

## STOP conditions

- The proposed task requires `Renderer`, `Lua`, or AppKit objects to become
  cross-thread.
- Stop semantics become nondeterministic.
- Supporting multiple frames per connection appears necessary.

## Maintenance notes

Keep client and server framing constants synchronized. If dump-state responses
later become large, add a separate response limit rather than raising the
request limit blindly.
