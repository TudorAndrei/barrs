---
id: ADR-0017
kind: decision
title: Release through the automated conventional-commit flow
status: accepted
date: 2026-07-23
---

## Decision

Use `mise run release-plan` to preview and `mise run release-auto` to execute
releases. Cocogitto derives SemVer; the automation updates metadata and the
changelog, verifies the binary, and lets cargo-release create the matching tag.

## Consequences

Do not hand-edit release versions, create tags manually, or publish to
crates.io. Every release tag must be `v{{version}}` and match the crate version.
