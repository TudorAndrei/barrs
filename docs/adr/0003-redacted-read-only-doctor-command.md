---
id: ADR-0003
kind: decision
title: Provide a redacted read-only doctor command
status: proposed
date: 2026-07-23
governs:
  - src/app.rs
  - src/cli.rs
  - src/config.rs
  - src/ipc.rs
  - src/render.rs
  - src/rift.rs
---

## Context

[[doctor-command]] defines a support-oriented diagnostic interface that
works with a running daemon, a managed service, or no daemon. Diagnostics must
be safe to paste into an issue and must not mutate daemon or service state.

## Decision

Add a future `barrs doctor` command with stable check IDs and human and JSON
output. It will aggregate bounded, read-only probes for the binary,
configuration, socket, daemon, Rift backend, display summary, and service hint.

Output must redact configuration content, item data, environment values, logs,
full paths, raw display identifiers, and hardware serials. Warnings and skipped
checks do not fail the command; one or more failed checks return a nonzero exit
status.

## Consequences

Probe results require a stable schema and allow-listed details. Platform and
daemon adapters must be isolated so an unavailable optional integration becomes
a warning rather than an unsafe or disruptive recovery attempt.

## Status

This is proposed work. The probe inventory, output contract, privacy rules, and
implementation test matrix remain in [[doctor-command]].
