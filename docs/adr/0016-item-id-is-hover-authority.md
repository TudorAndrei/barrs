---
id: ADR-0016
kind: decision
title: Use validated item IDs for synthetic hover events
status: accepted
date: 2026-07-23
governs:
  - src/daemon.rs
  - src/ipc.rs
  - src/render.rs
---

## Decision

For synthetic hover enter, update, and leave events, use the validated payload
item ID as the target. Retain coordinate hit testing solely for native pointer
event discovery.

## Consequences

`barrs item trigger` targets the requested item even with no meaningful mouse
coordinates. Unknown IDs preserve the established daemon-error path.
