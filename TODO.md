# TODO: Notch-Aware Bar Sections

## Phase 1: Section and Notch Config Model
- [x] Add an internal parser for `left`, `center`, `middle`, and `right` item placements while keeping `ItemConfig::placement`.
- [x] Update `src/config.rs::validate_config` to reject unsupported placement strings with item-specific errors.
- [x] Extend `src/config.rs::BarConfig` with `notch_width`, `notch_offset`, and `notch_display_height` fields using SketchyBar-inspired defaults.
- [x] Replace `src/render.rs::placement_rank` sorting with section-aware parsing while preserving `RenderItemSnapshot::order` inside each section.
- [x] Add tests in `src/config.rs` and `src/render.rs` for valid placements, `middle` aliasing, invalid placement rejection, notch config defaults, and stable per-section ordering.
- [x] Commit: `feat(config): model bar sections and notch settings`

## Phase 2: Section-Aware Layout Geometry
- [x] Add a renderer layout geometry type in `src/render.rs` with full bar width, safe insets, spacing, and optional center reserved range derived from `notch_width`.
- [x] Update `NativeSurfaceState::relayout` to independently position left, center/middle, and right sections.
- [x] Split middle/center items around the reserved notch gap when a built-in-display notch range is active, modeled on SketchyBar's `center_left`/`center_right` streams.
- [x] Update `BarScene`, `HostScenePlan`, and `host_scene_plan` so the planned window uses full layout width rather than content width.
- [x] Extend `MockNativeHost`/renderer tests for safe left start, right alignment, centered middle placement without a notch, and reserved-gap avoidance with a notch.
- [x] Commit: `feat(render): lay out independent bar sections`

## Phase 3: macOS Notch-Aware Host Metrics
- [x] Extend `NativeHost` in `src/render.rs` so the renderer can obtain current layout metrics from the host.
- [x] Update `AppKitHost` to derive top bar width and y-position from `NSScreen::frame()` and `NSScreen::visibleFrame()`.
- [x] Implement built-in-display notch-height detection modeled on SketchyBar's `CGDisplayIsBuiltin` plus `NSScreen.safeAreaInsets.top` approach when available through current AppKit bindings.
- [x] Apply `bar.notch_offset` and `bar.notch_display_height` to built-in-display top bar frame calculations.
- [x] Verify `configure_window`, content view sizing, hover panel anchoring, and item hit-testing still use consistent window coordinates.
- [ ] Commit: `feat(macos): avoid notch area in native bar layout`

## Phase 4: Docs, Sample Config, and Verification
- [ ] Update `README.md` to document `placement = "left"`, `"middle"`/`"center"`, and `"right"` plus `bar.notch_width`, `bar.notch_offset`, `bar.notch_display_height`, and notch-aware middle behavior.
- [ ] Update `barrs.lua` to demonstrate existing items distributed across left, middle, and right sections.
- [ ] Run `cargo fmt`.
- [ ] Run `cargo test`.
- [ ] Run `cargo run -- --version` and confirm it prints the crate version intended for the current build.
- [ ] Manual smoke test: run `cargo run -- start --config barrs.lua`, verify items render in all three sections, hover/click behavior still works, and middle items do not overlap the notch/reserved center area.
- [ ] Commit: `docs(config): document notch-aware sections`

## Verification
- [ ] Existing `src/config.rs`, `src/render.rs`, `src/daemon.rs`, and `src/cli.rs` tests pass under `cargo test`.
- [ ] New unit tests cover section parsing, layout positions for all three sections, middle/center aliasing, invalid placement handling, notch config defaults, and reserved-gap avoidance.
- [ ] Manual macOS smoke test confirms left, middle/center, and right sections render from `barrs.lua`.
- [ ] Manual macOS smoke test confirms item hover panels and `barrs item trigger <item-id> hover-enter` behavior still target the correct item frames.
- [ ] Edge cases tested: no middle items, no right items, all items in middle, middle section wider than the combined left/right notch-adjacent lanes, disabled/zero notch width, and zero `bar.spacing`.
- [ ] No regression in existing Rift workspace rendering, plugin refresh intervals, and `barrs validate-config --config barrs.lua`.

## Review
- [ ] Code reviewed.
- [ ] PLAN.md updated if approach changed during implementation.
- [ ] All phase commits are clean and describe their intent.
- [ ] TODO.md items all checked off.
