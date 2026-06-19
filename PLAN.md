# Plan: Notch-Aware Bar Sections

## Goal
Add first-class `left`, `middle`/`center`, and `right` bar sections to `barrs` so Lua-configured items can be placed interchangeably in any section, while the native macOS renderer lays out the bar with awareness of the display notch area using the same core model SketchyBar uses. The result should keep existing configs working, but make placement meaningful instead of only affecting sort order.

## Approach
The current config already carries `ItemConfig::placement: Option<String>` through `RenderItemSnapshot`, and `NativeSurfaceState::update_snapshot` sorts by `placement_rank`, but `NativeSurfaceState::relayout` still packs every item from the left edge. `host_scene_plan` then computes a content-width window, while `AppKitHost::configure_window` anchors that frame to the main screen width via `anchor_bar_frame`. The implementation should make section layout explicit before AppKit presentation: normalize item placement into a small internal section enum, group rendered items into left, middle/center, and right sections, and compute frames from a screen/layout geometry value rather than from a single left-to-right cursor.

Research through DeepWiki and Context7 shows SketchyBar's notch behavior is mostly explicit configuration plus a built-in-display guard. In `FelixKratz/SketchyBar`, `workspace_display_notch_height` checks `CGDisplayIsBuiltin` and, on macOS 12+, reads `NSScreen.safeAreaInsets.top`. `bar_manager_init` defaults `notch_width` to `200`, `notch_offset` to `0`, and `notch_display_height` to `0`. `bar_calculate_bounds_top_bottom` uses `notch_width` only for horizontal bars and only for `POSITION_CENTER_LEFT` and `POSITION_CENTER_RIGHT`: center-left items start at `(bar_width - notch_width) / 2` and flow left, while center-right items start at `(bar_width + notch_width) / 2` and flow right. Plain center items remain centered over the full bar. `bar_get_frame` applies `notch_offset` and optionally overrides the bar height with `notch_display_height` on built-in displays.

The renderer should keep the Lua-facing field named `placement` for compatibility and accept `left`, `center`, `middle`, and `right`, treating `center` and `middle` as the same section because the user asked for a middle section while the code already recognizes `center`. Missing placement should default to `left`; unsupported placement should be rejected by config validation with an item-specific error. Tests should cover ordering within each section, invalid placement handling, and the `middle` alias.

Notch awareness belongs in the macOS host where `NSScreen` and CoreGraphics display data are available. Extend `BarConfig` with SketchyBar-inspired notch controls: `notch_width` with a built-in-display default of `200`, `notch_offset` defaulting to `0`, and `notch_display_height` defaulting to `0`. The implementation should also detect notch height from `NSScreen.safeAreaInsets.top` on built-in displays when the current AppKit bindings allow it, using that for top anchoring/menu-bar-height awareness rather than for the horizontal reserved gap. If `safeAreaInsets` is not directly exposed through `objc2-app-kit`, isolate the fallback in a helper and keep the configurable `notch_width` path fully functional.

The layout model should produce a full-width `HostScenePlan.window`, keep left items flowing from the left safe inset, right items flowing from the right safe inset inward, and middle/center items centered when no built-in-display notch gap is active. When a notch gap is active, split the middle/center section into two deterministic streams around the reserved gap, mirroring SketchyBar's `center_left`/`center_right` idea internally without adding those as required user-facing sections. A simple rule is acceptable: place middle items in config order, balance cumulative width across the left and right sides of the gap, and keep both sides outside `[(bar_width - notch_width) / 2, (bar_width + notch_width) / 2]`. Hover hit-testing should continue to use `ItemFrame::contains`, so keeping item frames in the same coordinate space is important.

Documentation updates should explain `placement = "left"`, `placement = "middle"`/`"center"`, and `placement = "right"` in `README.md` and demonstrate all three sections in `barrs.lua`. This is out of scope for SketchyBar compatibility generally: the change should not add SketchyBar config parsing, item scripting compatibility, or multi-display support unless already needed to implement main-screen notch awareness.

## Implementation Phases

### Phase 1: Section and Notch Config Model
- Add an internal section/placement parser in `src/render.rs` or `src/config.rs` that maps `left`, `center`, `middle`, and `right` to a typed section value while preserving `ItemConfig::placement: Option<String>` for Lua compatibility.
- Update `validate_config` in `src/config.rs` to reject unsupported placement strings with an item-specific error message.
- Extend `BarConfig` in `src/config.rs` with `notch_width`, `notch_offset`, and `notch_display_height` fields using SketchyBar-compatible defaults where applicable.
- Replace `placement_rank` usage in `NativeSurfaceState::update_snapshot` with the new parser while preserving per-section item order from `RenderItemSnapshot::order`.
- Add unit tests in `src/config.rs` and `src/render.rs` for valid placements, the `middle` alias, invalid placement errors, notch config defaults, and stable order within sections.
**Commit:** `feat(config): model bar sections and notch settings`

### Phase 2: Section-Aware Layout Geometry
- Introduce a renderer-side layout geometry type in `src/render.rs` that includes bar width, safe left/right insets, item spacing, and an optional center reserved range derived from `notch_width`.
- Update `NativeSurfaceState` so `relayout` positions left, center/middle, and right sections independently instead of using one left-to-right cursor.
- Implement the notch-active middle behavior as an internal split around the reserved center gap, inspired by SketchyBar's `POSITION_CENTER_LEFT` and `POSITION_CENTER_RIGHT`.
- Update `BarScene`, `HostScenePlan`, and `host_scene_plan` as needed so `HostScenePlan.window.width` represents the full layout width, not just content width.
- Extend `MockNativeHost` tests to verify left items start at the safe left inset, right items are right-aligned, center/middle items are centered without a notch, and center/middle items avoid the configured reserved range when a notch is active.
**Commit:** `feat(render): lay out independent bar sections`

### Phase 3: macOS Notch-Aware Host Metrics
- Extend the `NativeHost` trait in `src/render.rs` to provide layout metrics during initialization or before each presentation, with mock defaults for non-macOS tests.
- Update `AppKitHost` to derive full-width top bar geometry from `NSScreen::frame()` and `NSScreen::visibleFrame()` while preserving the existing top anchoring behavior in `anchor_bar_frame`.
- Add macOS-only built-in-display and notch-height detection modeled on SketchyBar's `CGDisplayIsBuiltin` plus `NSScreen.safeAreaInsets.top` approach when exposed by the current bindings.
- Apply `bar.notch_offset` and `bar.notch_display_height` to top bar anchoring/height on built-in displays in the same spirit as SketchyBar's `bar_get_frame`.
- Ensure `configure_window`, content view sizing, hover panel anchoring, and event hit-testing remain in the same window coordinate space after the window becomes full-width.
**Commit:** `feat(macos): avoid notch area in native bar layout`

### Phase 4: Docs, Sample Config, and Verification
- Update `README.md` configuration docs to describe the three sections, accepted placement strings, default behavior, `bar.notch_width`, `bar.notch_offset`, `bar.notch_display_height`, and notch-aware middle behavior.
- Update `barrs.lua` to demonstrate `left`, `middle`, and `right` placements with the existing Rift/workspace and system plugin items.
- Run formatting and tests with `cargo fmt`, `cargo test`, and `cargo run -- --version`, then manually smoke test on macOS with `cargo run -- start --config barrs.lua`.
- If implementation required any approach changes, update `PLAN.md` and `TODO.md` before committing the phase.
**Commit:** `docs(config): document notch-aware sections`

## Risks & Tradeoffs
- Exact `safeAreaInsets` access may not be available through the current AppKit bindings without enabling additional `objc2-app-kit` features or calling newer APIs. Mitigation: isolate detection behind helper functions and keep the configurable `notch_width` layout path independent of automatic height detection.
- Full-width windows can change event behavior compared with content-width windows. Mitigation: keep hit-testing item-frame based and verify clicks on empty bar space do not dispatch item events.
- Middle item distribution around the notch is less explicit than SketchyBar's separate `center_left` and `center_right` positions. Mitigation: keep a deterministic balancing rule now and consider exposing explicit `center_left`/`center_right` aliases later if needed.
- Existing configs may use arbitrary `placement` strings assuming they are ignored. Mitigation: invalid placement validation is stricter but produces clear errors; if compatibility becomes more important, this can be softened to warn/default in a follow-up.

## Open Questions
- Should invalid `placement` values be hard errors during `validate-config`, or should they default to `left` for maximum backward compatibility?
- Should the user-facing name be documented primarily as `middle` while keeping `center` as an alias, or should both be treated equally in docs?
- Should `barrs` expose explicit `center_left` and `center_right` placement aliases now, matching SketchyBar more closely, or keep only the requested three user-facing sections and split `middle` internally?
- Should notch-aware layout be main-screen only for this change, or should multi-display support be designed now as a later phase?
