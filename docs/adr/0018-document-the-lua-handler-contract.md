---
id: ADR-0018
kind: decision
title: Treat the Lua handler contract as public API
status: accepted
date: 2026-07-23
governs:
  - src/config.rs
  - src/daemon.rs
  - src/ipc.rs
---

## Decision

Document all supported handler slots, the value-only event context, nullable
mouse fields, and modifier flags. Handler return values are ignored; configured
handler failures propagate through the daemon error path.

## Consequences

Changes to event payloads or handler names require matching documentation and
tests. Examples must remain deterministic and avoid platform side effects.
