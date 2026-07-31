---
id: ADR-0010
kind: decision
title: Reconcile complete renderer state on reload
status: accepted
date: 2026-07-23
governs:
  - src/daemon.rs
  - src/render.rs
---

## Decision

Treat a reload as a transaction over the complete next configuration and item
snapshot set. Reconcile retained, added, renamed, and removed items once per
reload, including removal of obsolete hover presentation and native layers.

## Consequences

Failed reloads retain the prior working state. Future renderer batching must
preserve complete-next-state reconciliation semantics.
