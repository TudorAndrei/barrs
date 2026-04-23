# barrs

Native macOS status bar for Rift.

## Install

After a tagged release publishes `Formula/barrs.rb`, you can install `barrs` as a tap-backed formula from this repository:

```bash
brew tap TudorAndrei/barrs https://github.com/TudorAndrei/barrs
brew install barrs
```

If you do not want to tap the repository first:

```bash
brew install --formula https://raw.githubusercontent.com/TudorAndrei/barrs/main/Formula/barrs.rb
```

## Service

Start `barrs` as a proper `launchd` user service:

```bash
brew services start barrs
```

Useful commands:

```bash
brew services restart barrs
brew services stop barrs
barrs status
barrs reload
```

## Config

On first start, `barrs` creates its default config at:

```bash
~/.config/barrs/barrs.lua
```
