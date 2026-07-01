# Plan: Daemon Hardening and Release Gates

## Goal
Harden the local daemon control surface and release pipeline for `barrs` without changing the visible bar behavior. The work covers the audited changes: characterize IPC-triggered Lua handler behavior, remove the daemon's arbitrary config-validation IPC path, move the default socket away from a shared `/tmp/barrs.sock` path with safer cleanup, add a normal CI test gate, and guard manual release workflow dispatches so release jobs only run against version tags.

## Approach
The implementation should match the current compact Rust style: inline unit tests under `src/*.rs`, small helper functions near their call sites, and minimal workflow YAML. Start with tests for existing daemon event behavior because `TriggerItem` flows through `Daemon::dispatch_event` and `invoke_lua_handler` in `src/daemon.rs`; that gives a regression baseline before changing IPC.

For IPC hardening, remove `Request::ValidateConfig { path }` from `src/ipc.rs` and the matching daemon branch in `Daemon::handle_request`. The public CLI path in `src/app.rs` already validates configs locally via `config::load_config`, so this should not remove a documented CLI feature. Keep `TriggerItem`, `Reload`, `Status`, `DumpState`, and `RiftBackend` intact.

For socket hardening, replace the fixed default `/tmp/barrs.sock` with a per-user temporary directory path, preferably based on `std::env::temp_dir().join("barrs.sock")`, because this project targets macOS and `TMPDIR` is user-scoped there. Update `Config::default` and `load_config` in `src/config.rs` to use the same `default_socket_path()` helper instead of a string constant. In `src/daemon.rs`, make `cleanup_socket` refuse to remove non-socket filesystem entries; use `std::os::unix::fs::FileTypeExt::is_socket()` on Unix before `fs::remove_file`. If a configured socket path points at a regular file, return an `InvalidConfig` or IO-style error instead of deleting it.

For CI, add a new workflow such as `.github/workflows/ci.yml` that runs on pull requests and pushes to `main`, checks out the repo, installs Rust stable, and runs `cargo test` plus `cargo check --all-targets`. Keep release automation in `.github/workflows/auto-release.yml` and `.github/workflows/release.yml` separate.

For manual release dispatch, update `.github/workflows/release.yml` so `workflow_dispatch` requires an explicit tag input, validates it against `v*`, checks out that tag, and uses that tag value for archive names and Homebrew formula generation. Push-triggered tag releases should continue using `github.ref_name`. This avoids treating `main` or another branch name as a release tag.

Out of scope: changing release tagging policy, publishing to crates.io, changing renderer behavior, adding authentication beyond local filesystem/socket hardening, changing Lua's capabilities for the user's own config file, or redesigning the CLI.

## Implementation Phases

### Phase 1: Characterize IPC-triggered Lua handlers
- Add daemon tests in `src/daemon.rs` that exercise `Request::TriggerItem` through `send_request`, not only direct `dispatch_event`.
- Use a temporary config file with a Lua handler that writes an observable marker under the test temp directory.
- Assert that a click event for a known item returns `Response::Ok` and executes the configured handler.
- Add a negative-path test for an unknown item or missing handler if it can be done without making the daemon test flaky.
- Run `cargo test daemon::tests`.
**Commit:** `test(daemon): cover ipc-triggered lua handlers`

### Phase 2: Harden daemon IPC and socket lifecycle
- Remove `ValidateConfig { path: PathBuf }` from `Request` in `src/ipc.rs` and remove the matching branch in `Daemon::handle_request` in `src/daemon.rs`.
- Confirm `Command::ValidateConfig` in `src/app.rs` still validates locally with `config::load_config` and does not use IPC.
- Change `default_socket_path()` in `src/ipc.rs` to return a per-user temp path, and update `Config::default` plus `load_config` in `src/config.rs` to use that helper.
- Update tests that assert the previous `/tmp/barrs.sock` value so they assert the new helper's behavior without depending on a hard-coded global path.
- Replace `cleanup_socket` in `src/daemon.rs` with a helper that removes only Unix socket files and refuses to remove regular files, directories, or symlinks.
- Add tests for the socket cleanup behavior, including a regular-file path that must not be deleted.
- Run `cargo test ipc::tests config::tests daemon::tests`.
**Commit:** `fix(daemon): harden local ipc socket handling`

### Phase 3: Add a normal CI gate
- Add `.github/workflows/ci.yml`.
- Trigger it on `pull_request` and pushes to `main`.
- Use `actions/checkout@v5`.
- Install or use the default Rust toolchain on the runner, then run `cargo check --all-targets` and `cargo test`.
- Keep CI read-only: no release credentials, no `contents: write`, and no formula updates.
- Run `cargo test` and `cargo check --all-targets` locally after adding the workflow.
**Commit:** `ci: add rust verification workflow`

### Phase 4: Guard manual release dispatch
- Add a `workflow_dispatch` input named `tag` to `.github/workflows/release.yml`.
- Introduce a job-level or step-level `TAG` value that resolves to `github.ref_name` for tag pushes and to `inputs.tag` for manual dispatch.
- Add an early validation step that fails unless `TAG` matches `v[0-9]*`.
- Ensure checkout, packaging archive names, release publication, and formula generation all use the resolved `TAG`, not an unvalidated branch ref.
- Preserve the existing tag-push behavior for `v*` tags.
- Validate workflow syntax by inspecting the final YAML and, if available, running a local YAML-aware checker; otherwise rely on GitHub Actions syntax review in PR.
**Commit:** `fix(release): require explicit tag for manual releases`

## Risks & Tradeoffs
- Moving the default socket path can break external scripts that hard-code `/tmp/barrs.sock`. Mitigate by documenting that `--socket` and `socket_path` remain supported override paths, and keep compatibility easy through explicit configuration.
- `std::env::temp_dir()` is per-user on macOS, which fits this project, but it is not universally private on every Unix. This is acceptable for the current macOS target; future Linux support should revisit runtime directory selection.
- Removing `Request::ValidateConfig` is an internal IPC protocol break. The documented `barrs validate-config --config ...` command remains local, so user-facing behavior should remain intact.
- Refusing to delete non-socket paths may expose existing bad configs that previously appeared to work. That is intentional; deleting arbitrary configured paths is the risky behavior.
- GitHub Actions expression syntax is easy to get subtly wrong. Keep the release workflow change small and prefer simple shell validation of the resolved tag.

## Open Questions
- Should the default socket path change be documented in `README.md`, or is it enough to keep `--socket` and `socket_path` as the documented escape hatch?
- Should the CI workflow include `cargo fmt --check` and Clippy now, or should this first gate stay limited to the existing verified commands, `cargo check --all-targets` and `cargo test`?
