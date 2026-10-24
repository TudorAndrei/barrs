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
reloads, renderer events, and the latest item snapshots. Each item has a typed
item source, `src/source.rs#enum:ItemSource`: a system plugin, a Rift plugin,
a Lua update function, or a fixed label. The daemon asks the source for its
default refresh interval, whether it reads a Rift snapshot, its Lua update
function, and its current JSON data; it does not match on plugin kinds. System
and Rift sources read their data through `src/plugin.rs`. The daemon
transforms that data into renderer snapshots through `src/render.rs`. Lua
updates run on the Lua executor. The native renderer is
macOS/AppKit-specific and is intentionally current-thread only; the noop
renderer supports tests and non-visual operation.

The daemon directly owns its state. Its event loop processes stateful requests
and results in order. Connection tasks parse requests, the blocking snapshot
task reads owned inputs, and `src/lua_executor.rs` owns Lua on a separate
thread. These tasks return values through messages. They do not share mutable
daemon state or retained AppKit objects.

## Configuration and interaction

`src/config.rs` evaluates the trusted Lua configuration once, deserializes it
into a schema document, and converts each item's `plugin` table into an item
source with `src/source.rs#fn:from_config`. That conversion rejects a Lua
plugin without `update`, `update` on other kinds, and a date format with NUL.
It then checks item IDs and placement, and confirms every configured handler
and update function name exists in the Lua globals. Items may use built-in
system plugins, Rift plugins, labels, hover text, and event handlers.

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
- Item sources and their refresh policy: `src/source.rs`
- Built-in system and Rift snapshot data: `src/plugin.rs`
- Rendering model and native AppKit host: `src/render.rs`
- Rift backends and event application: `src/rift.rs`
- Timed subprocess helper: `src/process.rs`
- Shared application errors: `src/error.rs`
