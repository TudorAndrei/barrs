---
id: SPIKE-DISPLAY-TARGETING
kind: doc
tags:
  - spike
  - display
  - renderer
---

# Display targeting: design decision

## Decision

Add one optional display selector for the existing single native bar. The
default remains `main`, preserving all existing configurations. Selection is
re-evaluated after the current CoreGraphics reconfiguration callback settles;
the host rebuilds its one window when the resolved target changes.

This is a design specification only. It does not add a config field, AppKit
code, or concurrent multi-bar ownership.

## Public identity model

`CGDirectDisplayID` is a runtime handle, not persisted configuration identity.
Use `NSScreen.CGDirectDisplayID()` only to associate the currently enumerated
`NSScreen` with CoreGraphics data and to detect a geometry/target change.

The proposed selectors use these public CoreGraphics APIs:

| Selector | Public API used to resolve it | Persistence policy |
|---|---|---|
| `main` | `CGMainDisplayID()` | Default; resolved afresh every time. |
| `builtin` | `CGDisplayIsBuiltin(display)` | Resolved afresh; no serial is stored. |
| `stable` | `CGDisplayVendorNumber(display)`, `CGDisplayModelNumber(display)`, and `CGDisplaySerialNumber(display)` | Persist the three-number fingerprint only when serial is nonzero. |

`CGDirectDisplayID` is documented as a `u32` display identifier, while the
vendor/model/serial functions report hardware properties for that identifier.
The Rust bindings expose these public APIs directly, including
[`CGDisplaySerialNumber`](https://docs.rs/objc2-core-graphics/latest/objc2_core_graphics/fn.CGDisplaySerialNumber.html)
and [`CGDirectDisplayID`](https://docs.rs/objc2-core-graphics/latest/objc2_core_graphics/type.CGDirectDisplayID.html).
The implementation must obtain equivalent public documentation for the vendor,
model, builtin, and mirror APIs before adding the feature.

There is no acceptable universal “stable display ID” assumption: adapters,
virtual displays, and displays reporting serial `0` cannot provide a unique
persistent fingerprint. A `stable` selector with a zero serial is a validation
error, not a best-effort match. Two configured displays with the same complete
fingerprint are also an error. This makes target identity unambiguous and avoids
silently picking one of two physically indistinguishable displays.

## Proposed Lua configuration

`bar.display` is new and optional:

```lua
bar = {
  display = "main", -- default; also accepts "builtin"
}

-- A persisted hardware target captured from public CGDisplay properties:
bar = {
  display = { vendor = 1552, model = 41001, serial = 123456 },
}
```

The object form is the only stable selector in the first release. It has no
optional fields and must contain positive integer `vendor`, `model`, and
`serial` values. Unknown strings, partial objects, zero serial, duplicate live
matches, and an object that has no current match produce a precise validation
error only when no configured fallback policy can resolve it (see below).
`main` is the backward-compatible default.

## Resolution and fallback

Resolve from the current `NSScreen::screens` list, pairing each screen with its
`CGDirectDisplayID`. Do not select by `NSScreen` array index.

1. `main`: choose the screen whose ID equals `CGMainDisplayID()`; if it is not
   present during a transient change, use `NSScreen::mainScreen`, then the
   lowest current display ID as a last-resort deterministic screen.
2. `builtin`: choose the sole screen where `CGDisplayIsBuiltin` is true. If
   none exists, fall back to `main` and emit a rate-limited diagnostic. If more
   than one appears, choose the lowest current display ID and report ambiguity.
3. `stable`: choose the sole complete vendor/model/nonzero-serial match. If it
   is absent, fall back to `main`, retain the preferred selector, and emit a
   diagnostic. The preferred target is selected automatically when it returns.

Use public `CGDisplayIsInHWMirrorSet` and `CGDisplayMirrorsDisplay` to mark a
mirror set during resolution. A selector that identifies a member resolves to
that member's current `NSScreen` when it exists; `main` remains the fallback
when it does not. Mirroring never creates a second bar. If macOS exposes only
one usable screen for a mirror set, that one screen is the target and the
diagnostic states that the physical member could not be distinguished.

## Reconfiguration transition table

| Event | Resolved target | Window action | Preferred selector state |
|---|---|---|---|
| Existing config starts | `main` | Create one bar on main | `main` |
| Builtin laptop screen present | builtin | Create/rebuild on builtin | `builtin` |
| Builtin selector on desktop | main fallback | Create/rebuild on main; diagnose | `builtin` retained |
| Stable target present | exact fingerprint match | Create/rebuild on match | `stable` retained |
| Stable target removed | main fallback | Invalidate old window, rebuild one on main after settle | `stable` retained |
| Stable target returns | exact fingerprint match | Rebuild one bar on returning target | `stable` retained |
| Main display changes | newly resolved main | Rebuild one bar | unchanged |
| Displays reordered | same resolved ID/fingerprint | No rebuild unless geometry changed | unchanged |
| Mode/scale/frame change | same target, new signature | Rebuild one bar | unchanged |
| Mirror begins/ends | selector re-resolved by rules above | Rebuild only if selected screen/geometry changes | unchanged |
| No usable screens during callback | none | Keep window hidden; retry existing settle schedule | unchanged |

The existing `CGDisplayRegisterReconfigurationCallback` invalidation, settle,
and bounded rebuild sequence remains the lifecycle trigger. Target resolution
must be pure over a list of public display descriptors so the callback itself
does no AppKit work. All window invalidation/rebuild work stays on the
`AppKitHost` main thread established in Plan 007.

## Validation, tests, and rollout

Implementation starts by extracting pure descriptor/selector code from
`src/render.rs`, then adding `bar.display` parsing and validation in
`src/config.rs`. Keep `ScreenSignature` runtime-only; it is not a persisted
identity. Exact tests:

- `src/config.rs`: absent selector defaults to `main`; string/object parsing;
  unknown selector; missing or zero stable fields.
- `src/render.rs`: main, builtin, exact stable match, no-builtin fallback,
  no-match fallback, duplicate fingerprint, reorder, and mirror descriptors.
- `src/render.rs`: target-removal/return and geometry-change transition tests
  drive the existing callback-settle state without AppKit.
- macOS manual matrix: built-in only; external only; builtin plus external;
  external as main; mirrored pair; selected external removal and return; display
  order change; resolution/scale change; desktop with no builtin; stable target
  unavailable at start; and config reload from `main` to each selector.

Roll out in three commits: parser + pure selection tests; main-thread host
integration/reconfiguration tests; documentation plus the manual matrix. Do
not add an automatic migration, private display APIs, or multi-bar support.

## Go / no-go criteria

Proceed only if public API testing shows that the vendor/model/nonzero-serial
tuple uniquely identifies the intended physical monitor across reconnect and
reboot on supported hardware, and if mirror/no-screen transitions preserve one
main-thread-owned window without stale layers. Otherwise ship only `main` and
`builtin` (or reject the stable form) and document the limitation. A future
multi-bar feature requires a separate ownership/state model and is explicitly
out of scope.
