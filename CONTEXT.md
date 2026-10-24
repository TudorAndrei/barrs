# Context

Domain glossary for barrs.

**Item source**: the typed origin of one bar item's data (`ItemSource`): a
system plugin, a Rift plugin, a Lua update function, or a label source. The
config loader converts it once from the item's `plugin` table. _Avoid_:
provider, plugin binding (the binding is only the config schema).

**System plugin**: a built-in item source that queries the local machine
(CPU, time, date, battery, GPU) in the daemon process. _Avoid_: builtin
plugin (Rift plugins are built in too).

**Rift plugin**: a built-in item source that reads the Rift window manager
snapshot (workspaces or layout). _Avoid_: Rift item.

**Label source**: an item source with no plugin; the item shows its label, or
its id, and its icon. _Avoid_: static item, plain item.

**Item content**: what one bar item shows (`ItemContent`): text with optional
data fields, the Rift workspaces, or unavailable. Item sources produce it and
the renderer draws it; `dump-state` shows its JSON form as the item data.
_Avoid_: snapshot data, payload.

**Unavailable content**: item content for a source that cannot report a value
now; it shows the placeholder text and has `available = false` in its data.
_Avoid_: error item, empty item.

**Rift tracker**: the daemon's cache of the latest Rift snapshot
(`RiftTracker`). It applies Rift events, asks for a fresh snapshot when an
event cannot be applied exactly, and debounces renders of the Rift consumers.
_Avoid_: Rift cache, Rift state.

**Rift consumer**: a configured item whose source is a Rift plugin. _Avoid_:
Rift item.

**Published signature**: the signature of the Rift snapshot that every Rift
consumer shows now. A snapshot with another signature needs a render.
_Avoid_: last signature.

**Snapshot producer**: the daemon's module (`SnapshotProducer`) that turns
item sources into renderer snapshots on blocking tasks, reads Rift through the
injected Rift backend, and keeps the Rift tracker current. _Avoid_: refresher,
fetcher.

**Staged reload**: a loaded config and Lua runtime that wait for their
complete snapshot set before they replace the current ones. _Avoid_: pending
config.
