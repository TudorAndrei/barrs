# Plan 003: Return nonzero status for daemon response errors

> **Executor instructions**: Preserve existing successful output exactly.
>
> **Drift check (run first)**:
> `git diff --stat ff4d7df..HEAD -- src/app.rs src/error.rs`

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `plans/001-enforce-rust-quality-gates.md`
- **Category**: bug
- **Planned at**: commit `ff4d7df`, 2026-07-23

## Why this matters

`barrs reload`, `stop`, and item triggers return process status 0 even when the
daemon returns `Response::Error`. Shell scripts and service tooling therefore
cannot distinguish success from failure.

## Current state

- `src/app.rs:51-111` prints responses and returns `Ok(())`.
- `src/app.rs:190-216` sends `Response::Error.message` to stderr but
  `print_response` returns `()`.
- `src/main.rs` already returns `Result`, so a `BarrsError` naturally produces a
  nonzero process exit.
- Errors are centralized in `src/error.rs`; add any new variant there.

```rust
// src/app.rs:216
Response::Error { message } => eprintln!("{message}"),
```

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Focused tests | `cargo test --locked app::tests` | all pass |
| Full gate | `mise run check` | exit 0 |

## Scope

**In scope**: `src/app.rs`, `src/error.rs`.

**Out of scope**: changing response JSON, adding machine-readable CLI output,
startup readiness, daemon protocol authentication.

## Git workflow

- Branch: `advisor/003-cli-response-errors`
- Commit: `fix(cli): propagate daemon response errors`

## Steps

1. Add a specific error representation for a daemon-declared failure; do not
   mislabel it as transport unavailability.
   **Verify**: `cargo check --all-targets --locked` exits 0.
2. Make `print_response` return `Result<(), BarrsError>`. Return the new error
   for `Response::Error`; preserve stdout formatting for every successful
   variant. Propagate the result from every caller with `?`.
   **Verify**: focused unit tests cover one successful and one error response.
3. If a process-level test can invoke the binary without starting a real native
   renderer, assert error status is nonzero and the daemon message is on stderr.
   **Verify**: `mise run check` exits 0.

## Test plan

Add unit tests for each successful output family and `Response::Error`. Add a
noop/socket-backed process test if it can remain deterministic and AppKit-free.

## Done criteria

- [ ] A daemon error reaches `main` as `Err`.
- [ ] Successful commands retain their output and zero status.
- [ ] New unit/process tests pass.
- [ ] `mise run check` exits 0.
- [ ] `git status --short` lists only scoped files and the plan status update.
- [ ] `plans/README.md` row is updated.

## STOP conditions

- The change requires altering serialized `Response`.
- Tests require launching AppKit or an installed Homebrew service.

## Maintenance notes

Any future JSON-output mode should encode failure on stdout only by explicit
contract while retaining a nonzero process status.
