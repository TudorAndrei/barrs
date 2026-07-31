---
id: ADR-0008
kind: decision
title: Bound IPC frames and serialize request execution
status: accepted
date: 2026-07-23
governs:
  - src/daemon.rs
  - src/ipc.rs
---

## Decision

Bound request-frame reads and parse each socket connection outside the daemon
loop. Send parsed requests to the daemon, which remains the sole executor of
stateful requests, Lua, renderer work, reload, and shutdown.

## Consequences

Slow or oversized clients cannot block refresh and rendering. AppKit and Lua
objects do not cross connection-task boundaries.
