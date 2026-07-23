# Plan: Harden and evolve barrs from the full implementation audit

## Goal

Resolve all 16 audited correctness, security, performance, test, DX, and
documentation findings, then produce reviewed design decisions for the three
grounded product directions. The work should leave barrs single-instance,
truthful at the CLI boundary, bounded and responsive over IPC, correct across
reload/Rift/hover transitions, and explicit about AppKit main-thread ownership.

## Approach

Work is divided into small conventional commits, each detailed in a numbered
handoff plan under `plans/`. Begin with a pinned verification baseline and
deterministic daemon tests. Keep socket framing concurrent but all Lua, daemon
state, and AppKit execution serialized. Treat reload as complete-state
reconciliation and remove AppKit's unsafe cross-thread promise. Documentation
plans follow the stabilized behavior; direction plans are design spikes only.

The repository's existing conventions remain authoritative: inline Rust module
tests, `BarrsError`/`Result` error propagation, serde-tagged IPC enums, a
current-thread Tokio runtime, conventional commits, cargo-release-managed
version/tag matching, and no crates.io publishing.

Out of scope for this program: SketchyBar compatibility, non-macOS rendering,
multi-bar support, dependency upgrades without advisory/EOL evidence, release
execution, and implementation of the three direction spikes.

## Implementation Phases

### Phase 1: Establish the quality gate

- Execute [Plan 001](./001-enforce-rust-quality-gates.md): pin Rust, expose one
  local check task, fix current format/Clippy drift, and enforce the gate in CI.
  **Commit:** `chore(ci): enforce rust quality gates`

### Phase 2: Characterize daemon transitions

- Execute [Plan 002](./002-characterize-daemon-state-machines.md): add
  deterministic scheduler/Rift state seams and tests without fixing later bugs.
  **Commit:** `test(daemon): characterize refresh and rift state transitions`

### Phase 3: Make daemon errors fail the CLI

- Execute [Plan 003](./003-propagate-daemon-response-errors.md): propagate
  `Response::Error` through `BarrsError` to a nonzero process result.
  **Commit:** `fix(cli): propagate daemon response errors`

### Phase 4: Bound and decouple IPC

- Execute [Plan 004](./004-harden-ipc-framing-and-concurrency.md): limit request
  frames and move framing off the stateful daemon loop.
  **Commit:** `fix(ipc): bound frames and isolate slow clients`

### Phase 5: Make startup owned and truthful

- Execute [Plan 005](./005-make-startup-single-instance-and-ready.md): refuse a
  live daemon, reclaim only stale sockets, and wait for child readiness.
  **Commit:** `fix(daemon): enforce single-instance ready startup`

### Phase 6: Reconcile complete renderer state

- Execute [Plan 006](./006-reconcile-renderer-items-on-reload.md): remove stale
  items/hover state and make reload transactional.
  **Commit:** `fix(render): reconcile items during reload`

### Phase 7: Restore AppKit confinement

- Execute [Plan 007](./007-confine-appkit-renderer-to-main-thread.md): remove
  unsafe cross-thread promises and retain main-thread ownership.
  **Commit:** `refactor(render): confine appkit host to main thread`

### Phase 8: Load each Lua config once

- Execute [Plan 008](./008-evaluate-lua-config-once.md): keep each `Config` and
  `Lua` runtime from one evaluation.
  **Commit:** `fix(config): evaluate lua once per load`

### Phase 9: Finish no-op Rift debounce cycles

- Execute [Plan 009](./009-clear-rift-debounce-terminal-state.md): clear terminal
  dirty/deadline state and subscribe only for Rift consumers.
  **Commit:** `fix(rift): finish no-op debounce cycles`

### Phase 10: Elide redundant hover presentation

- Execute [Plan 010](./010-elide-unchanged-hover-publications.md): preserve Lua
  updates while skipping unchanged scene/AppKit work.
  **Commit:** `perf(render): skip unchanged hover publications`

### Phase 11: Preserve exact Rift counts

- Execute [Plan 011](./011-preserve-rift-window-counts.md): retain or resync
  exact per-workspace counts rather than converting booleans.
  **Commit:** `fix(rift): preserve workspace window counts`

### Phase 12: Correct synthetic hover targeting

- Execute [Plan 012](./012-target-synthetic-hover-by-item-id.md): use the
  validated item identity instead of default coordinates.
  **Commit:** `fix(render): honor synthetic hover item targets`

### Phase 13: Align release instructions

- Execute [Plan 013](./013-align-release-documentation.md): make AGENTS and
  README describe the same Cocogitto/cargo-release flow.
  **Commit:** `docs(release): align canonical release workflow`

### Phase 14: Publish the Lua handler contract

- Execute [Plan 014](./014-document-lua-handler-contract.md): document all
  handlers, context fields, error behavior, and ignored returns.
  **Commit:** `docs(config): define lua handler contract`

### Phase 15: Specify custom Lua snapshots

- Execute [Plan 015](./015-spike-lua-snapshot-providers.md): write the API,
  execution, state, error, and rollout decision document only.
  **Commit:** `docs(design): specify lua snapshot providers`

### Phase 16: Specify display targeting

- Execute [Plan 016](./016-spike-display-targeting.md): choose stable selector
  and hotplug fallback semantics while deferring multi-bar support.
  **Commit:** `docs(design): specify display targeting`

### Phase 17: Specify `barrs doctor`

- Execute [Plan 017](./017-design-doctor-command.md): define probes, human/JSON
  output, exit statuses, redaction, and implementation slices.
  **Commit:** `docs(design): specify doctor diagnostics`

## Risks & Tradeoffs

- IPC concurrency must never move Lua/AppKit objects into connection tasks;
  value-only request/response channels are the safe boundary.
- Socket liveness probes are trustworthy only after slow-client isolation.
- Renderer reconciliation must remove stale layers without destroying/recreating
  the native window on every reload.
- Removing `Send + Sync` may require local-task test changes; retaining unsafe
  promises for convenience is not acceptable.
- Exact Rift counts depend on a verified payload or resync source; guessing the
  protocol would replace a visible bug with silent drift.
- Denying all Clippy warnings increases upgrade friction but makes the selected
  pinned toolchain deterministic.

## Open Questions

- Does Rift reliably include exact counts in workspace events, or must Phase 11
  resync on each switch? Recommended: resync when exactness is not provable.
- Should Lua snapshot providers ever execute off-thread given `mlua` runtime
  ownership? Plan 015 must decide before implementation.
- Which public macOS display identifier is stable enough for Plan 016? Stop the
  feature if none satisfies the documented transition matrix.
