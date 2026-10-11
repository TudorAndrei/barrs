---
id: ADR-0020
kind: decision
title: Run Lua plugins and handlers on one executor thread
status: accepted
date: 2026-10-11
supersedes:
  - ADR-0004
governs:
  - src/config.rs
  - src/daemon.rs
  - src/lua_executor.rs
  - src/plugin.rs
---

## Decision

Configure Lua snapshot providers as a plugin kind,
`plugin = { kind = "lua", update = "fn" }`, not as a separate `provider` item
field. An item can bind only one plugin, so a provider/plugin conflict cannot
occur.

One executor thread per config generation evaluates the config and owns its
`Lua` runtime. The daemon sends it handler calls and update requests, and it
returns only serde values, so no Lua value crosses a thread boundary. All Lua
calls run on that thread in order. An instruction-count hook stops a handler
after 1 s and an update function after 5 s.

The daemon keeps at most one update per item queued or running. A due item that
is still in flight keeps its deadline and runs once more after the call ends.
An update must return a string or a table with a string `text`, at most 16 KiB
as JSON. A failure, invalid result, or timeout publishes the unavailable
placeholder. Reload starts a new executor before it replaces the old one, and
results from an older generation are discarded.

## Consequences

The bar and IPC stay responsive while an update function runs. A handler or an
update waits while the executor runs another call, for at most that call's time
limit. The hook counts Lua instructions, so it cannot stop a call that waits
inside one C function such as `io.popen`; that call holds the executor until it
returns. The design and its test matrix are in [[lua-snapshot-providers]].
