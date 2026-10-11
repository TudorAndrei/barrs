# TODO: Simplify daemon ownership and built-in snapshots

Implementation design: [PLAN.md](PLAN.md).

## Phase 1: Give the daemon direct state ownership

- [x] Record the `mise run check` baseline and any existing failures.
- [x] Confirm that background tasks do not acquire daemon state.
- [x] Store `DaemonState<R>` directly in `Daemon.state`.
- [x] Update `new_loaded`, state access, mutable receivers, and imports.
- [x] Update `request_lua_updates` to use directly owned state.
- [x] Remove private async methods that no longer perform a real wait.
- [x] Remove lock-only copies; retain owned worker inputs.
- [x] Preserve request order, reload commit order, result generation checks,
  deadlines, Lua active-item tracking, and stop-reply draining.
- [x] Adapt inline daemon tests without changing behavior assertions.
- [x] Pass `mise exec -- cargo test --locked daemon::tests`.
- [x] Pass `mise run check`.
- [x] Update daemon ownership in `docs/architecture.md`.
- [x] Pass `criv watch --once`, `criv check`, `criv enforce --stage ci`, and
  `git diff --check`.
- [x] Review the phase diff and update the plan if needed.
- [x] Commit: `refactor(daemon): own daemon state directly`

## Phase 2: Return built-in snapshot data directly

- [x] Change `from_item_config` to `Result<Option<Value>, BarrsError>`.
- [x] Preserve `None` for no plugin and Lua; preserve built-in data and errors.
- [x] Remove the `Plugin` trait, seven temporary plugin types, and no-op
  plugin event call.
- [x] Preserve platform commands, parser behavior, timeouts, and JSON fields.
- [x] Format Rift data from borrowed snapshots and preserve exact counts.
- [x] Update `snapshot_for_item`; preserve Lua placeholders and label fallback.
- [x] Update `dispatch_event`; preserve handler order, hover behavior,
  click-triggered built-in refresh, and Lua deadline changes.
- [x] Adapt existing plugin tests to the value-returning interface.
- [x] Check click-refresh and error coverage; add or extend a test only for a
  specific uncovered behavior. Add no removed-code absence tests.
- [x] Update built-in snapshot behavior in `docs/architecture.md`.
- [x] Record the public Rust interface change in the commit body with a
  `BREAKING CHANGE` footer.
- [x] Pass `mise exec -- cargo test --locked plugin::tests`.
- [x] Pass `mise exec -- cargo test --locked daemon::tests`.
- [x] Pass `mise run check`.
- [x] Complete the behavior and smoke checks below.
- [x] Pass `criv watch --once`, `criv check`, `criv enforce --stage ci`, and
  `git diff --check`.
- [x] Review the phase diff and update the plan if needed.
- [ ] Commit: `refactor(plugin)!: return built-in snapshot data directly`

## Verification

- [x] No behavior change in command-line responses, IPC data, Lua configuration,
  handler order, snapshot JSON, refresh timing, or renderer thread ownership.
- [x] `stop_reply_arrives_before_the_daemon_runtime_shuts_down` passes.
- [x] `slow_lua_update_does_not_block_ipc` and `daemon_survives_silent_client`
  pass.
- [x] `loaded_config_and_lua_runtime_are_evaluated_once_together`,
  `lua_handler_state_persists_between_events`, and
  `reload_resets_lua_handler_state` pass.
- [x] `daemon_survives_reload_with_broken_config`,
  `reload_reconciles_removed_and_renamed_renderer_items`, and
  `failed_renderer_reload_retains_the_previous_daemon_state` pass.
- [x] `scheduler_transitions_are_deterministic`,
  `in_flight_lua_updates_are_not_requested_again`, and existing interval tests
  pass. Review unchanged generation and completion ordering in both result paths.
- [x] `rift_debounce_clears_when_no_items_consume_rift` and
  `rift_debounce_clears_equal_signatures_and_allows_later_change` pass.
- [x] `daemon_refreshes_builtin_items_on_click_but_not_hover`,
  `daemon_delivers_unchanged_hover_updates_to_lua_handlers`, and existing
  synthetic-hover tests pass.
- [x] Plugin tests preserve CPU, battery, and GPU parsing; local time and date
  formats; unavailable data; and exact Rift workspace counts.
- [x] Foreground noop smoke uses a temporary config and socket with time,
  date, label, and Lua items. Ping, status, and dump-state report expected data.
- [x] Smoke click runs its Lua handler and produces the expected update.
- [x] Smoke reload replaces configured items; invalid reload retains prior
  working state. Stop returns its reply and removes the temporary socket.
- [x] Remove smoke artifacts and leave the normal daemon unchanged.
- [x] Review the Rust source compatibility change separately from runtime
  behavior. Do not change Cargo versions or create release tags.

## Review

- [x] Source review confirms direct ownership without replacement locks,
  interior mutability, new unsafe code, or renderer `Send` requirements.
- [x] The plugin change removes object management without moving platform
  logic into the daemon.
- [x] Tests protect behavior rather than removed names or structures.
- [x] `PLAN.md` and `TODO.md` reflect the implemented approach.
- [ ] Each phase commit succeeded before its commit checkbox was marked.
- [ ] All phase commits are scoped and use the exact drafted messages.
- [ ] All implementation tasks are checked after their work is complete.
