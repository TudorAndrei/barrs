# Implementation Plans

Generated from the standard-depth `improve` audit on 2026-07-23 at commit
`ff4d7df`. All 16 presented findings and all three direction options are
represented below. Execute in dependency order, read the selected plan fully
before starting, honor its STOP conditions, and update its status row.

The aggregate `$plan` view is in [PLAN.md](./PLAN.md); implementation tracking
is in [TODO.md](./TODO.md).

## Execution order and status

| Plan | Title | Covers | Priority | Effort | Depends on | Status |
|---|---|---|---|---|---|---|
| 001 | Enforce Rust quality gates | Finding 11 | P1 | S | — | DONE |
| 002 | Characterize daemon state machines | Finding 14 | P1 | M | 001 | DONE |
| 003 | Propagate daemon response errors | Finding 3 | P1 | S | 001 | DONE |
| 004 | Harden IPC framing and concurrency | Findings 2, 6 | P1 | M | 001, 002 | DONE |
| 005 | Make startup single-instance and ready | Findings 1, 7 | P1 | M | 003, 004 | DONE |
| 006 | Reconcile renderer items on reload | Finding 4 | P1 | M | 001, 002 | DONE |
| 007 | Confine AppKit to the main thread | Finding 5 | P1 | M | 001, 006 | DONE |
| 008 | Evaluate Lua config once | Finding 8 | P2 | S | 001 | DONE |
| 009 | Clear Rift terminal dirty state | Finding 9 | P2 | S | 002 | DONE |
| 010 | Elide unchanged hover publications | Finding 10 | P2 | S | 006 | DONE |
| 011 | Preserve Rift window counts | Finding 12 | P2 | M | 002 | DONE |
| 012 | Target synthetic hover by item ID | Finding 13 | P2 | S | 003, 006 | DONE |
| 013 | Align release documentation | Finding 15 | P2 | S | 001 | DONE |
| 014 | Document Lua handler contract | Finding 16 | P2 | S | 003, 012 | TODO |
| 015 | Spike Lua snapshot providers | Direction 1 | P3 | M | 008, 014 | TODO |
| 016 | Spike display targeting | Direction 2 | P3 | M | 007 | TODO |
| 017 | Design `barrs doctor` | Direction 3 | P3 | M | 003, 005 | TODO |

Status values: `TODO`, `IN PROGRESS`, `DONE`, `BLOCKED: <reason>`, or
`REJECTED: <reason>`.

## Dependency notes

- 001 lands first so every executor uses the same formatter, linter, compiler,
  and locked dependency graph.
- 002 creates deterministic state-machine seams used by Rift and reload work.
- 004 precedes 005 because startup liveness probes must not be defeated by a
  silent IPC client.
- 003 precedes 005 and 012 so startup/trigger failures can return nonzero.
- 006 precedes 007 and 010 so AppKit ownership and hover optimizations build on
  correct complete-state reconciliation.
- Direction spikes follow the corresponding correctness/API plans and must not
  silently implement production features.

## Finding-to-plan map

| Audit finding | Plan |
|---|---|
| 1. Live daemon socket takeover | 005 |
| 2. Unbounded IPC request frames | 004 |
| 3. Daemon errors exit with status 0 | 003 |
| 4. Obsolete items survive reload | 006 |
| 5. Unsound AppKit `Send + Sync` contract | 007 |
| 6. Slow IPC client stalls the event loop | 004 |
| 7. Detached startup reports false success | 005 |
| 8. Lua config evaluated twice | 008 |
| 9. Rift dirty state spins after no-op work | 009 |
| 10. Unchanged hover updates republish at 62 Hz | 010 |
| 11. Formatting/Clippy are not enforced | 001 |
| 12. Rift window counts collapse to 0/1 | 011 |
| 13. Synthetic hover ignores requested item ID | 012 |
| 14. Daemon/Rift transitions lack deterministic tests | 002 |
| 15. Release instructions disagree | 013 |
| 16. Lua handler contract is undocumented | 014 |

## Findings considered and rejected or deferred

- Arbitrary Lua execution is an intentional local configuration capability, not
  injection. Plan 015 must preserve that trust model explicitly.
- Fixed `top`, `pmset`, `ioreg`, and `rift-cli` argument lists do not interpolate
  IPC/Lua values; command injection was unsupported.
- The default predictable socket filename is inside the audited per-user macOS
  temporary parent; no cross-user exposure was established. Plans 004/005 still
  harden availability and ownership.
- Immediate removal of the private SkyLight call was rejected without evidence
  of a supported replacement or observed breakage.
- Upgrading `mlua` or adding an update bot solely because newer versions exist
  was rejected; no advisory or EOL evidence was available.
- Nested linear item scans and 20 ms child polling are low leverage at normal bar
  sizes.
- Batch scene publication and display-geometry caching are plausible follow-ups
  but require profiling after Plans 009/010; they are not current plans.
- Splitting `src/render.rs` is deferred until Plans 006/007 establish stable
  reconciliation and threading boundaries.
- A full AppKit lifecycle smoke suite is deferred until the main-thread boundary
  from Plan 007 is explicit; pure scheduling tests belong with later extraction.

## Verification baseline at planning time

- `cargo check --all-targets`: passed.
- `cargo test --locked`: 89 passed.
- `cargo run -- --version`: printed `barrs 0.2.3`.
- `rustc --version`: `rustc 1.97.1`; `cargo --version`: `cargo 1.97.1`.
- `cargo fmt --all -- --check`: failed with formatting drift.
- `cargo clippy --all-targets -- -D warnings`: failed with five diagnostics.
- Dependency advisories were not established: `cargo-audit` is unavailable and
  repository Dependabot alerts are disabled.
