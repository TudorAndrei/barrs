---
id: ADR-0002
kind: decision
title: Use deterministic targeting for one native bar
status: proposed
date: 2026-07-23
governs:
  - src/config.rs
  - src/render.rs
---

## Context

[[display-targeting]] defines a display-selection design for the native
bar. A display ID alone is not a safe persisted identity, and display topology
changes must not create duplicate bars or move AppKit work off the main thread.

## Decision

Keep one native bar. Add an optional `bar.display` selector with `main` as the
backward-compatible default, `builtin` for a built-in screen, and an exact
vendor/model/nonzero-serial selector for a stable external target.

Resolve the preferred selector against current display descriptors. When the
preferred display is unavailable, render on `main`, preserve the preference, and
reselect the preferred display when it returns. Rebuild only when the selected
screen or its geometry changes.

## Consequences

The implementation must isolate pure selection and validation from AppKit. It
must reject ambiguous or zero-serial stable identities and retain the existing
main-thread ownership and bounded display-reconfiguration lifecycle.

## Status

This is proposed work. The full API, transition table, tests, and go/no-go
criteria remain in [[display-targeting]].
