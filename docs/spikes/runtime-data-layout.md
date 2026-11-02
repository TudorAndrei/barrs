---
id: DOC-RUNTIME-DATA-LAYOUT
kind: doc
tags:
  - performance
  - runtime
---

# Renderer batches and indexed Lua state

## Scope and contracts

This change implements the three selected rows from
[[data-layout-measurements]]: three renderer updates, all renderer updates,
and the Lua state trace. The two renderer rows use the same batch API.
The platform remains macOS arm64 on a current-thread Tokio runtime. No
retained AppKit object crosses a thread boundary.

Inputs are completed snapshot groups and Lua requests for validated config
items. Outputs are one published scene per nonempty group and value-only Lua
completion messages. The configuration is stable within an epoch. Only a
successful reload replaces it. The supplied configuration has six items;
the benchmarks also use synthetic lists of 64 and 256 renderer items and
1024 Lua items. The active user configuration was not sampled.

The dominant avoidable work was repeated layout and scene publication within
one completion group, plus string hashing and allocation for Lua update
state. The implementation removes that work. It adds no cache or dependency.
Success requires equal final scenes, preserved hover and reload behavior,
failure recovery, at most one pending Lua repeat, and stale-epoch rejection
before indexed state access. Repeated release measurements check actual
production methods against the retained former operations.

## Renderer

`Renderer::render_items` accepts a slice. `render_item` forwards a one-element
slice to that batch path. `Box<dyn Renderer>` forwards the batch operation.
Renderer implementations now supply `render_items`; this is an internal Rust
interface change. There is no CLI, Lua config, or IPC format change.

The daemon collects successful scheduled snapshots and publishes the group
once. Failed sources retain their error logging and attempted refresh
interval. Rift consumer updates also publish once per group. A single Lua
completion or item event uses the same batch path. Reload still uses the
complete-state `reconcile` transaction from ADR-0010.

The native renderer replaces retained snapshots in place and arranges once,
then publishes one scene. Display geometry can still cause a further layout
pass when it changes. The update journal moves displaced snapshots into a
contiguous vector; it does not copy the complete old scene. On failure, the
renderer restores the previous height, layout, and snapshots. Reverse replay
handles repeated updates to one ID and removes newly added items. The bar
updates its shown snapshots only after the renderer succeeds. Empty groups
publish nothing. A single shown item uses a stack array at the bar boundary.
The rollback journal uses O(D) storage for D updates.

The AppKit host advances its saved scene and plan after commands succeed, so
an error does not mark a failed plan as applied. An OS error can still occur
after some native commands have run; the logical rollback is not an OS-wide
transaction. A retry uses the last accepted plan. Existing native window
ownership and reload behavior remain in place.

## Lua

One array entry per config item replaces the two string sets. Each entry is
idle, running, or running with one repeat request. A scheduled request starts
an idle item and skips a running one. An event requests one repeat while a
call runs. Completion either starts that repeat or returns the item to idle.
Further event requests coalesce into that same repeat.

The scheduler sends Lua item indices directly. The event boundary resolves an
ID once and sends its index with the handler call. Both handler and update
messages carry the index and epoch. The dispatch rejects an old update epoch
before it accesses the array. Old handler replies still return to their
caller, but cannot schedule an update in the new epoch.

The constructor and successful reload build a new idle array from the current
config. Invalid scheduled indices and non-Lua items are skipped. No index is
persisted across config epochs. Strings remain in external event payloads,
Lua contexts, and rendered snapshots where the public contract needs them.
The array costs O(N) small states for N config items and has direct indexed
access; no per-transition ID key allocation is needed.

`LuaDispatch::new` and `replace` now take the current config. `deliver` takes
the validated item index; `request_updates` takes indices from that config.
These Rust interface changes are used by the daemon and its tests.

## Verification

The batch behavior test uses the real native renderer and bar with a host that
can reject publication. It checks one accepted scene for the group, complete
logical recovery on error after a geometry change, duplicate-ID replay, new
item recovery, retry, shown-state consistency, and an empty group. Existing
layout, hover, and complete reload tests remain in place.

The Lua coalescing test now queues two event requests during one scheduled
update and checks that only one repeat runs. The existing stale-epoch test
runs both with a reused slot and a moved Lua item after reload. It also sends
invalid and non-Lua scheduled indices. The first Lua update, shared handler
state, failed-update placeholder, IPC replies, and reload behavior retain
their existing test owners.

## Measurement contract

The release test retains the former per-item renderer operation as the
reference. Its candidate now calls production `render_items`, including the
rollback journal. Both sides use the real layout, scene-plan, and mock host
code and alternate the same text values. Final surfaces and host plans must
match before and after timing. AppKit drawing and screen latency are excluded.

The Lua trace retains the former string-set operations. The candidate now
calls production `UpdateState` methods. The trace and sizes are unchanged.
Lua execution, channels, and whole dispatch latency are not measured. The
large state-storage ratio must not be read as a full Lua speedup.

This is a declared contract update from benchmark commit `8534bfb`: the
prototypes are replaced with production methods, and the old per-item renderer
path is retained explicitly. The unchanged workload, release profile, and
Rust 1.97.1 toolchain are used on both sides of each comparison in one binary.
The same-binary exception avoids a public benchmark API. Two process runs use
nine alternating rounds with 16 warmup operations. Reported values are
medians and ranges of round means, not request latency percentiles. The
machine has no CPU isolation. Peak memory and allocations are not measured.

```sh
RUSTUP_TOOLCHAIN=1.97.1 cargo test --release --locked --lib \
  measure_ -- --ignored --nocapture --test-threads=1
```

## Production measurements

Implementation commit: `fec3d5e`. Reference behavior: `e6ab398`.
Both process runs passed all five benchmark output checks. This report saves
only the renderer and Lua rows selected for implementation. The other
experiments still run as checks but make no new performance claim here.

Run 2 medians, in microseconds per operation:

| Items | Operation | Previous | Implemented | Factor |
| ---: | --- | ---: | ---: | ---: |
| 6 | Renderer: one update | 2.441 | 2.317 | 1.05x |
| 6 | Renderer: three updates | 7.433 | 2.937 | 2.53x |
| 6 | Renderer: all updates | 14.717 | 3.795 | 3.88x |
| 6 | Lua: state trace only | 1.558 | 0.020 | 78.33x |
| 64 | Renderer: one update | 28.271 | 26.993 | 1.05x |
| 64 | Renderer: three updates | 85.542 | 27.733 | 3.08x |
| 64 | Renderer: all updates | 1807.337 | 50.129 | 36.05x |
| 64 | Lua: state trace only | 18.389 | 0.189 | 97.46x |

The [interactive comparison](runtime-data-layout-results.html) shows ranges
and both process runs. The [raw samples](runtime-data-layout-results.json)
include every recorded round mean for the selected operations and sizes.

At 64 items, run 1 measured 84.23 vs 27.32 microseconds for three updates,
1819.06 vs 50.94 for all updates, and 19.55 vs 0.189 for the Lua state trace.
Run 2 retained these gains. At six items, three updates saved about
4.5 microseconds and all six saved about 10.9 in run 2. The one-item renderer
case was about two percent slower at a list size of one and lower at the
other tested sizes. This is within the three-percent comparison limit; no
material single-update regression was observed across the two process runs.

The production renderer differs from the earlier prototype: it replaces
retained snapshots in place and keeps displaced snapshots for rollback.
The production Lua measurement includes the actual state transition methods.
Use these new results for the shipped implementation. The earlier prototype
numbers remain a historical record, not a claim about this implementation.

These factors apply only to the named operation. The Lua ratio does not
measure Lua execution or full dispatch. The renderer ratio does not measure
AppKit drawing or screen latency. There is no claim about whole-daemon speed,
energy use, peak memory, or allocator counts.

Final checks passed: 114 unit tests, 23 Cucumber scenarios, both manual
benchmark runs, formatting, strict Clippy for all targets and features, the
locked all-target build check, Rust documentation, and criv validation and
CI enforcement. Six manual timing tests are skipped in the normal suite.
