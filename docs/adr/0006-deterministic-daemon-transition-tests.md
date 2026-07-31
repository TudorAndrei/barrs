---
id: ADR-0006
kind: decision
title: Test daemon transitions deterministically
status: accepted
date: 2026-07-23
governs:
  - src/daemon.rs
---

## Decision

Model scheduler and Rift-debounce transitions with small test seams and explicit
state assertions. Avoid long wall-clock sleeps and assertions based only on
minimum render counts.

## Consequences

Future daemon state-machine changes must test stale epochs, pending refreshes,
terminal branches, and debounce decisions deterministically.
