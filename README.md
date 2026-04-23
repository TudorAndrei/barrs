# barrs

`barrs` is a native macOS status bar for Rift.

It is built as a lightweight bar daemon with a Rust core, a native AppKit renderer, built-in system plugins, and direct Rift integration. The current focus is a practical top bar for daily use on macOS rather than SketchyBar compatibility.

## Features

- Native macOS renderer
- Tight Rift workspace and layout integration
- Built-in plugins for CPU, GPU, battery, and time
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
- built-in plugin bindings
- hover tooltips
- click and hover handlers

## Built-in plugins

- `cpu`
- `gpu`
- `battery`
- `time`
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

## Notes

- The bar is top-mounted only.
- Release installs are designed to run as a service.
- Development runs stay in the foreground.
- Rift-backed items use the direct Mach backend when available and fall back to the CLI backend otherwise.
