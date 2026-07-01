# TODO: Daemon Hardening and Release Gates

## Phase 1: Characterize IPC-triggered Lua handlers
- [x] Add daemon IPC tests in `src/daemon.rs` that send `Request::TriggerItem` through `send_request`.
- [x] Use a temporary Lua config whose click handler writes an observable marker under the temp directory.
- [x] Assert the known-item click returns `Response::Ok` and executes the Lua handler.
- [x] Add a negative-path daemon event test if it can remain deterministic.
- [x] Run `cargo test daemon::tests`.
- [x] Commit: `test(daemon): cover ipc-triggered lua handlers`

## Phase 2: Harden daemon IPC and socket lifecycle
- [x] Remove `Request::ValidateConfig { path }` from `src/ipc.rs`.
- [x] Remove the `Request::ValidateConfig` branch from `Daemon::handle_request` in `src/daemon.rs`.
- [x] Confirm `Command::ValidateConfig` in `src/app.rs` still validates locally through `config::load_config`.
- [x] Change `default_socket_path()` in `src/ipc.rs` to use a per-user temp directory path.
- [x] Update `Config::default` and `load_config` in `src/config.rs` to use `default_socket_path()`.
- [x] Update tests that currently assume `/tmp/barrs.sock`.
- [x] Change `cleanup_socket` in `src/daemon.rs` so it removes only Unix socket files.
- [x] Add socket cleanup tests proving regular files are not deleted.
- [x] Run `cargo test ipc::tests`, `cargo test config::tests`, and `cargo test daemon::tests`.
- [x] Commit: `fix(daemon): harden local ipc socket handling`

## Phase 3: Add a normal CI gate
- [x] Add `.github/workflows/ci.yml`.
- [x] Configure CI for `pull_request` and pushes to `main`.
- [x] Use `actions/checkout@v5`.
- [x] Run `cargo check --all-targets` in CI.
- [x] Run `cargo test` in CI.
- [x] Keep the workflow without write permissions or release credentials.
- [x] Run `cargo test` locally.
- [x] Run `cargo check --all-targets` locally.
- [x] Commit: `ci: add rust verification workflow`

## Phase 4: Guard manual release dispatch
- [x] Add a required `tag` input for `workflow_dispatch` in `.github/workflows/release.yml`.
- [x] Resolve one `TAG` value for both tag-push and manual-dispatch runs.
- [x] Add an early validation step that rejects non-`v[0-9]*` tags.
- [x] Update packaging archive names to use the resolved `TAG`.
- [x] Update release publication and formula generation to use the resolved `TAG`.
- [x] Preserve existing `push.tags: "v*"` behavior.
- [x] Review final YAML syntax for GitHub Actions compatibility.
- [ ] Commit: `fix(release): require explicit tag for manual releases`

## Verification
- [ ] `cargo test` passes.
- [ ] `cargo check --all-targets` passes.
- [ ] New tests cover IPC `TriggerItem` executing a Lua handler through the daemon socket.
- [ ] New tests cover missing/unknown event handling if added in Phase 1.
- [ ] New tests cover default socket path behavior without hard-coding `/tmp/barrs.sock`.
- [ ] New tests cover socket cleanup refusing to delete a regular file.
- [ ] Manual smoke test: `cargo run -- validate-config --config barrs.lua` still validates locally.
- [ ] Manual smoke test: start the daemon with a temp `--socket`, run `ping`, `item trigger`, and `stop` against that socket.
- [ ] Release workflow review confirms manual dispatch cannot package a branch name as a release tag.
- [ ] No regressions in `reload`, `status`, `dump-state`, `rift backend`, or `item trigger` CLI behavior.

## Review
- [ ] Code reviewed.
- [ ] PLAN.md updated if approach changed during implementation.
- [ ] All phase commits are clean and describe their intent.
- [ ] TODO.md items all checked off.
