# TODO: Harden and evolve barrs from the full implementation audit

Check off a phase only after its detailed plan's done criteria pass and its
commit succeeds.

## Phase 1: Quality gate

- [x] Complete `plans/001-enforce-rust-quality-gates.md`.
- [x] `mise run check` passes on the pinned toolchain.
- [x] Commit: `chore(ci): enforce rust quality gates`

## Phase 2: Daemon transition coverage

- [x] Complete `plans/002-characterize-daemon-state-machines.md`.
- [x] Scheduler/Rift tests are deterministic and do not codify known bugs.
- [x] Commit: `test(daemon): characterize refresh and rift state transitions`

## Phase 3: CLI error status

- [x] Complete `plans/003-propagate-daemon-response-errors.md`.
- [x] Daemon-declared errors produce nonzero process status.
- [x] Commit: `fix(cli): propagate daemon response errors`

## Phase 4: IPC hardening

- [x] Complete `plans/004-harden-ipc-framing-and-concurrency.md`.
- [x] Oversized/silent-client regressions pass.
- [x] Commit: `fix(ipc): bound frames and isolate slow clients`

## Phase 5: Startup lifecycle

- [x] Complete `plans/005-make-startup-single-instance-and-ready.md`.
- [x] Concurrent/live/invalid/ready startup cases pass.
- [x] Commit: `fix(daemon): enforce single-instance ready startup`

## Phase 6: Reload reconciliation

- [x] Complete `plans/006-reconcile-renderer-items-on-reload.md`.
- [x] Removed/renamed/failed-reload tests pass.
- [x] Commit: `fix(render): reconcile items during reload`

## Phase 7: AppKit main-thread ownership

- [x] Complete `plans/007-confine-appkit-renderer-to-main-thread.md`.
- [x] Unsafe AppKit `Send`/`Sync` implementations are absent.
- [x] Commit: `refactor(render): confine appkit host to main thread`

## Phase 8: Single config evaluation

- [x] Complete `plans/008-evaluate-lua-config-once.md`.
- [x] Startup/reload each evaluate Lua exactly once.
- [x] Commit: `fix(config): evaluate lua once per load`

## Phase 9: Rift terminal state

- [x] Complete `plans/009-clear-rift-debounce-terminal-state.md`.
- [x] No-consumer/equal-signature regression tests pass.
- [x] Commit: `fix(rift): finish no-op debounce cycles`

## Phase 10: Hover publication

- [x] Complete `plans/010-elide-unchanged-hover-publications.md`.
- [x] Same-target updates preserve handlers without AppKit publication.
- [x] Commit: `perf(render): skip unchanged hover publications`

## Phase 11: Rift counts

- [x] Complete `plans/011-preserve-rift-window-counts.md`.
- [x] Counts 0, 1, and 3 remain exact across workspace switches.
- [x] Commit: `fix(rift): preserve workspace window counts`

## Phase 12: Synthetic hover

- [x] Complete `plans/012-target-synthetic-hover-by-item-id.md`.
- [x] Enter/update/leave target the requested ID; unknown IDs fail.
- [x] Commit: `fix(render): honor synthetic hover item targets`

## Phase 13: Release documentation

- [x] Complete `plans/013-align-release-documentation.md`.
- [x] AGENTS and README name one canonical non-executed flow.
- [x] Commit: `docs(release): align canonical release workflow`

## Phase 14: Lua handler documentation

- [x] Complete `plans/014-document-lua-handler-contract.md`.
- [x] Handler/context/return/error contract and sample are accurate.
- [x] Commit: `docs(config): define lua handler contract`

## Phase 15: Lua provider spike

- [x] Complete `plans/015-spike-lua-snapshot-providers.md`.
- [x] Design document resolves or blocks execution/runtime semantics.
- [x] Commit: `docs(design): specify lua snapshot providers`

## Phase 16: Display targeting spike

- [x] Complete `plans/016-spike-display-targeting.md`.
- [x] Selector, fallback, transition matrix, and compatibility are specified.
- [ ] Commit: `docs(design): specify display targeting`

## Phase 17: Doctor command design

- [ ] Complete `plans/017-design-doctor-command.md`.
- [ ] Probe, JSON, exit-status, redaction, and phase contracts are specified.
- [ ] Commit: `docs(design): specify doctor diagnostics`

## Verification

- [ ] `cargo fmt --all -- --check` exits 0.
- [ ] `cargo clippy --all-targets -- -D warnings` exits 0.
- [ ] `cargo check --all-targets --locked` exits 0.
- [ ] `cargo test --locked` passes all existing and new tests.
- [ ] `cargo run --locked -- --version` matches the intended crate version/tag.
- [ ] Manual smoke: foreground noop daemon starts, answers ping/status, reloads,
  rejects a second instance, removes a deleted item, and stops cleanly.
- [ ] Manual smoke on macOS: native bar handles hover/click and display hotplug
  without leaving duplicate windows or stale layers.
- [ ] Edge cases: oversized/silent IPC, stale/live socket, invalid detached
  startup, failed reload, no-consumer Rift event, equal Rift signature,
  multi-window workspace switch, synthetic hover with mismatched coordinates.
- [ ] Direction phases changed only `docs/spikes/*.md`.

## Review

- [ ] Every detailed plan's drift check was run before implementation.
- [ ] Code reviewed with special attention to unsafe/AppKit and IPC boundaries.
- [ ] `plans/PLAN.md` and affected detailed plans were updated if approach
  changed.
- [ ] Every phase is a clean conventional commit with its drafted message.
- [ ] `plans/README.md` status rows reflect actual outcomes.
- [ ] All TODO items are checked or explicitly marked blocked with a reason.
