---
id: ADR-0004
kind: decision
title: Isolate Lua snapshot providers in one executor
status: proposed
date: 2026-07-23
governs:
  - src/config.rs
  - src/daemon.rs
  - src/plugin.rs
  - src/render.rs
---

## Context

[[lua-snapshot-providers]] considers user-defined Lua functions that
produce item snapshots on a refresh interval. Calling arbitrary Lua work in the
daemon or renderer loop risks blocking IPC and AppKit interaction.

## Decision

If Lua-defined snapshot providers are implemented, run all provider and handler
work through one dedicated current-thread Lua executor. The daemon owns
scheduling, snapshots, and rendering; the executor owns a single loaded Lua
runtime and exchanges only serializable request and result values with the
daemon.

Provider work is serialized per item and overlapping due ticks are coalesced.
Failures publish a bounded unavailable snapshot. Reload must construct a new
Lua generation before atomically swapping it into the live daemon.

## Consequences

The feature is conditional on proving that over-budget or stuck provider work
cannot permanently block the executor. The implementation must preserve the
existing plugin and handler behavior, reject provider/plugin conflicts, and
keep AppKit and Lua values out of cross-thread messages.

## Status

This is proposed work. The candidate Lua API, state machine, security boundary,
and prototype acceptance criteria remain in [[lua-snapshot-providers]].
