---
id: ADR-0011
kind: decision
title: Confine retained AppKit objects to the main thread
status: accepted
date: 2026-07-23
governs:
  - src/daemon.rs
  - src/main.rs
  - src/render.rs
---

## Decision

Do not promise `Send` or `Sync` for the native renderer or retained AppKit
objects. Keep production on the current-thread runtime and require main-thread
access at every AppKit entry point.

## Consequences

Background work must use value-only messages to a main-thread owner; it must
never move `NSApplication`, windows, views, or panels across threads.
