# Lua snapshot providers: design decision

## Decision

Do not add providers to the current daemon loop. If this direction is approved,
run all Lua work through one dedicated, current-thread Lua executor and exchange
only serializable request/result values with the daemon. The daemon remains the
owner of scheduling, item state, and rendering; the executor remains the owner
of the one loaded Lua runtime and its persistent Lua globals.

This is a design decision, not an implemented API. It is conditional on proving
that the chosen `mlua` version can enforce a cooperative instruction/time budget
and that the executor can be replaced transactionally on reload. If either is
not demonstrated by a prototype, this direction is **no-go** rather than a
reason to run providers on the renderer/daemon thread.

## User stories

- A local user exposes a small status value (for example, a project name or a
  local service state) without adding a Rust built-in plugin.
- A provider retains ordinary Lua state between scheduled refreshes and handler
  events, then starts with fresh state after a successful reload.
- A slow or failing provider presents a stable unavailable item without blocking
  pointer handling, AppKit presentation, IPC framing, or unrelated refreshes.

## Candidate configuration API

`provider` is a **new** item field. It names a global Lua function; `interval`
already exists and controls its scheduled refresh cadence. A provider item must
not also set the existing `plugin` field in the first release.

```lua
function project_status(ctx)
  return {
    text = "api: ready",
    icon = "✓",
    branch = "main",
    pending_jobs = 2,
  }
end

return {
  items = {
    {
      id = "project-status",
      provider = "project_status", -- new
      interval = 5,                 -- existing seconds field
    },
  },
}
```

The successful return must be a table that serializes to a JSON object with a
string `text`. `icon` is an optional string already understood by render
snapshots. Additional JSON-compatible fields are structured provider data and
are preserved in the render snapshot; their individual names are user-defined.
`available` is optional and follows the existing built-in placeholder
convention. `provider`, the provider context, and a redacted error field are
new concepts; `id`, `interval`, `text`, `icon`, and structured data map to
existing configuration/snapshot concepts.

The proposed provider context is deliberately small and value-only:

```lua
{ item_id = "project-status", timestamp_ms = 0 }
```

It does not contain AppKit, daemon, renderer, socket, or Rust object handles.
The existing event-handler context stays unchanged. Providers and event
handlers intentionally share Lua globals because both are requests to the same
configured Lua program.

## Execution models considered

| Model | Result | Reason |
|---|---|---|
| Call Lua synchronously from the daemon/main thread with a strict budget | Rejected | Even a budget breach or a non-cooperative Lua call stalls the serialized daemon loop that drives renderer events. It also risks AppKit-visible latency at the 16 ms interaction target. A wall-clock timeout cannot safely interrupt arbitrary synchronous Rust/Lua execution. |
| Dedicated current-thread Lua executor with value-only requests/results | Recommended, contingent on the go criteria below | It keeps AppKit and daemon state out of Lua work, preserves a single owned Lua runtime, serializes stateful Lua calls, and lets the daemon continue scheduling/rendering while a provider is running. |

The executor owns `Lua` for its whole generation. The daemon sends
`RunProvider`, `RunHandler`, and `Reload` messages and receives serde values or
structured failures; it never shares `Lua`, functions, or tables across the
channel. This is compatible with the current one-runtime-per-config-load model
and avoids relying on `Send` merely because the crate enables it. The executor
itself must be a current-thread task/runtime, with an explicit test that no Lua
or AppKit type crosses the request/result boundary.

Cancellation is cooperative only. A timeout means “do not wait or publish this
result”; it must not pretend to kill a running Lua call. Before implementation,
a prototype must prove instruction-budget hooks can stop an over-budget call
and leave the runtime usable, or must prove that a timed-out executor can be
discarded and replaced without leaking work. Without one of those properties,
an infinite provider can wedge the single executor and this proposal is no-go.

## Refresh state machine

```text
Idle --due--> Queued --executor starts--> Running
  ^             |                         |
  |             | provider already        | valid object
  |             | queued/running          v
  |             +---- coalesce -------- Published --> Idle
  |                                       |
  +-- placeholder <--- error/invalid/timeout
```

- At most one provider execution is queued or running per item.
- A due tick while that item is queued/running is coalesced into one later
  refresh, rather than adding overlapping calls.
- The daemon advances the attempt deadline on every terminal result to prevent a
  tight retry loop, consistent with existing refresh behavior.
- A valid result atomically replaces that item's snapshot and is rendered.
- Invalid serialization, a Lua exception, timeout, or executor rejection
  publishes `{ text = "—", available = false }`. A user-facing error field, if
  adopted, must be bounded and redacted; it must never include arbitrary Lua
  values, paths, or command output by default.
- Reload builds a new config and new Lua executor generation first. On success,
  the daemon swaps generations, clears provider in-flight/coalesced state, and
  reconciles snapshots. On failure, the old config/runtime/snapshots remain
  live. Provider and handler globals therefore persist within a generation and
  reset exactly once after a successful reload.

## Trust, security, and compatibility

Lua configuration is already a trusted local-code capability. Providers do not
make untrusted input safe and must not gain privileges beyond the `barrs`
process. The implementation must not add shell interpolation, network access,
or implicit filesystem paths. Returned values are data, not Lua code; serialize
them with the existing serde boundary and reject non-finite numbers, unsupported
Lua types, cyclic tables, and a configured maximum payload size.

Existing `plugin`, static-label, hover, and handler items retain their exact
behavior. The first provider release rejects `plugin` plus `provider` on the
same item rather than choosing precedence. The feature is opt-in, documented as
experimental, and ships with no changed default configuration. Roll out behind
config parsing/validation first, then a disabled-by-default executor path, then
the public field after the test matrix passes.

## Future implementation slices

1. **Config and schema** — add and validate `ItemConfig.provider` in
   `src/config.rs`; reject plugin/provider conflicts and missing globals.
   Tests: parsing, validation, conflict, and provider return-shape fixtures.
2. **Value contract** — add a small provider request/result module beside
   `src/plugin.rs` or `src/daemon.rs`; convert Lua tables to bounded
   `serde_json::Value` and create the unavailable placeholder. Tests: required
   `text`, optional `icon`, extra structured data, unsupported values, oversized
   values, and redaction.
3. **Executor and scheduling** — introduce a current-thread Lua executor and
   value-only channel in `src/daemon.rs`. Tests: interval scheduling, no
   overlapping refreshes, coalescing, a slow provider that does not delay
   renderer/IPC work, timeout/budget behavior, and executor failure.
4. **Lifecycle** — make reload create/swap executor generations transactionally
   with the existing reconciliation path. Tests: Lua state persists before
   reload, resets after reload, failed reload keeps the old provider, and
   in-flight old-generation results are discarded.
5. **Release review** — document the feature, add an opt-in sample, and run the
   full quality gate plus a macOS manual interaction smoke test.

## Go / no-go checklist

Proceed only when all are demonstrated in a prototype and tests:

- an over-budget or infinite provider cannot permanently block future Lua work;
- no AppKit or `Lua` value crosses a worker boundary;
- the value-only result is bounded and validated before rendering;
- provider overlap, timeout, error placeholder, state persistence, and reload
  transition tests pass; and
- the UI/IPC responsiveness test holds while a provider is slow.

Otherwise retain the current built-in plugin and Lua-handler model and close
this direction without implementation.
