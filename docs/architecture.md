---
id: DOC-ARCHITECTURE
kind: doc
tags:
  - architecture
  - runtime
  - macos
---

# Barrs architecture

`barrs` is a single-process macOS status bar daemon. The public CLI parses a
command, loads Lua configuration when needed, and either starts the daemon or
sends it a Unix-socket request. The runtime keeps configuration, plugins,
snapshots, rendering, and Rift integration coordinated on the application's
current-thread Tokio runtime.

## Runtime flow

`src/main.rs#fn:main` starts the current-thread runtime and calls
`src/app.rs#fn:run`. `start` creates the default Lua configuration when
missing;
release builds then spawn the hidden `run` command and wait for its readiness
signal. `run` loads the Lua configuration, constructs a renderer, and gives both
to `src/daemon.rs`.

The daemon owns the Unix socket, scheduled plugin refreshes, configuration
reloads, renderer events, and the latest item snapshots. It turns each
configuration item into a plugin via `src/plugin.rs` and transforms plugin data
into renderer snapshots through `src/render.rs`. The native renderer is
macOS/AppKit-specific and is intentionally current-thread only; the noop
renderer supports tests and non-visual operation.

## Configuration and interaction

`src/config.rs` evaluates the trusted Lua configuration once, deserializes it
into the Rust model, checks item IDs and placement, and confirms every
configured handler name exists in the Lua globals. Items may use built-in system
plugins, Rift plugins, labels, hover text, and event handlers.

`src/ipc.rs` defines the framed request and response protocol used by commands
such as `stop`, `reload`, `status`, `dump-state`, and `item trigger`.
`src/app.rs` formats those responses for the command line. `src/error.rs`
provides the shared error vocabulary across configuration, IPC, daemon, and
platform paths.

## External data

`src/rift.rs` selects the Rift Mach backend when available, otherwise the Rift
CLI backend. It supplies workspace and layout snapshots and can subscribe to
Rift events so the daemon refreshes affected items. `src/process.rs` provides
bounded child-process execution for platform queries and the CLI fallback.

## Source map

- Entrypoint and module exports: `src/main.rs#fn:main` and `src/lib.rs`
- CLI commands and arguments: `src/cli.rs`
- Command dispatch and daemon startup: `src/app.rs`
- Lua configuration schema and validation: `src/config.rs`
- Daemon lifecycle, scheduler, and snapshots: `src/daemon.rs`
- Framed Unix-socket protocol: `src/ipc.rs`
- Plugin abstraction and built-in providers: `src/plugin.rs`
- Rendering model and native AppKit host: `src/render.rs`
- Rift backends and event application: `src/rift.rs`
- Timed subprocess helper: `src/process.rs`
- Shared application errors: `src/error.rs`
