---
id: ADR-0013
kind: decision
title: Terminate Rift debounce cycles that require no render
status: accepted
date: 2026-07-23
governs:
  - src/daemon.rs
---

## Decision

Clear Rift dirty state and its deadline on every terminal no-render branch,
including no configured Rift consumers and an unchanged final signature.
Subscribe to Rift events only while configured items consume them.

## Consequences

The 16 ms daemon tick does not repeatedly scan completed no-op work. Reload
re-evaluates subscription ownership when Rift items change.
