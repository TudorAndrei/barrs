---
id: ADR-0007
kind: decision
title: Return nonzero for daemon-declared errors
status: accepted
date: 2026-07-23
governs:
  - src/app.rs
  - src/error.rs
---

## Decision

Treat an IPC `Response::Error` as an application error. Preserve successful
command output and zero exit status, but print daemon errors to stderr and exit
nonzero.

## Consequences

Shell scripts and service tooling can distinguish a rejected daemon request
from success without changing the serialized IPC response schema.
