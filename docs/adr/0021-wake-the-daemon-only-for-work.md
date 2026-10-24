---
id: ADR-0021
kind: decision
title: Wake the daemon only for input, Rift events, and due debounces
status: accepted
date: 2026-10-24
governs:
  - src/daemon.rs
  - src/producer.rs
  - src/render.rs
---

## Decision

The daemon does not run a fixed 16 ms tick. The snapshot producer wakes the
event loop when a Rift event arrives or a Rift render debounce is due; the
Rift subscription is an async channel that the producer awaits.

The renderer sets the wait before its next event poll. The `AppKit` host polls
every 16 ms while the pointer is on or near the bar, or while a display change
settles, and every 100 ms otherwise. It sends a hover update only when the
pointer moved, and it looks up the bar screen only after a display
reconfiguration.

## Consequences

An idle bar wakes a few times each second, not about 60 times. The first hover
or click after the pointer comes to the bar from far away can wait up to
100 ms. Rift changes still render one debounce after their event, as
ADR-0013 describes.
