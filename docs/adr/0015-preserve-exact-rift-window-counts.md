---
id: ADR-0015
kind: decision
title: Preserve exact Rift workspace window counts
status: accepted
date: 2026-07-23
governs:
  - src/plugin.rs
  - src/rift.rs
---

## Decision

Keep an exact window count for each Rift workspace and derive occupancy from
that count. Do not infer a count from a boolean when applying workspace events.

## Consequences

Workspace and layout plugins report correct multi-window state across active
workspace changes. Missing exact data must trigger resynchronization instead of
guessing.
