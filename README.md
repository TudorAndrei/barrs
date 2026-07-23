# barrs

`barrs` is a native macOS status bar for Rift.

It is built as a lightweight bar daemon with a Rust core, a native AppKit renderer, built-in system plugins, and direct Rift integration. The current focus is a practical top bar for daily use on macOS rather than SketchyBar compatibility.

## Features

- Native macOS renderer
- Tight Rift workspace and layout integration
- Built-in plugins for CPU, GPU, battery, time, and date
- Hover tooltips and click handlers
- Lua configuration
- Homebrew installation and `launchd` service support

## Requirements

- macOS
- Rift, if you want workspace and layout items

## Installation

### Homebrew

This repository can be used as a tap. Because the repository is named `barrs` rather than `homebrew-barrs`, use the explicit URL form:

```bash
brew tap TudorAndrei/barrs https://github.com/TudorAndrei/barrs
brew install barrs
```

You can also install the formula directly:

```bash
brew install --formula https://raw.githubusercontent.com/TudorAndrei/barrs/main/Formula/barrs.rb
```

### From source

```bash
cargo build --release
```

The binary will be available at:

```bash
target/release/barrs
```

## Running

### Installed version

Start `barrs` as a user service:

```bash
brew services start barrs
```

Useful service commands:

```bash
brew services restart barrs
brew services stop barrs
```

You can still interact with the running daemon directly:

```bash
barrs status
barrs reload
barrs dump-state
barrs stop
```

### Development

For development, run it directly from the repository:

```bash
cargo run -- start --config barrs.lua
```

In debug builds, `start` stays attached to the terminal so you can iterate without a detached background process.

## Configuration

On first start, `barrs` creates a default config at:

```bash
~/.config/barrs/barrs.lua
```

You can also point it at an explicit config file:

```bash
barrs start --config /path/to/barrs.lua
```

The repository includes a sample config at [barrs.lua](./barrs.lua).

Configuration is written in Lua and currently covers:

- bar appearance
- global item spacing
- item order and placement
- icons and labels
- per-item refresh intervals with `interval`
- built-in plugin bindings
- hover tooltips
- click and hover handlers

Items can be placed in three sections with `placement`:

- `left`
- `middle` or `center`
- `right`

Omitting `placement` defaults to `left`. Left items flow from the left edge, right items flow inward from the right edge, and middle/center items are centered in the available bar space. On built-in Mac displays with a notch, middle items are split around the reserved notch gap so they do not render under the notch.

### Lua event handlers

Assign global Lua function names in an item's `handlers` table. Each slot is
called for its matching event:

| Handler slot | Event value |
|---|---|
| `click` | `click` |
| `right_click` | `right_click` |
| `scroll` | `scroll` |
| `hover_enter` | `hover_enter` |
| `hover_leave` | `hover_leave` |
| `hover_update` | `hover_update` |

Each function receives one `ctx` table:

```lua
{
  item_id = "time",
  event = "click",
  timestamp_ms = 0,
  mouse = { x = 0, y = 0, button = nil, scroll_delta = nil },
  modifiers = { shift = false, control = false, option = false, command = false },
}
```

`mouse.button` is present for pointer buttons, `mouse.scroll_delta` is present
for scroll events, and both are otherwise `nil`. Handler return values are
ignored. A configured handler name must exist when the config is loaded. If a
handler raises an error while processing an IPC request, the daemon returns an
error response and the CLI exits nonzero; errors from native renderer events
are logged by the daemon.

The bundled sample records the most recent time-item event in Lua state without
launching external programs:

```lua
function record_time_event(ctx)
  last_time_event = ctx.event
end
```

The bar also accepts SketchyBar-style notch settings:

```lua
bar = {
  spacing = 0,
  background = "#000000",
  notch_width = 200,
  notch_padding = 8,
  notch_offset = 0,
  notch_display_height = 0,
}
```

`notch_width` is the physical horizontal notch width used on notched built-in displays. `notch_padding` adds clearance on each side of that width (default: `8`), preventing icons from sitting flush against or clipping beneath the notch. `notch_offset` shifts the native top bar frame vertically on built-in displays, and `notch_display_height` overrides the bar window height when set to a value greater than `0`.

Built-in `time`, `date`, `cpu`, `gpu`, and `battery` items now refresh automatically with sane defaults. Set `interval` on an item to override that default. Example:

```lua
{
  id = "time",
  icon = "󰥔",
  placement = "middle",
  interval = 1,
  plugin = { kind = "time" },
}
```

The `date` plugin accepts a raw `strftime` format string. Omit `format` to use `%Y-%m-%d`:

```lua
{
  id = "date",
  icon = "󰃭",
  plugin = { kind = "date", format = "%Y-%m-%d" },
}
```

The bundled default config also sets explicit intervals for `cpu`, `gpu`, `battery`, `time`, and `date`.

## Built-in plugins

- `cpu`
- `gpu`
- `battery`
- `time`
- `date`
- `rift_workspaces`
- `rift_layout`

## Command overview

```bash
barrs start
barrs stop
barrs reload
barrs status
barrs ping
barrs validate-config --config /path/to/barrs.lua
barrs dump-state
barrs rift backend
barrs item trigger <item-id> <event>
```

## Releases

Releases are automated from conventional commits on `main`. Cocogitto derives the next SemVer version; the canonical local flow updates [CHANGELOG.md](./CHANGELOG.md) and Cargo metadata, then uses `cargo-release` to create the release commit and matching `v{{version}}` tag.

Install the local release tooling with `mise`:

```bash
mise install
```

Preview the next automatic version:

```bash
mise run release-plan
```

From a clean `main` working tree, execute the reviewed release:

```bash
mise run release-auto
```

This repository uses [release.toml](./release.toml) to:

- create tags as `v{{version}}`
- create a release commit before tagging
- push the branch and tag to `origin`
- skip `cargo publish`, since GitHub Actions handles packaging and the Homebrew formula update

After the tag is pushed, GitHub Actions builds the macOS archives, publishes the GitHub release assets, and updates `Formula/barrs.rb`.

`release-auto` invokes `cargo-release` internally. Do not run a separate manual
patch-release flow, hand-edit `Cargo.toml`, or create tags manually.

## Notes

- The bar is top-mounted only.
- Release installs are designed to run as a service.
- Development runs stay in the foreground.
- Rift-backed items use the direct Mach backend when available and fall back to the CLI backend otherwise.
