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
