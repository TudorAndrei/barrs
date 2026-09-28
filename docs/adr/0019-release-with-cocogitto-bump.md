---
id: ADR-0019
kind: decision
title: Release with Cocogitto bump hooks
status: accepted
date: 2026-09-28
supersedes:
  - ADR-0017
---

## Decision

Use `mise run release-plan` to preview and `mise run release-auto` to execute
releases. Both run `cog bump --auto`. Cocogitto derives SemVer, writes the
changelog from `.cog/changelog.tera`, creates the `chore(version)` commit and
the `v{{version}}` tag, and pushes them. Its pre-bump hooks set the Cargo
version with `cargo release version`, run the tests, and verify that
`barrs --version` prints the new version.

## Consequences

There is no release shell script and no `release.toml`. Do not hand-edit
release versions, create tags manually, or publish to crates.io. Every release
tag must be `v{{version}}` and match the crate version. Bot commits use
conventional messages (`chore(version)`, `chore(brew)`), and the changelog
omits `chore` commits.
