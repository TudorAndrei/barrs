---
id: ADR-0005
kind: decision
title: Enforce reproducible Rust quality gates
status: accepted
date: 2026-07-23
---

## Decision

Pin the Rust toolchain and use one locked quality gate: formatting, Clippy with
warnings denied, `cargo check --all-targets --locked`, and `cargo test --locked`.
Expose that gate through `mise run check` and run the same checks in CI.

## Consequences

Toolchain and lint-policy changes must be deliberate and pass the full gate.
This baseline applies before changing daemon, renderer, IPC, or Rift behavior.
