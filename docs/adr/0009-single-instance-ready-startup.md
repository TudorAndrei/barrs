---
id: ADR-0009
kind: decision
title: Establish single-instance readiness before reporting startup
status: accepted
date: 2026-07-23
governs:
  - src/app.rs
  - src/daemon.rs
  - src/error.rs
  - src/ipc.rs
---

## Decision

Use bounded IPC probing to retain a live socket and reclaim only stale socket
endpoints. Detached startup reports success only after the child owns its socket
and completes configuration and renderer initialization.

## Consequences

Two daemons cannot silently share an endpoint or display duplicate bars. A
failed detached startup reaches the caller as a nonzero result.
