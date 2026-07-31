---
id: ADR-0014
kind: decision
title: Publish hover scenes only when presentation changes
status: accepted
date: 2026-07-23
governs:
  - src/daemon.rs
  - src/render.rs
---

## Decision

Do not republish a renderer scene or force AppKit updates when a hover event
leaves the active target and presentation unchanged. Continue dispatching Lua
`hover_update` handlers at their normal cadence.

## Consequences

Stationary-pointer updates avoid redundant UI work without changing the Lua
handler contract. Future handler-driven visual changes require explicit
invalidation.
