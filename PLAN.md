# Plan: Simplify daemon ownership and built-in snapshots

## Goal

Implement suggestions 1 and 2 from the architecture review at commit `1249b1c`.
Give the daemon direct ownership of its state. Replace temporary built-in
plugin objects with snapshot values. Reduce locks, copies, and caller steps
while preserving command-line, IPC, Lua, rendering, and scheduling behavior.

## Approach

Use two implementation phases. Each phase must pass its checks and have its
own commit before the next phase starts. Track work in [TODO.md](TODO.md).
The older program in `plans/PLAN.md` and `plans/TODO.md` remains historical.

### Direct daemon ownership

In `src/daemon.rs`, change `Daemon.state` from
`Arc<Mutex<DaemonState<R>>>` to `DaemonState<R>`. Keep `DaemonState` as the
existing private state module. The daemon loop already processes stateful
requests and results in order. Connection tasks, the blocking snapshot task,
and the Lua executor exchange values; they do not receive the state handle.

Use mutable receivers for methods that change state. Remove private async
methods whose only waits acquire state locks. Keep asynchronous waits in
`run`, `handle_request`, `process_renderer_events`, `dispatch_event`, and
`invoke_lua_handler` where they wait for I/O or Lua replies.

Change `request_lua_updates` to use the directly owned state instead of taking
both `&self` and a separate mutable state argument. Borrow distinct fields
when a method needs configuration and mutable deadlines together. Remove
copies used only to release a lock. Keep owned inputs that the blocking task
needs after the daemon resumes.

Keep the current worker message seam and renderer adapter seam. Keep the
constructor's public result type. Do not replace the mutex with interior
mutability, add `unsafe`, or add a `Send` requirement to `Renderer`.

### Direct built-in snapshot values

Keep selection in `src/plugin.rs` and use the existing `PluginKind` match.
Change the existing `from_item_config` interface to return snapshot data:

```rust
pub fn from_item_config(
    item: &ItemConfig,
    rift: Option<&RiftSnapshot>,
) -> Result<Option<Value>, BarrsError>
```

Return `Ok(None)` for an item without a plugin and for `PluginKind::Lua`.
Return `Ok(Some(data))` for a built-in snapshot, including unavailable data.
Propagate the existing time and date formatting errors. These outcomes keep
absence distinct from data-source failure.

Remove `Plugin`, its default `handle_event`, and the seven temporary object
types: `CpuPlugin`, `TimePlugin`, `DatePlugin`, `BatteryPlugin`, `GpuPlugin`,
`RiftWorkspacesPlugin`, and `RiftLayoutPlugin`. Keep the existing platform
query and parsing functions. Format Rift data from the borrowed snapshot.
Retain the current JSON fields and date-format fallback.

Update both production callers in `src/daemon.rs`: `snapshot_for_item` and
`dispatch_event`. Keep the Lua placeholder branch and label fallback in
`snapshot_for_item`. In `dispatch_event`, keep handler execution before
renderer event handling and any built-in refresh. Keep hover events excluded
from built-in refresh. A non-hover Lua event must still mark its item due.

This increases depth in the plugin module by reducing the caller interface.
It improves locality for snapshot selection and gives both daemon paths
leverage from the same implementation. It adds no adapter or module.

### Compatibility and scope

The Rust plugin types and `from_item_config` are public through `src/lib.rs`.
Removing them and changing the return type breaks Rust source compatibility.
The plan includes that change and marks phase 2 with a breaking conventional
commit. Existing documentation describes command-line and Lua use, but that
does not prove that external Rust callers do not exist. Review this explicit
tradeoff before implementation. Do not retain unused compatibility objects.

Preserve these recorded requirements:

- ADR-0006: deterministic scheduler and Rift transition checks.
- ADR-0008 and ADR-0011: serialized daemon work and main-thread AppKit access.
- ADR-0010 and ADR-0012: reload reconciliation and one Lua load per generation.
- ADR-0013 through ADR-0016: Rift completion, hover behavior, exact counts,
  and validated item targets.
- ADR-0018 and ADR-0020: Lua handler behavior and executor ownership.

No new dependency is needed. Refresh schedule extraction from suggestion 3,
renderer changes, new concurrency, timeout changes, and release execution are
out of scope. Keep existing failure ordering; report any separate defect
instead of changing it as part of this refactor.

## Implementation phases

### Phase 1: Give the daemon direct state ownership

- Recheck `src/daemon.rs` for any new external state owner before editing.
  Record the result of the existing `mise run check` baseline.
- Change `Daemon.state` and its construction in `new_loaded`. Remove the
  production `Arc` and `Mutex` imports if no production use remains. Keep
  test imports needed for shared renderer counters and failure controls.
- Update state reads and writes in request handling, reload, refresh,
  event dispatch, and Rift processing. Update `request_lua_updates` as
  described above. Keep field borrows short and retain required worker copies.
- Make `reload`, `refresh_all_items`, `refresh_due_items`, `apply_update`,
  `apply_refresh`, `process_rift_events`, and
  `refresh_selected_items_with_rift` synchronous when no real wait remains.
  Update their callers in `run`, other methods, and inline tests.
- Preserve reload preparation before renderer reconciliation and the state
  commit. Preserve generation checks before accepting results, deadline
  advancement, Lua active-item tracking, and stop-reply draining.
- Adapt inline daemon tests to mutable daemon bindings and direct state
  access. Preserve their behavior assertions. Do not add tests that assert
  that lock types or removed code are absent.
- Run `mise exec -- cargo test --locked daemon::tests`, then `mise run check`.
  Check the named behavior cases in `TODO.md`.
- Update the ownership description in `docs/architecture.md`. Run the criv
  checks below. Update both planning files if the approach changes.
- Check off completed phase tasks and commit the phase. Mark its commit
  checkbox only after the commit succeeds.

**Commit:** `refactor(daemon): own daemon state directly`

### Phase 2: Return built-in snapshot data directly

- Change `from_item_config` to the result contract above. Move snapshot
  behavior into its selection path and private implementation as needed.
  Keep the existing query functions, parsing functions, time formatting,
  timeout constants, and unavailable-data format.
- Remove the seven plugin types, the `Plugin` trait, and the no-op plugin
  event call. Use borrowed Rift data and date-format strings where possible.
- Update `snapshot_for_item` and `dispatch_event` to consume values. Preserve
  the separate Lua executor path, label fallback, and event ordering.
- Adapt inline plugin tests to `from_item_config`. Preserve output checks,
  parser checks, date formats, unavailable-data checks, and exact Rift counts.
  Retain valid absence checks where they describe current snapshot behavior.
- Review existing coverage of click-triggered built-in refresh and snapshot
  errors. Extend an existing test only if it leaves a specific behavior
  unprotected. Do not test removed symbol names or the old object structure.
- Update `docs/architecture.md` to describe direct built-in snapshots.
  Document the changed Rust interface in the phase commit body, including a
  `BREAKING CHANGE` footer. Keep `src/lib.rs` otherwise unchanged.
- Run `mise exec -- cargo test --locked plugin::tests` and
  `mise exec -- cargo test --locked daemon::tests`, then `mise run check`.
  Complete the smoke checks and documentation checks below.
- Check off completed phase tasks and commit the phase. Mark its commit
  checkbox only after the commit succeeds.

**Commit:** `refactor(plugin)!: return built-in snapshot data directly`

## Verification

Use the pinned toolchain and existing `mise` tasks. `mise run check` runs
formatting, Clippy, all-target compilation, and locked tests. Run it for each
phase after focused tests pass. Do not replace the build-cache configuration.

Preserve existing daemon tests for slow Lua updates, stop replies, silent
clients, invalid reloads, Lua state, complete renderer reconciliation, Rift
completion, and hover delivery. The detailed checklist is in `TODO.md`.
Tests may change their setup when ownership changes, but their observed
behavior must remain the same. Add no general test framework for this work.

Use an isolated temporary configuration and socket for a foreground noop
daemon. Include a time item, a formatted date item, a label item, and a Lua
update item with a click handler. Check ping, status, dump-state, click,
successful reload, failed reload, and stop. Verify that the normal daemon
and configuration are not affected. Use a test renderer for hover and
reconciliation checks; native display reconfiguration is outside this scope.

After documentation changes, run:

```sh
criv watch --once
criv check
criv enforce --stage ci
git diff --check
```

Do not change Cargo versions or create release tags. A later release must use
the reviewed `mise run release-plan` and `mise run release-auto` flow.

## Risks and tradeoffs

- Mutable borrowing can expose dependencies hidden by the mutex. Use distinct
  field borrows and short scopes. Do not restore shared state to satisfy the
  compiler.
- Worker input copies have a real lifetime purpose. Removing them can prevent
  work from leaving the daemon thread. Keep the value-only worker model.
- Reload and result handling depend on operation order. Keep the current
  order for rendering, state updates, generation checks, and deadlines.
- The new plugin result combines absence and failure in one return type.
  Check both callers so they preserve Lua and label behavior.
- Public Rust callers need migration. The breaking commit makes this visible
  to release tooling; it does not authorize a release.
- Tests that use system commands can depend on local tools. Record failures
  separately from deterministic parser and state tests; do not weaken them.

## Open questions

The user approved implementation, including the stated Rust source
compatibility change. No design question remains open.

## Planning validation

The code and tests were inspected at `1249b1c`. No Rust code changed during
planning. The implementation baseline and smoke tests remain unchecked.

## Implementation record

Phase 1 uses the owned Rift cache directly in
`refresh_selected_items_with_rift`; its separate snapshot argument is no longer
needed. `refresh_due_items` now returns no value because it cannot fail.

The baseline passed all 121 tests. Phase 1 passed 34 focused daemon tests and
the complete `mise run check` with all 121 tests. The subagent review found no
change to operation order. The criv refresh, check, and CI enforcement passed.

Phase 1 was committed as `38b8b87`. Phase 2 passed 13 focused plugin tests,
35 focused daemon tests, and the complete quality check with all 123 tests.
The plugin subagent changed `src/plugin.rs`; a separate subagent reviewed both
phases and reported no findings.

The existing hover test now also checks click-triggered built-in refresh.
Two new tests check the real date-format error at the plugin interface and
its propagation through initial refresh and click dispatch.

The isolated noop smoke test passed ping, status, snapshot data, built-in and
Lua clicks, hover, successful reload, failed reload, stop reply, and socket
cleanup. It used a temporary config and socket. The full criv checks and
Markdown checks passed. No release was run.

Phase 2 was committed as `5a2482a`. Both phase commits succeeded before their
commit checkboxes were marked. The two changed Rust files have 163 fewer
production lines and 66 more test lines than the review baseline. All tasks
in `TODO.md` are complete.
