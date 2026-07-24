---
id: ADR-0001
kind: decision
title: Maintain executable documentation with criv
status: accepted
date: 2026-07-23
---

## Context

The repository has accumulated implementation plans and design spikes that need
to remain connected to the source code they describe. Plain Markdown alone
cannot detect stale source references or malformed decision metadata.

## Decision

Use criv as the repository's local documentation graph. Keep human-authored
documentation and ADRs in `docs/`, use criv metadata for notes and decisions,
and refresh generated local graph state after documentation or source changes.

The project installs criv through mise. Agent skills, Git hooks, editor
recommendations, and the local `.criv` state are initialized through
`criv init`; `.criv` remains untracked.

## Consequences

Documentation changes must pass `criv check` before completion. Accepted ADRs
are the durable record of architectural decisions; detailed research stays in
linked spikes and plans.
