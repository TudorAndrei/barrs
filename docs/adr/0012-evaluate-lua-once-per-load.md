---
id: ADR-0012
kind: decision
title: Evaluate Lua configuration once per lifecycle load
status: accepted
date: 2026-07-23
governs:
  - src/app.rs
  - src/config.rs
  - src/daemon.rs
---

## Decision

Load configuration as an atomic `(Config, Lua)` pair and pass that pair into the
daemon. Startup and reload each evaluate Lua exactly once.

## Consequences

The daemon configuration and handler runtime always arise from one evaluation.
Handler state persists within a generation and resets only after a successful
reload.
