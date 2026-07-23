# Plan 013: Align release instructions with the automated release flow

> **Executor instructions**: Do not execute release commands, create tags, or
> change package versions while completing this documentation plan.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- AGENTS.md README.md`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `plans/001-enforce-rust-quality-gates.md`
- **Category**: docs
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

An agent following `AGENTS.md` is told to run a manual patch release, while the
README and active workflow derive the version with Cocogitto, generate the
changelog, and invoke cargo-release in stages. Following the wrong canonical
path can produce an inconsistent release.

## Current state

- `AGENTS.md:9-24` calls `cargo release patch` and
  `cargo release patch --execute`.
- `README.md:184-215` calls `mise run release-plan` and
  `mise run release-auto`.
- `scripts/release-auto.sh:151-172` determines a version with Cocogitto, updates
  changelog/Cargo files, tests, checks `barrs <version>`, commits, tags, and
  pushes with cargo-release.
- `release.toml` requires `v{{version}}` tags and disables publishing.

```text
# AGENTS.md:15,21 versus README.md:197,203
cargo release patch
cargo release patch --execute
mise run release-plan
mise run release-auto
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Script syntax | `bash -n scripts/release-auto.sh` | exit 0 |
| Dry preview | `mise run release-plan` | exit 0 with next/no-release result; no writes |
| Diff hygiene | `git diff --check` | no output |

## Scope

**In scope**: `AGENTS.md`, `README.md`.

**Out of scope**: changing release scripts/workflows, cutting a release,
updating `Cargo.toml`, tags, or `Formula/barrs.rb`.

## Git workflow

- Branch: `advisor/013-release-docs`
- Commit: `docs(release): align canonical release workflow`

## Steps

1. Make `AGENTS.md` name `mise run release-plan` and `mise run release-auto` as
   the standard local flow and explain that automation derives SemVer from
   conventional commits.
   **Verify**: `rg 'cargo release patch' AGENTS.md README.md` has no unlabeled
   standard-flow match.
2. Keep the invariant that cargo-release creates matching crate version/tag and
   `cargo publish` remains disabled. If a manual/emergency procedure remains,
   label it explicitly and include changelog/version verification.
   **Verify**: commands and tag format agree with `mise.toml`, `release.toml`,
   and the script.
3. Run syntax/diff checks; do not run execution mode.

## Test plan

Cross-check every documented command against `mise.toml`, `release.toml`, and
`scripts/release-auto.sh`. Run only the read-only preview and shell syntax
check; never use `--execute`.

## Done criteria

- [x] AGENTS and README identify one canonical release flow.
- [x] Version/tag matching and no-publish policies remain explicit.
- [x] No release state changed.
- [x] `git diff --check` passes.
- [x] `git status --short` lists only scoped files and the plan status update.
- [x] `plans/README.md` row is updated.

## STOP conditions

- The maintainer says manual patch releases are intentionally canonical.
- Documentation cannot match the script without changing release behavior.

## Maintenance notes

Whenever release automation changes, update agent and human instructions in the
same commit.
