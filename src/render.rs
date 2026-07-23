use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::ffi::c_void;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{BarSection, Config, HoverConfig, ItemConfig};
use crate::error::BarrsError;
use crate::ipc::{EventKind, EventPayload, current_timestamp_ms};

#[cfg(target_os = "macos")]
use objc2::MainThreadOnly;
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSEvent,
    NSEventMask, NSEventModifierFlags, NSEventType, NSFont, NSMainMenuWindowLevel, NSPanel,
    NSScreen, NSStatusWindowLevel, NSTextField, NSView, NSWindow, NSWindowStyleMask,
};
#[cfg(target_os = "macos")]
use objc2_core_graphics::{
    CGDisplayBounds, CGDisplayChangeSummaryFlags, CGDisplayIsBuiltin,
    CGDisplayRegisterReconfigurationCallback, CGDisplayRemoveReconfigurationCallback, CGError,
    CGMainDisplayID,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{MainThreadMarker, NSDefaultRunLoopMode, NSPoint, NSRect, NSSize, NSString};

#[cfg(target_os = "macos")]
#[link(name = "SkyLight", kind = "framework")]
unsafe extern "C" {
    fn SLSMainConnectionID() -> u32;
    fn SLSSetWindowLevel(cid: u32, wid: u32, level: i32) -> i32;
}

#[cfg(target_os = "macos")]
const BACKSTOP_MENU_LEVEL: i32 = -20;

const ITEM_HORIZONTAL_PADDING: f64 = 6.0;
const CHARACTER_WIDTH: f64 = 9.5;
const ICON_WIDTH: f64 = 18.0;
const ICON_TEXT_SPACING: f64 = 6.0;
const ITEM_LABEL_FONT_SIZE: f64 = 14.0;
const ITEM_TEXT_HEIGHT: f64 = 18.0;
const ITEM_TRAILING_TEXT_PADDING: f64 = 0.0;
const BAR_HEIGHT: f64 = 28.0;
const DEFAULT_ITEM_SPACING: f64 = 6.0;
#[cfg(target_os = "macos")]
const DISPLAY_RECONFIGURATION_SETTLE_MS: u64 = 500;
#[cfg(target_os = "macos")]
const DISPLAY_RECONFIGURATION_RETRY_MS: u64 = 1000;
#[cfg(target_os = "macos")]
const DISPLAY_RECONFIGURATION_REBUILDS: u8 = 3;
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RendererKind {
    Native,
    Noop,
}

pub trait Renderer: Send + Sync {
    fn initialize(&mut self, _config: &Config) -> Result<(), BarrsError>;
    fn render_item(&mut self, snapshot: &RenderItemSnapshot) -> Result<(), BarrsError>;
    fn drain_events(&mut self) -> Result<Vec<EventPayload>, BarrsError> {
        Ok(Vec::new())
    }
    fn handle_event(&mut self, _event: &EventPayload) -> Result<(), BarrsError> {
        Ok(())
    }
}

impl<T: Renderer + ?Sized> Renderer for Box<T> {
    fn initialize(&mut self, config: &Config) -> Result<(), BarrsError> {
        (**self).initialize(config)
    }

    fn render_item(&mut self, snapshot: &RenderItemSnapshot) -> Result<(), BarrsError> {
        (**self).render_item(snapshot)
    }

    fn drain_events(&mut self) -> Result<Vec<EventPayload>, BarrsError> {
        (**self).drain_events()
    }

    fn handle_event(&mut self, event: &EventPayload) -> Result<(), BarrsError> {
        (**self).handle_event(event)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenderItemSnapshot {
    pub id: String,
    pub order: usize,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub placement: Option<String>,
    pub text: String,
    pub hover: Option<HoverSurface>,
    pub interactive: InteractiveSnapshot,
    pub data: Value,
}

impl RenderItemSnapshot {
    pub fn from_item_config(item: &ItemConfig, order: usize, data: Value) -> Self {
        let text = data
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| item.label.clone())
            .unwrap_or_else(|| item.id.clone());
        Self {
            id: item.id.clone(),
            order,
            label: item.label.clone(),
            icon: item.icon.clone(),
            placement: item.placement.clone(),
            text,
            hover: item.hover.as_ref().map(HoverSurface::from),
            interactive: InteractiveSnapshot::from_handlers(item),
            data,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HoverSurface {
    pub tooltip: Option<String>,
    pub detail: Option<String>,
}

impl From<&HoverConfig> for HoverSurface {
    fn from(value: &HoverConfig) -> Self {
        Self {
            tooltip: value.tooltip.clone(),
            detail: value.detail.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InteractiveSnapshot {
    pub click: bool,
    pub right_click: bool,
    pub scroll: bool,
    pub hover: bool,
}

impl InteractiveSnapshot {
    fn from_handlers(item: &ItemConfig) -> Self {
        Self {
            click: item.handlers.click.is_some(),
            right_click: item.handlers.right_click.is_some(),
            scroll: item.handlers.scroll.is_some(),
            hover: item.hover.is_some()
                || item.handlers.hover_enter.is_some()
                || item.handlers.hover_leave.is_some()
                || item.handlers.hover_update.is_some(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemFrame {
    pub x: f64,
    pub width: f64,
    pub height: f64,
}

impl ItemFrame {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.width && y >= 0.0 && y <= self.height
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PositionedItemSnapshot {
    pub snapshot: RenderItemSnapshot,
    pub frame: ItemFrame,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HoverPresentation {
    pub item_id: String,
    pub anchor_x: f64,
    pub anchor_y: f64,
    pub tooltip: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BarScene {
    pub bar_height: f64,
    pub bar_width: f64,
    pub items: Vec<PositionedItemSnapshot>,
    pub hover: Option<HoverPresentation>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CenterReservedRange {
    pub start: f64,
    pub end: f64,
}

impl CenterReservedRange {
    fn normalized(self, bar_width: f64) -> Option<Self> {
        let start = self.start.max(0.0).min(bar_width);
        let end = self.end.max(start).min(bar_width);
        if end > start {
            Some(Self { start, end })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct LayoutGeometry {
    pub bar_width: f64,
    pub safe_left: f64,
    pub safe_right: f64,
    pub center_reserved: Option<CenterReservedRange>,
}

impl Default for LayoutGeometry {
    fn default() -> Self {
        Self {
            bar_width: 0.0,
            safe_left: 0.0,
            safe_right: 0.0,
            center_reserved: None,
        }
    }
}

impl LayoutGeometry {
    fn usable_width(self) -> f64 {
        (self.bar_width - self.safe_left - self.safe_right).max(0.0)
    }

    fn normalized_reserved(self) -> Option<CenterReservedRange> {
        self.center_reserved
            .and_then(|range| range.normalized(self.bar_width))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextLayerPlan {
    pub value: String,
    pub x: f64,
    pub y: f64,
    pub tone: TextTone,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TextTone {
    Primary,
    Secondary,
    Tertiary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemLayerPlan {
    pub item_id: String,
    pub frame: ItemFrame,
    pub icon: Option<TextLayerPlan>,
    pub text_segments: Vec<TextLayerPlan>,
    pub hoverable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HoverPanelPlan {
    pub item_id: String,
    pub anchor_x: f64,
    pub anchor_y: f64,
    pub title: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostScenePlan {
    pub window: WindowFrame,
    pub item_layers: Vec<ItemLayerPlan>,
    pub hover_panel: Option<HoverPanelPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LayerMutation {
    pub item_id: String,
    pub layer: ItemLayerPlan,
    pub is_new: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HostCommand {
    ConfigureWindow(WindowFrame),
    UpsertItemLayer(LayerMutation),
    RemoveItemLayer { item_id: String },
    ShowHoverPanel(HoverPanelPlan),
    HideHoverPanel,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HostRuntimeState {
    pub window: Option<WindowFrame>,
    pub item_layers: HashMap<String, ItemLayerPlan>,
    pub hover_panel: Option<HoverPanelPlan>,
}

impl HostRuntimeState {
    fn apply(&mut self, commands: &[HostCommand]) {
        for command in commands {
            match command {
                HostCommand::ConfigureWindow(frame) => {
                    self.window = Some(frame.clone());
                }
                HostCommand::UpsertItemLayer(layer) => {
                    self.item_layers
                        .insert(layer.item_id.clone(), layer.layer.clone());
                }
                HostCommand::RemoveItemLayer { item_id } => {
                    self.item_layers.remove(item_id);
                }
                HostCommand::ShowHoverPanel(panel) => {
                    self.hover_panel = Some(panel.clone());
                }
                HostCommand::HideHoverPanel => {
                    self.hover_panel = None;
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NativeSurfaceState {
    pub bar_height: f64,
    pub item_spacing: f64,
    pub layout: LayoutGeometry,
    pub active_hover_item: Option<String>,
    pub items: Vec<PositionedItemSnapshot>,
}

impl Default for NativeSurfaceState {
    fn default() -> Self {
        Self {
            bar_height: 0.0,
            item_spacing: DEFAULT_ITEM_SPACING,
            layout: LayoutGeometry::default(),
            active_hover_item: None,
            items: Vec::new(),
        }
    }
}

impl NativeSurfaceState {
    fn update_snapshot(&mut self, snapshot: RenderItemSnapshot, bar_height: f64) {
        self.bar_height = bar_height;
        self.items.retain(|item| item.snapshot.id != snapshot.id);
        self.items.push(PositionedItemSnapshot {
            snapshot,
            frame: ItemFrame {
                x: 0.0,
                width: 0.0,
                height: bar_height,
            },
        });
        self.items.sort_by(|left, right| {
            section_rank(&left.snapshot.placement)
                .cmp(&section_rank(&right.snapshot.placement))
                .then_with(|| left.snapshot.order.cmp(&right.snapshot.order))
        });
        self.relayout();
    }

    pub fn set_layout_geometry(&mut self, layout: LayoutGeometry) {
        self.layout = layout;
        self.relayout();
    }

    fn relayout(&mut self) {
        if self.layout.bar_width <= 0.0 {
            self.relayout_content_width();
            return;
        }

        let left = item_indices_for_section(&self.items, BarSection::Left);
        let middle = item_indices_for_section(&self.items, BarSection::Middle);
        let right = item_indices_for_section(&self.items, BarSection::Right);

        self.layout_left_items(&left);
        self.layout_right_items(&right);
        self.layout_middle_items(&middle);
    }

    fn relayout_content_width(&mut self) {
        let mut cursor = self.item_spacing;
        for index in 0..self.items.len() {
            let width = measure_item_width(&self.items[index].snapshot);
            self.items[index].frame = ItemFrame {
                x: cursor,
                width,
                height: self.bar_height,
            };
            cursor += width + self.item_spacing;
        }
    }

    fn layout_left_items(&mut self, indices: &[usize]) {
        let mut cursor = self.layout.safe_left + self.item_spacing;
        for &index in indices {
            let width = measure_item_width(&self.items[index].snapshot);
            self.items[index].frame = ItemFrame {
                x: cursor,
                width,
                height: self.bar_height,
            };
            cursor += width + self.item_spacing;
        }
    }

    fn layout_right_items(&mut self, indices: &[usize]) {
        let mut cursor = (self.layout.bar_width - self.layout.safe_right - self.item_spacing)
            .max(self.layout.safe_left);
        for &index in indices {
            let width = measure_item_width(&self.items[index].snapshot);
            cursor = (cursor - width).max(self.layout.safe_left);
            self.items[index].frame = ItemFrame {
                x: cursor,
                width,
                height: self.bar_height,
            };
            cursor = (cursor - self.item_spacing).max(self.layout.safe_left);
        }
    }

    fn layout_middle_items(&mut self, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }

        if let Some(reserved) = self.layout.normalized_reserved() {
            self.layout_middle_items_around_reserved(indices, reserved);
            return;
        }

        let total_width = section_total_width(&self.items, indices, self.item_spacing);
        let usable_start = self.layout.safe_left;
        let usable_width = self.layout.usable_width();
        let mut cursor = usable_start + ((usable_width - total_width) / 2.0).max(0.0);
        for &index in indices {
            let width = measure_item_width(&self.items[index].snapshot);
            self.items[index].frame = ItemFrame {
                x: cursor,
                width,
                height: self.bar_height,
            };
            cursor += width + self.item_spacing;
        }
    }

    fn layout_middle_items_around_reserved(
        &mut self,
        indices: &[usize],
        reserved: CenterReservedRange,
    ) {
        let (left_indices, right_indices) =
            split_middle_indices(&self.items, indices, self.item_spacing);
        let mut left_cursor = (reserved.start - self.item_spacing).min(self.layout.bar_width);
        for index in left_indices {
            let width = measure_item_width(&self.items[index].snapshot);
            left_cursor = (left_cursor - width).max(self.layout.safe_left);
            self.items[index].frame = ItemFrame {
                x: left_cursor,
                width,
                height: self.bar_height,
            };
            left_cursor = (left_cursor - self.item_spacing).max(self.layout.safe_left);
        }

        let mut right_cursor = (reserved.end + self.item_spacing)
            .max(self.layout.safe_left)
            .min(self.layout.bar_width - self.layout.safe_right);
        for index in right_indices {
            let width = measure_item_width(&self.items[index].snapshot);
            let max_x =
                (self.layout.bar_width - self.layout.safe_right - width).max(self.layout.safe_left);
            let x = right_cursor.min(max_x);
            self.items[index].frame = ItemFrame {
                x,
                width,
                height: self.bar_height,
            };
            right_cursor =
                (x + width + self.item_spacing).min(self.layout.bar_width - self.layout.safe_right);
        }
    }

    fn handle_event(&mut self, event: &EventPayload) {
        match event.event {
            EventKind::HoverEnter | EventKind::HoverUpdate => {
                self.active_hover_item = self.item_at(event.mouse.x as f64, event.mouse.y as f64);
            }
            EventKind::HoverLeave
                if self.active_hover_item.as_deref() == Some(event.item_id.as_str()) =>
            {
                self.active_hover_item = None;
            }
            _ => {}
        }
    }

    fn item_at(&self, x: f64, y: f64) -> Option<String> {
        self.items
            .iter()
            .find(|item| item.frame.contains(x, y))
            .map(|item| item.snapshot.id.clone())
    }

    fn scene(&self) -> BarScene {
        let hover = self.active_hover_item.as_ref().and_then(|active_id| {
            self.items.iter().find_map(|item| {
                if &item.snapshot.id != active_id {
                    return None;
                }
                item.snapshot.hover.as_ref().map(|hover| HoverPresentation {
                    item_id: item.snapshot.id.clone(),
                    anchor_x: item.frame.x + (item.frame.width / 2.0),
                    anchor_y: item.frame.height,
                    tooltip: hover.tooltip.clone(),
                    detail: hover.detail.clone(),
                })
            })
        });

        BarScene {
            bar_height: self.bar_height,
            bar_width: if self.layout.bar_width > 0.0 {
                self.layout.bar_width
            } else {
                content_width(&self.items, self.item_spacing)
            },
            items: self.items.clone(),
            hover,
        }
    }
}

trait NativeHost: Send + Sync {
    fn initialize(&mut self, config: &Config) -> Result<(), BarrsError>;
    fn layout_geometry(
        &self,
        _config: &Config,
        _bar_height: f64,
    ) -> Result<LayoutGeometry, BarrsError> {
        Ok(LayoutGeometry::default())
    }
    fn present(&mut self, scene: &BarScene) -> Result<(), BarrsError>;
    fn drain_events(&mut self) -> Result<Vec<EventPayload>, BarrsError> {
        Ok(Vec::new())
    }
}

#[derive(Default)]
struct MockNativeHost {
    last_scene: Option<BarScene>,
    last_plan: Option<HostScenePlan>,
    last_commands: Vec<HostCommand>,
    runtime: HostRuntimeState,
}

impl MockNativeHost {
    #[cfg(test)]
    fn last_scene(&self) -> Option<&BarScene> {
        self.last_scene.as_ref()
    }

    #[cfg(test)]
    fn last_commands(&self) -> &[HostCommand] {
        &self.last_commands
    }

    #[cfg(test)]
    fn runtime(&self) -> &HostRuntimeState {
        &self.runtime
    }
}

impl NativeHost for MockNativeHost {
    fn initialize(&mut self, _config: &Config) -> Result<(), BarrsError> {
        Ok(())
    }

    fn present(&mut self, scene: &BarScene) -> Result<(), BarrsError> {
        let next_plan = host_scene_plan(scene);
        let commands = diff_host_scene(self.last_plan.as_ref(), &next_plan);
        self.last_scene = Some(scene.clone());
        self.last_plan = Some(next_plan);
        self.runtime.apply(&commands);
        self.last_commands = commands;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
struct AppKitItemView {
    icon_label: Option<objc2::rc::Retained<NSTextField>>,
    text_labels: Vec<objc2::rc::Retained<NSTextField>>,
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct AppKitHost {
    last_scene: Option<BarScene>,
    last_plan: Option<HostScenePlan>,
    last_commands: Vec<HostCommand>,
    runtime: HostRuntimeState,
    app: Option<objc2::rc::Retained<NSApplication>>,
    window: Option<objc2::rc::Retained<NSWindow>>,
    content_view: Option<objc2::rc::Retained<NSView>>,
    hover_panel: Option<objc2::rc::Retained<NSPanel>>,
    hover_label: Option<objc2::rc::Retained<NSTextField>>,
    item_views: HashMap<String, AppKitItemView>,
    pending_events: Vec<EventPayload>,
    pointer_item: Option<String>,
    background: Option<String>,
    current_screen_signature: Option<ScreenSignature>,
    display_reconfiguration_pending: Option<Box<AtomicBool>>,
    display_reconfiguration_deadline: Option<Instant>,
    display_reconfiguration_rebuilds_remaining: u8,
    display_callback_registered: bool,
    notch_offset: u32,
    notch_display_height: u32,
}

#[cfg(target_os = "macos")]
unsafe impl Send for AppKitHost {}

#[cfg(target_os = "macos")]
unsafe impl Sync for AppKitHost {}

#[cfg(target_os = "macos")]
impl NativeHost for AppKitHost {
    fn initialize(&mut self, config: &Config) -> Result<(), BarrsError> {
        let mtm = main_thread_marker()?;
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        self.background = config.bar.background.clone();
        self.notch_offset = config.bar.notch_offset;
        self.notch_display_height = config.bar.notch_display_height;
        self.app = Some(app);
        self.register_display_reconfiguration_callback()?;
        Ok(())
    }

    fn layout_geometry(
        &self,
        config: &Config,
        _bar_height: f64,
    ) -> Result<LayoutGeometry, BarrsError> {
        let mtm = main_thread_marker()?;
        Ok(bar_screen_layout_geometry(config, mtm))
    }

    fn present(&mut self, scene: &BarScene) -> Result<(), BarrsError> {
        self.prepare_for_target_display()?;
        let next_plan = host_scene_plan(scene);
        let commands = diff_host_scene(self.last_plan.as_ref(), &next_plan);
        self.last_scene = Some(scene.clone());
        self.last_plan = Some(next_plan);
        self.runtime.apply(&commands);
        self.apply_commands(&commands)?;
        self.last_commands = commands;
        Ok(())
    }

    fn drain_events(&mut self) -> Result<Vec<EventPayload>, BarrsError> {
        self.pump_events()?;
        let hover_events = self.poll_hover_payloads();
        self.pending_events.extend(hover_events);
        self.update_display_reconfiguration_schedule();
        let screen_changed = self.target_screen_changed()?;
        let reconfiguration_rebuild_due = self.display_reconfiguration_rebuild_due();
        if screen_changed || reconfiguration_rebuild_due {
            if reconfiguration_rebuild_due {
                self.invalidate_native_window("display-reconfiguration-rebuild");
            }
            if let Some(scene) = self.last_scene.clone() {
                self.present(&scene)?;
            }
        }
        Ok(std::mem::take(&mut self.pending_events))
    }
}

#[cfg(target_os = "macos")]
impl Drop for AppKitHost {
    fn drop(&mut self) {
        if !self.display_callback_registered {
            return;
        }

        if let Some(flag) = self.display_reconfiguration_pending.as_ref() {
            unsafe {
                CGDisplayRemoveReconfigurationCallback(
                    Some(display_reconfiguration_callback),
                    flag.as_ref() as *const AtomicBool as *mut c_void,
                );
            }
        }
        self.display_callback_registered = false;
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C-unwind" fn display_reconfiguration_callback(
    _display: u32,
    flags: CGDisplayChangeSummaryFlags,
    user_info: *mut c_void,
) {
    if user_info.is_null() || flags.contains(CGDisplayChangeSummaryFlags::BeginConfigurationFlag) {
        return;
    }

    let relevant = CGDisplayChangeSummaryFlags::MovedFlag
        | CGDisplayChangeSummaryFlags::SetMainFlag
        | CGDisplayChangeSummaryFlags::SetModeFlag
        | CGDisplayChangeSummaryFlags::AddFlag
        | CGDisplayChangeSummaryFlags::RemoveFlag
        | CGDisplayChangeSummaryFlags::EnabledFlag
        | CGDisplayChangeSummaryFlags::DisabledFlag
        | CGDisplayChangeSummaryFlags::MirrorFlag
        | CGDisplayChangeSummaryFlags::UnMirrorFlag
        | CGDisplayChangeSummaryFlags::DesktopShapeChangedFlag;

    if flags.intersects(relevant) {
        let pending = unsafe { &*(user_info as *const AtomicBool) };
        pending.store(true, Ordering::Release);
    }
}

#[cfg(target_os = "macos")]
impl AppKitHost {
    fn register_display_reconfiguration_callback(&mut self) -> Result<(), BarrsError> {
        if self.display_callback_registered {
            return Ok(());
        }

        let flag = self
            .display_reconfiguration_pending
            .get_or_insert_with(|| Box::new(AtomicBool::new(false)));
        let result = unsafe {
            CGDisplayRegisterReconfigurationCallback(
                Some(display_reconfiguration_callback),
                flag.as_ref() as *const AtomicBool as *mut c_void,
            )
        };
        if result != CGError::Success {
            return Err(BarrsError::Unsupported(format!(
                "failed to register display reconfiguration callback: {result:?}"
            )));
        }
        self.display_callback_registered = true;
        Ok(())
    }

    fn take_display_reconfiguration_pending(&self) -> bool {
        self.display_reconfiguration_pending
            .as_ref()
            .map(|flag| flag.swap(false, Ordering::AcqRel))
            .unwrap_or(false)
    }

    fn update_display_reconfiguration_schedule(&mut self) {
        if !self.take_display_reconfiguration_pending() {
            return;
        }

        debug_display_state("display-reconfiguration-callback", None);
        self.invalidate_native_window("display-reconfiguration-start");
        self.display_reconfiguration_rebuilds_remaining = DISPLAY_RECONFIGURATION_REBUILDS;
        self.display_reconfiguration_deadline =
            Some(Instant::now() + Duration::from_millis(DISPLAY_RECONFIGURATION_SETTLE_MS));
    }

    fn display_reconfiguration_rebuild_due(&mut self) -> bool {
        let Some(deadline) = self.display_reconfiguration_deadline else {
            return false;
        };
        if Instant::now() < deadline {
            return false;
        }

        if self.display_reconfiguration_rebuilds_remaining > 0 {
            self.display_reconfiguration_rebuilds_remaining -= 1;
        }
        if self.display_reconfiguration_rebuilds_remaining > 0 {
            self.display_reconfiguration_deadline =
                Some(Instant::now() + Duration::from_millis(DISPLAY_RECONFIGURATION_RETRY_MS));
        } else {
            self.display_reconfiguration_deadline = None;
        }
        true
    }

    fn invalidate_native_window(&mut self, reason: &str) {
        debug_display_state(reason, None);
        if let Some(window) = self.window.take() {
            window.orderOut(None);
        }
        self.content_view = None;
        self.item_views.clear();
        self.last_plan = None;
        self.current_screen_signature = None;
    }

    fn prepare_for_target_display(&mut self) -> Result<(), BarrsError> {
        let target_screen_signature = target_screen_signature()?;
        if self.current_screen_signature == target_screen_signature {
            return Ok(());
        }

        self.invalidate_native_window("target-screen-changed");
        self.current_screen_signature = target_screen_signature;
        Ok(())
    }

    fn target_screen_changed(&self) -> Result<bool, BarrsError> {
        Ok(self.current_screen_signature != target_screen_signature()?)
    }

    fn pump_events(&mut self) -> Result<(), BarrsError> {
        let Some(app) = self.app.clone() else {
            return Ok(());
        };
        let mask = NSEventMask::MouseMoved
            | NSEventMask::LeftMouseDragged
            | NSEventMask::RightMouseDragged
            | NSEventMask::LeftMouseDown
            | NSEventMask::RightMouseDown
            | NSEventMask::ScrollWheel;
        for _ in 0..64 {
            let Some(event) = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                mask,
                None,
                unsafe { NSDefaultRunLoopMode },
                true,
            ) else {
                break;
            };
            let payloads = self.event_payloads(&event);
            app.sendEvent(&event);
            self.pending_events.extend(payloads);
        }
        Ok(())
    }

    fn event_payloads(&mut self, event: &NSEvent) -> Vec<EventPayload> {
        let target = self.hit_test_event(event);
        match event.r#type() {
            NSEventType::LeftMouseDown => {
                if let Some(target) = target {
                    vec![event_payload(
                        target,
                        EventKind::Click,
                        event,
                        Some("left".into()),
                        None,
                    )]
                } else {
                    self.dismiss_hover_panel();
                    Vec::new()
                }
            }
            NSEventType::RightMouseDown => {
                if let Some(target) = target {
                    vec![event_payload(
                        target,
                        EventKind::RightClick,
                        event,
                        Some("right".into()),
                        None,
                    )]
                } else {
                    self.dismiss_hover_panel();
                    Vec::new()
                }
            }
            NSEventType::ScrollWheel => {
                if let Some(target) = target {
                    vec![event_payload(
                        target,
                        EventKind::Scroll,
                        event,
                        None,
                        Some(event.scrollingDeltaY().round() as i32),
                    )]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    fn poll_hover_payloads(&mut self) -> Vec<EventPayload> {
        let Some(window) = &self.window else {
            return Vec::new();
        };

        let location = window.mouseLocationOutsideOfEventStream();
        let next_item = self.last_scene.as_ref().and_then(|scene| {
            scene.items.iter().find_map(|item| {
                if item.frame.contains(location.x, location.y) {
                    Some(item.snapshot.id.clone())
                } else {
                    None
                }
            })
        });

        match (self.pointer_item.clone(), next_item) {
            (Some(previous), Some(next)) if previous == next => {
                vec![synthetic_event_payload(
                    next,
                    EventKind::HoverUpdate,
                    location.x,
                    location.y,
                )]
            }
            (Some(previous), Some(next)) => {
                self.pointer_item = Some(next.clone());
                vec![
                    synthetic_event_payload(
                        previous,
                        EventKind::HoverLeave,
                        location.x,
                        location.y,
                    ),
                    synthetic_event_payload(next, EventKind::HoverEnter, location.x, location.y),
                ]
            }
            (None, Some(next)) => {
                self.pointer_item = Some(next.clone());
                vec![synthetic_event_payload(
                    next,
                    EventKind::HoverEnter,
                    location.x,
                    location.y,
                )]
            }
            (Some(previous), None) => {
                self.pointer_item = None;
                self.dismiss_hover_panel();
                vec![synthetic_event_payload(
                    previous,
                    EventKind::HoverLeave,
                    location.x,
                    location.y,
                )]
            }
            (None, None) => Vec::new(),
        }
    }

    fn hit_test_event(&self, event: &NSEvent) -> Option<String> {
        let Some(window) = &self.window else {
            return None;
        };
        if event.windowNumber() != window.windowNumber() {
            return None;
        }
        let location = event.locationInWindow();
        self.last_scene.as_ref().and_then(|scene| {
            scene.items.iter().find_map(|item| {
                if item.frame.contains(location.x, location.y) {
                    Some(item.snapshot.id.clone())
                } else {
                    None
                }
            })
        })
    }

    fn apply_commands(&mut self, commands: &[HostCommand]) -> Result<(), BarrsError> {
        for command in commands {
            match command {
                HostCommand::ConfigureWindow(frame) => self.configure_window(frame)?,
                HostCommand::UpsertItemLayer(mutation) => self.upsert_item_layer(mutation)?,
                HostCommand::RemoveItemLayer { item_id } => self.remove_item_layer(item_id)?,
                HostCommand::ShowHoverPanel(panel) => self.show_hover_panel(panel)?,
                HostCommand::HideHoverPanel => self.hide_hover_panel(),
            }
        }

        if let Some(window) = &self.window {
            window.display();
        }
        if let Some(app) = &self.app {
            app.updateWindows();
        }
        Ok(())
    }

    fn configure_window(&mut self, frame: &WindowFrame) -> Result<(), BarrsError> {
        let mtm = main_thread_marker()?;
        let anchored = anchor_bar_frame(frame, mtm, self.notch_offset, self.notch_display_height);
        debug_display_state("configure-window", Some(&anchored));
        let content_frame = WindowFrame {
            x: 0.0,
            y: 0.0,
            width: anchored.width,
            height: anchored.height,
        };
        if self.window.is_none() || self.content_view.is_none() {
            let window = create_bar_window(mtm, &anchored, self.background.as_deref())?;
            let content_view = create_content_view(mtm, &content_frame);
            window.setContentView(Some(&content_view));
            window.orderFrontRegardless();
            self.content_view = Some(content_view);
            self.window = Some(window);
        }

        if let Some(window) = &self.window {
            apply_window_background(window, self.background.as_deref());
            window.setFrame_display(ns_rect(&anchored), true);
            window.orderFrontRegardless();
            apply_backstop_level(window);
        }
        if let Some(content_view) = &self.content_view {
            content_view.setFrame(ns_rect(&content_frame));
        }
        Ok(())
    }

    fn upsert_item_layer(&mut self, mutation: &LayerMutation) -> Result<(), BarrsError> {
        let mtm = main_thread_marker()?;
        let content_view = self
            .content_view
            .as_ref()
            .ok_or_else(|| BarrsError::Unsupported("content view is not initialized".into()))?;
        let entry = self
            .item_views
            .entry(mutation.item_id.clone())
            .or_insert_with(|| AppKitItemView {
                icon_label: None,
                text_labels: Vec::new(),
            });

        if let Some(icon) = &mutation.layer.icon {
            if entry.icon_label.is_none() {
                let icon_label = create_icon_label(mtm, &icon.value, &mutation.layer.frame)?;
                icon_label.setTextColor(Some(&color_for_tone(TextTone::Secondary)));
                content_view.addSubview(&icon_label);
                entry.icon_label = Some(icon_label);
            }
            if let Some(icon_label) = &entry.icon_label {
                icon_label.setStringValue(&NSString::from_str(&icon.value));
                layout_icon_label(icon_label, &mutation.layer.frame);
                icon_label.setHidden(false);
            }
        } else if let Some(icon_label) = &entry.icon_label {
            icon_label.setHidden(true);
        }

        while entry.text_labels.len() < mutation.layer.text_segments.len() {
            let label = create_label(mtm, "", &mutation.layer.frame)?;
            content_view.addSubview(&label);
            entry.text_labels.push(label);
        }

        for (index, segment) in mutation.layer.text_segments.iter().enumerate() {
            let label = &entry.text_labels[index];
            label.setStringValue(&NSString::from_str(&segment.value));
            label.setTextColor(Some(&color_for_tone(segment.tone)));
            layout_text_label(label, segment, &mutation.layer.frame);
            label.setHidden(false);
        }

        for label in entry
            .text_labels
            .iter()
            .skip(mutation.layer.text_segments.len())
        {
            label.setHidden(true);
        }
        Ok(())
    }

    fn remove_item_layer(&mut self, item_id: &str) -> Result<(), BarrsError> {
        if let Some(view) = self.item_views.remove(item_id) {
            if let Some(icon_label) = view.icon_label {
                icon_label.removeFromSuperview();
            }
            for label in view.text_labels {
                label.removeFromSuperview();
            }
        }
        Ok(())
    }

    fn show_hover_panel(&mut self, panel: &HoverPanelPlan) -> Result<(), BarrsError> {
        let mtm = main_thread_marker()?;
        if self.hover_panel.is_none() || self.hover_label.is_none() {
            let hover_panel = create_hover_panel(mtm, panel)?;
            let hover_label = create_hover_label(mtm, panel)?;
            if let Some(content_view) = hover_panel.contentView() {
                content_view.addSubview(&hover_label);
            }
            self.hover_label = Some(hover_label);
            self.hover_panel = Some(hover_panel);
        }

        let text = hover_panel_text(panel);
        let anchor = self
            .window
            .as_ref()
            .map(|window| {
                let frame = window.frame();
                (frame.origin.x + panel.anchor_x, frame.origin.y)
            })
            .unwrap_or((panel.anchor_x, panel.anchor_y));
        let frame = hover_panel_frame_at(anchor.0, anchor.1, &text);
        if let Some(label) = &self.hover_label {
            label.setStringValue(&NSString::from_str(&text));
            label.setFrame(NSRect {
                origin: NSPoint { x: 12.0, y: 8.0 },
                size: NSSize {
                    width: frame.width - 24.0,
                    height: frame.height - 16.0,
                },
            });
        }
        if let Some(panel_window) = &self.hover_panel {
            panel_window.setFrame_display(ns_rect(&frame), true);
            panel_window.orderFrontRegardless();
        }
        Ok(())
    }

    fn hide_hover_panel(&mut self) {
        if let Some(panel) = &self.hover_panel {
            panel.orderOut(None);
        }
    }

    fn dismiss_hover_panel(&mut self) {
        self.pointer_item = None;
        self.hide_hover_panel();
    }
}

#[cfg(target_os = "macos")]
fn main_thread_marker() -> Result<MainThreadMarker, BarrsError> {
    MainThreadMarker::new()
        .ok_or_else(|| BarrsError::Unsupported("AppKit host must run on the main thread".into()))
}

#[cfg(target_os = "macos")]
fn ns_rect(frame: &WindowFrame) -> NSRect {
    NSRect {
        origin: NSPoint {
            x: frame.x,
            y: frame.y,
        },
        size: NSSize {
            width: frame.width.max(1.0),
            height: frame.height.max(1.0),
        },
    }
}

#[cfg(target_os = "macos")]
fn create_bar_window(
    mtm: MainThreadMarker,
    frame: &WindowFrame,
    background: Option<&str>,
) -> Result<objc2::rc::Retained<NSWindow>, BarrsError> {
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            ns_rect(frame),
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe {
        window.setReleasedWhenClosed(false);
    }
    window.setOpaque(false);
    window.setHasShadow(false);
    window.setMovableByWindowBackground(false);
    window.setAcceptsMouseMovedEvents(true);
    window.setLevel(NSMainMenuWindowLevel);
    apply_window_background(&window, background);
    Ok(window)
}

#[cfg(target_os = "macos")]
fn apply_backstop_level(window: &NSWindow) {
    let window_id = window.windowNumber() as u32;
    if window_id == 0 {
        return;
    }
    unsafe {
        let cid = SLSMainConnectionID();
        SLSSetWindowLevel(cid, window_id, BACKSTOP_MENU_LEVEL);
    }
}

#[cfg(target_os = "macos")]
fn apply_window_background(window: &NSWindow, background: Option<&str>) {
    if let Some(color) = background.and_then(ns_color_from_hex) {
        window.setBackgroundColor(Some(&color));
    } else {
        window.setBackgroundColor(Some(&NSColor::windowBackgroundColor()));
    }
}

#[cfg(target_os = "macos")]
fn ns_color_from_hex(hex: &str) -> Option<objc2::rc::Retained<NSColor>> {
    let (red, green, blue, alpha) = parse_hex_color(hex)?;
    Some(NSColor::colorWithSRGBRed_green_blue_alpha(
        red, green, blue, alpha,
    ))
}

fn parse_hex_color(hex: &str) -> Option<(f64, f64, f64, f64)> {
    let value = hex.trim().strip_prefix('#')?;
    match value.len() {
        6 => {
            let red = u8::from_str_radix(&value[0..2], 16).ok()? as f64 / 255.0;
            let green = u8::from_str_radix(&value[2..4], 16).ok()? as f64 / 255.0;
            let blue = u8::from_str_radix(&value[4..6], 16).ok()? as f64 / 255.0;
            Some((red, green, blue, 1.0))
        }
        8 => {
            let red = u8::from_str_radix(&value[0..2], 16).ok()? as f64 / 255.0;
            let green = u8::from_str_radix(&value[2..4], 16).ok()? as f64 / 255.0;
            let blue = u8::from_str_radix(&value[4..6], 16).ok()? as f64 / 255.0;
            let alpha = u8::from_str_radix(&value[6..8], 16).ok()? as f64 / 255.0;
            Some((red, green, blue, alpha))
        }
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn create_content_view(mtm: MainThreadMarker, frame: &WindowFrame) -> objc2::rc::Retained<NSView> {
    let view = NSView::initWithFrame(NSView::alloc(mtm), ns_rect(frame));
    view.setWantsLayer(true);
    view
}

#[cfg(target_os = "macos")]
fn create_label(
    mtm: MainThreadMarker,
    value: &str,
    frame: &ItemFrame,
) -> Result<objc2::rc::Retained<NSTextField>, BarrsError> {
    let label = NSTextField::labelWithString(&NSString::from_str(value), mtm);
    label.setBezeled(false);
    label.setBordered(false);
    label.setDrawsBackground(false);
    label.setSelectable(false);
    label.setEditable(false);
    label.setFont(Some(&NSFont::systemFontOfSize(ITEM_LABEL_FONT_SIZE)));
    label.setTextColor(Some(&color_for_tone(TextTone::Primary)));
    layout_text_label(
        &label,
        &TextLayerPlan {
            value: value.into(),
            x: frame.x + ITEM_HORIZONTAL_PADDING,
            y: frame.height / 2.0,
            tone: TextTone::Primary,
        },
        frame,
    );
    Ok(label)
}

#[cfg(target_os = "macos")]
fn create_icon_label(
    mtm: MainThreadMarker,
    value: &str,
    frame: &ItemFrame,
) -> Result<objc2::rc::Retained<NSTextField>, BarrsError> {
    let label = NSTextField::labelWithString(&NSString::from_str(value), mtm);
    label.setBezeled(false);
    label.setBordered(false);
    label.setDrawsBackground(false);
    label.setSelectable(false);
    label.setEditable(false);
    label.setFont(Some(&NSFont::systemFontOfSize(ITEM_LABEL_FONT_SIZE)));
    layout_icon_label(&label, frame);
    Ok(label)
}

#[cfg(target_os = "macos")]
fn create_hover_panel(
    mtm: MainThreadMarker,
    panel: &HoverPanelPlan,
) -> Result<objc2::rc::Retained<NSPanel>, BarrsError> {
    let frame = hover_panel_frame(panel, &hover_panel_text(panel));
    let panel_window = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        ns_rect(&frame),
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    unsafe {
        panel_window.setReleasedWhenClosed(false);
    }
    panel_window.setFloatingPanel(true);
    panel_window.setWorksWhenModal(true);
    panel_window.setOpaque(false);
    panel_window.setHasShadow(true);
    panel_window.setLevel(NSStatusWindowLevel);
    panel_window.setBackgroundColor(Some(&NSColor::windowBackgroundColor()));

    let content_view = NSView::initWithFrame(NSView::alloc(mtm), ns_rect(&frame));
    panel_window.setContentView(Some(&content_view));
    Ok(panel_window)
}

#[cfg(target_os = "macos")]
fn create_hover_label(
    mtm: MainThreadMarker,
    panel: &HoverPanelPlan,
) -> Result<objc2::rc::Retained<NSTextField>, BarrsError> {
    let text = hover_panel_text(panel);
    let frame = hover_panel_frame(panel, &text);
    let label = NSTextField::labelWithString(&NSString::from_str(&text), mtm);
    label.setBezeled(false);
    label.setBordered(false);
    label.setDrawsBackground(false);
    label.setSelectable(false);
    label.setEditable(false);
    label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    label.setFont(Some(&NSFont::systemFontOfSize(13.0)));
    label.setFrame(NSRect {
        origin: NSPoint { x: 12.0, y: 8.0 },
        size: NSSize {
            width: frame.width - 24.0,
            height: frame.height - 16.0,
        },
    });
    Ok(label)
}

#[cfg(target_os = "macos")]
fn layout_text_label(label: &NSTextField, text: &TextLayerPlan, frame: &ItemFrame) {
    label.sizeToFit();
    let fitted = label.frame();
    let width = (fitted.size.width + 2.0).max(1.0);
    let height = fitted.size.height.max(ITEM_TEXT_HEIGHT);
    let y = ((frame.height - height) / 2.0).max(0.0);
    label.setFrame(NSRect {
        origin: NSPoint { x: text.x, y },
        size: NSSize { width, height },
    });
}

#[cfg(target_os = "macos")]
fn color_for_tone(tone: TextTone) -> objc2::rc::Retained<NSColor> {
    match tone {
        TextTone::Primary => NSColor::controlAccentColor(),
        TextTone::Secondary => NSColor::labelColor(),
        TextTone::Tertiary => NSColor::tertiaryLabelColor(),
    }
}

#[cfg(target_os = "macos")]
fn layout_icon_label(label: &NSTextField, frame: &ItemFrame) {
    label.sizeToFit();
    let fitted = label.frame();
    let height = fitted.size.height.max(ITEM_TEXT_HEIGHT);
    let y = ((frame.height - height) / 2.0).max(0.0);
    label.setFrame(NSRect {
        origin: NSPoint {
            x: frame.x + ITEM_HORIZONTAL_PADDING,
            y,
        },
        size: NSSize {
            width: ICON_WIDTH,
            height,
        },
    });
}

#[cfg(target_os = "macos")]
fn hover_panel_text(panel: &HoverPanelPlan) -> String {
    match (&panel.title, &panel.detail) {
        (Some(title), Some(detail)) => format!("{title}\n{detail}"),
        (Some(title), None) => title.clone(),
        (None, Some(detail)) => detail.clone(),
        (None, None) => String::new(),
    }
}

#[cfg(target_os = "macos")]
fn hover_panel_frame(panel: &HoverPanelPlan, text: &str) -> WindowFrame {
    hover_panel_frame_at(panel.anchor_x, panel.anchor_y, text)
}

#[cfg(target_os = "macos")]
fn hover_panel_frame_at(anchor_x: f64, anchor_y: f64, text: &str) -> WindowFrame {
    let max_line_width = text
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(10) as f64;
    let line_count = text.lines().count().max(1) as f64;
    let width = (max_line_width * CHARACTER_WIDTH) + 24.0;
    let height = (line_count * 18.0) + 16.0;
    WindowFrame {
        x: anchor_x - (width / 2.0),
        y: anchor_y - height - 4.0,
        width,
        height,
    }
}

#[cfg(target_os = "macos")]
fn bar_screen(mtm: MainThreadMarker) -> Option<objc2::rc::Retained<NSScreen>> {
    let screens = NSScreen::screens(mtm);
    let main_display = CGMainDisplayID();
    for index in 0..screens.count() {
        let screen = screens.objectAtIndex(index);
        if screen.CGDirectDisplayID() == main_display {
            return Some(screen);
        }
    }

    NSScreen::mainScreen(mtm).or_else(|| screens.firstObject())
}

#[cfg(target_os = "macos")]
fn target_screen_signature() -> Result<Option<ScreenSignature>, BarrsError> {
    let mtm = main_thread_marker()?;
    Ok(bar_screen(mtm).map(|screen| ScreenSignature::from(&appkit_screen_geometry(&screen))))
}

#[cfg(target_os = "macos")]
fn bar_screen_layout_geometry(config: &Config, mtm: MainThreadMarker) -> LayoutGeometry {
    let Some(screen) = bar_screen(mtm) else {
        return LayoutGeometry::default();
    };
    screen_layout_geometry(config, appkit_screen_geometry(&screen))
}

#[derive(Debug, Clone)]
struct ScreenGeometry {
    display_id: u32,
    full: WindowFrame,
    visible: WindowFrame,
    builtin: bool,
    notch_height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScreenSignature {
    display_id: u32,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
}

impl From<&ScreenGeometry> for ScreenSignature {
    fn from(screen: &ScreenGeometry) -> Self {
        Self {
            display_id: screen.display_id,
            x: screen.full.x.round() as i64,
            y: screen.full.y.round() as i64,
            width: screen.full.width.round() as i64,
            height: screen.full.height.round() as i64,
        }
    }
}

#[cfg(target_os = "macos")]
fn appkit_screen_geometry(screen: &NSScreen) -> ScreenGeometry {
    let display_id = screen.CGDirectDisplayID();
    let builtin = CGDisplayIsBuiltin(display_id);
    ScreenGeometry {
        display_id,
        full: window_frame_from_rect(screen.frame()),
        visible: window_frame_from_rect(screen.visibleFrame()),
        builtin,
        notch_height: if builtin {
            screen.safeAreaInsets().top.max(0.0)
        } else {
            0.0
        },
    }
}

#[cfg(target_os = "macos")]
fn window_frame_from_rect(rect: NSRect) -> WindowFrame {
    WindowFrame {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}

fn screen_layout_geometry(config: &Config, screen: ScreenGeometry) -> LayoutGeometry {
    LayoutGeometry {
        bar_width: screen.full.width,
        safe_left: 0.0,
        safe_right: 0.0,
        center_reserved: center_reserved_range(
            screen.full.width,
            config.bar.notch_width,
            config.bar.notch_padding,
            screen.builtin && screen.notch_height > 0.0,
        ),
    }
}

fn center_reserved_range(
    bar_width: f64,
    notch_width: u32,
    notch_padding: u32,
    notch_active: bool,
) -> Option<CenterReservedRange> {
    if !notch_active || bar_width <= 0.0 || notch_width == 0 {
        return None;
    }
    let width = (notch_width as f64 + 2.0 * notch_padding as f64).min(bar_width);
    let center = bar_width / 2.0;
    Some(CenterReservedRange {
        start: center - (width / 2.0),
        end: center + (width / 2.0),
    })
}

#[cfg(target_os = "macos")]
fn anchor_bar_frame(
    frame: &WindowFrame,
    mtm: MainThreadMarker,
    notch_offset: u32,
    notch_display_height: u32,
) -> WindowFrame {
    let Some(screen) = bar_screen(mtm) else {
        return frame.clone();
    };
    anchor_bar_frame_for_screen(
        frame,
        appkit_screen_geometry(&screen),
        notch_offset,
        notch_display_height,
    )
}

#[cfg(target_os = "macos")]
fn debug_display_state(reason: &str, anchored: Option<&WindowFrame>) {
    if std::env::var_os("BARRS_DEBUG_DISPLAYS").is_none() {
        return;
    }

    let Ok(mtm) = main_thread_marker() else {
        eprintln!("[barrs-display] {reason}: no main thread marker");
        return;
    };
    let main_display = CGMainDisplayID();
    eprintln!("[barrs-display] {reason}: main_display={main_display}");
    if let Some(anchored) = anchored {
        eprintln!("[barrs-display] {reason}: anchored={anchored:?}");
    }

    let screens = NSScreen::screens(mtm);
    for index in 0..screens.count() {
        let screen = screens.objectAtIndex(index);
        let display_id = screen.CGDirectDisplayID();
        let frame = screen.frame();
        let visible = screen.visibleFrame();
        let cg = CGDisplayBounds(display_id);
        eprintln!(
            "[barrs-display] {reason}: screen[{index}] id={display_id} is_main={} frame=({}, {}, {}, {}) visible=({}, {}, {}, {}) cg=({}, {}, {}, {}) safe_top={} scale={}",
            display_id == main_display,
            frame.origin.x,
            frame.origin.y,
            frame.size.width,
            frame.size.height,
            visible.origin.x,
            visible.origin.y,
            visible.size.width,
            visible.size.height,
            cg.origin.x,
            cg.origin.y,
            cg.size.width,
            cg.size.height,
            screen.safeAreaInsets().top,
            screen.backingScaleFactor()
        );
    }
}

fn anchor_bar_frame_for_screen(
    frame: &WindowFrame,
    screen: ScreenGeometry,
    notch_offset: u32,
    notch_display_height: u32,
) -> WindowFrame {
    let visible_top = screen.visible.y + screen.visible.height;
    let full_top = screen.full.y + screen.full.height;
    let height = if screen.builtin && notch_display_height > 0 {
        notch_display_height as f64
    } else {
        frame.height
    };
    let y = bar_window_y(full_top, visible_top, height);
    WindowFrame {
        x: screen.full.x,
        y: y + if screen.builtin {
            notch_offset as f64
        } else {
            0.0
        },
        width: screen.full.width,
        height,
    }
}

fn bar_window_y(full_top: f64, visible_top: f64, height: f64) -> f64 {
    let top_inset = full_top - visible_top;
    let max_menu_bar_inset = (height * 3.0).max(96.0);
    if top_inset.is_finite() && top_inset > 0.0 && top_inset <= max_menu_bar_inset {
        visible_top
    } else {
        full_top - height
    }
}

fn create_native_host() -> Box<dyn NativeHost> {
    #[cfg(target_os = "macos")]
    {
        Box::new(AppKitHost::default())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(MockNativeHost::default())
    }
}

pub fn create_renderer(kind: RendererKind) -> Result<Box<dyn Renderer>, BarrsError> {
    match kind {
        RendererKind::Native => Ok(Box::new(NativeRenderer::new(create_native_host()))),
        RendererKind::Noop => Ok(Box::new(NoopRenderer::default())),
    }
}

pub struct NativeRenderer {
    state: NativeSurfaceState,
    host: Box<dyn NativeHost>,
    config: Option<Config>,
}

impl NativeRenderer {
    fn new(host: Box<dyn NativeHost>) -> Self {
        Self {
            state: NativeSurfaceState::default(),
            host,
            config: None,
        }
    }

    pub fn surface_state(&self) -> &NativeSurfaceState {
        &self.state
    }

    fn publish_scene(&mut self) -> Result<(), BarrsError> {
        self.refresh_layout_geometry()?;
        let scene = self.state.scene();
        self.host.present(&scene)
    }

    fn refresh_layout_geometry(&mut self) -> Result<bool, BarrsError> {
        let Some(config) = self.config.as_ref() else {
            return Ok(false);
        };
        let layout = self.host.layout_geometry(config, self.state.bar_height)?;
        if layout == LayoutGeometry::default() {
            return Ok(false);
        }
        if self.state.layout != layout {
            self.state.set_layout_geometry(layout);
            return Ok(true);
        }
        Ok(false)
    }
}

impl Default for NativeRenderer {
    fn default() -> Self {
        Self::new(Box::new(MockNativeHost::default()))
    }
}

impl Renderer for NativeRenderer {
    fn initialize(&mut self, config: &Config) -> Result<(), BarrsError> {
        self.state.bar_height = BAR_HEIGHT;
        self.state.item_spacing = config.bar.spacing as f64;
        self.config = Some(config.clone());
        self.host.initialize(config)?;
        let layout = self.host.layout_geometry(config, self.state.bar_height)?;
        self.state.set_layout_geometry(layout);
        Ok(())
    }

    fn render_item(&mut self, snapshot: &RenderItemSnapshot) -> Result<(), BarrsError> {
        self.state
            .update_snapshot(snapshot.clone(), self.state.bar_height.max(1.0));
        self.publish_scene()
    }

    fn drain_events(&mut self) -> Result<Vec<EventPayload>, BarrsError> {
        let events = self.host.drain_events()?;
        if self.refresh_layout_geometry()? {
            let scene = self.state.scene();
            self.host.present(&scene)?;
        }
        Ok(events)
    }

    fn handle_event(&mut self, event: &EventPayload) -> Result<(), BarrsError> {
        self.state.handle_event(event);
        self.publish_scene()
    }
}

#[derive(Default)]
pub struct NoopRenderer {
    rendered_items: usize,
    snapshots: HashMap<String, RenderItemSnapshot>,
}

impl NoopRenderer {
    pub fn rendered_items(&self) -> usize {
        self.rendered_items
    }

    pub fn snapshot(&self, item_id: &str) -> Option<&RenderItemSnapshot> {
        self.snapshots.get(item_id)
    }
}

impl Renderer for NoopRenderer {
    fn initialize(&mut self, _config: &Config) -> Result<(), BarrsError> {
        Ok(())
    }

    fn render_item(&mut self, snapshot: &RenderItemSnapshot) -> Result<(), BarrsError> {
        self.rendered_items += 1;
        self.snapshots.insert(snapshot.id.clone(), snapshot.clone());
        Ok(())
    }
}

fn measure_item_width(snapshot: &RenderItemSnapshot) -> f64 {
    let icon_width = if snapshot.icon.is_some() {
        ICON_WIDTH + ICON_TEXT_SPACING
    } else {
        0.0
    };
    let text_width = measure_snapshot_text_width(snapshot);
    ITEM_HORIZONTAL_PADDING * 2.0 + icon_width + text_width + ITEM_TRAILING_TEXT_PADDING
}

fn item_indices_for_section(items: &[PositionedItemSnapshot], section: BarSection) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            if BarSection::from_placement(item.snapshot.placement.as_deref())
                .unwrap_or(BarSection::Left)
                == section
            {
                Some(index)
            } else {
                None
            }
        })
        .collect()
}

fn section_total_width(items: &[PositionedItemSnapshot], indices: &[usize], spacing: f64) -> f64 {
    let widths = indices
        .iter()
        .map(|index| measure_item_width(&items[*index].snapshot))
        .sum::<f64>();
    widths + spacing * indices.len().saturating_sub(1) as f64
}

fn content_width(items: &[PositionedItemSnapshot], spacing: f64) -> f64 {
    items
        .iter()
        .map(|item| item.frame.x + item.frame.width + spacing)
        .fold(0.0, f64::max)
}

fn split_middle_indices(
    items: &[PositionedItemSnapshot],
    indices: &[usize],
    spacing: f64,
) -> (Vec<usize>, Vec<usize>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut left_width = 0.0;
    let mut right_width = 0.0;

    for &index in indices {
        let width = measure_item_width(&items[index].snapshot);
        if left_width <= right_width {
            left.push(index);
            left_width += width + spacing;
        } else {
            right.push(index);
            right_width += width + spacing;
        }
    }

    (left, right)
}

fn measure_snapshot_text_width(snapshot: &RenderItemSnapshot) -> f64 {
    if let Some(width) = measure_workspace_text_width(snapshot) {
        return width;
    }
    stable_text_char_count(&snapshot.text) as f64 * CHARACTER_WIDTH
}

fn stable_text_char_count(text: &str) -> usize {
    if text
        .strip_suffix('%')
        .is_some_and(|value| value.parse::<u8>().is_ok_and(|percent| percent <= 100))
    {
        return "100%".chars().count();
    }
    text.chars().count()
}

fn measure_workspace_text_width(snapshot: &RenderItemSnapshot) -> Option<f64> {
    let workspaces = snapshot.data.get("workspaces")?.as_array()?;
    let mut width = 0.0;
    for (index, workspace) in workspaces.iter().enumerate() {
        let name = workspace.get("name").and_then(Value::as_str)?;
        if index > 0 {
            width += DEFAULT_ITEM_SPACING;
        }
        width += name.chars().count() as f64 * CHARACTER_WIDTH;
    }
    Some(width)
}

fn workspace_text_segments(
    snapshot: &RenderItemSnapshot,
    base_x: f64,
    center_y: f64,
) -> Option<Vec<TextLayerPlan>> {
    let workspaces = snapshot.data.get("workspaces")?.as_array()?;
    let mut cursor = base_x;
    let mut segments = Vec::with_capacity(workspaces.len());
    for workspace in workspaces {
        let name = workspace.get("name").and_then(Value::as_str)?.to_string();
        let is_current = workspace
            .get("is_current")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let has_windows = workspace
            .get("has_windows")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        segments.push(TextLayerPlan {
            value: name.clone(),
            x: cursor,
            y: center_y,
            tone: if is_current {
                TextTone::Primary
            } else if has_windows {
                TextTone::Secondary
            } else {
                TextTone::Tertiary
            },
        });
        cursor += (name.chars().count() as f64 * CHARACTER_WIDTH) + DEFAULT_ITEM_SPACING;
    }
    Some(segments)
}

fn host_scene_plan(scene: &BarScene) -> HostScenePlan {
    let content_width = scene
        .items
        .last()
        .map(|item| item.frame.x + item.frame.width + scene.items[0].frame.x)
        .unwrap_or(0.0);
    let item_layers = scene
        .items
        .iter()
        .map(|item| {
            let icon = item.snapshot.icon.as_ref().map(|value| TextLayerPlan {
                value: value.clone(),
                x: item.frame.x + ITEM_HORIZONTAL_PADDING,
                y: item.frame.height / 2.0,
                tone: TextTone::Secondary,
            });
            let text_x = item.frame.x
                + ITEM_HORIZONTAL_PADDING
                + if icon.is_some() {
                    ICON_WIDTH + ICON_TEXT_SPACING
                } else {
                    0.0
                };
            let text_segments =
                workspace_text_segments(&item.snapshot, text_x, item.frame.height / 2.0)
                    .unwrap_or_else(|| {
                        vec![TextLayerPlan {
                            value: item.snapshot.text.clone(),
                            x: text_x,
                            y: item.frame.height / 2.0,
                            tone: TextTone::Secondary,
                        }]
                    });
            ItemLayerPlan {
                item_id: item.snapshot.id.clone(),
                frame: item.frame.clone(),
                icon,
                text_segments,
                hoverable: item.snapshot.interactive.hover,
            }
        })
        .collect();
    let hover_panel = scene.hover.as_ref().map(|hover| HoverPanelPlan {
        item_id: hover.item_id.clone(),
        anchor_x: hover.anchor_x,
        anchor_y: hover.anchor_y,
        title: hover.tooltip.clone(),
        detail: hover.detail.clone(),
    });

    HostScenePlan {
        window: WindowFrame {
            x: 0.0,
            y: 0.0,
            width: scene.bar_width.max(content_width),
            height: scene.bar_height,
        },
        item_layers,
        hover_panel,
    }
}

fn diff_host_scene(previous: Option<&HostScenePlan>, next: &HostScenePlan) -> Vec<HostCommand> {
    let mut commands = Vec::new();
    if previous.map(|plan| &plan.window) != Some(&next.window) {
        commands.push(HostCommand::ConfigureWindow(next.window.clone()));
    }

    let mut previous_layers = HashMap::new();
    if let Some(previous) = previous {
        for layer in &previous.item_layers {
            previous_layers.insert(layer.item_id.clone(), layer);
        }
    }

    for layer in &next.item_layers {
        let previous_layer = previous_layers.get(&layer.item_id);
        if previous_layer != Some(&layer) {
            commands.push(HostCommand::UpsertItemLayer(LayerMutation {
                item_id: layer.item_id.clone(),
                layer: layer.clone(),
                is_new: previous_layer.is_none(),
            }));
        }
    }

    if let Some(previous) = previous {
        for layer in &previous.item_layers {
            if !next
                .item_layers
                .iter()
                .any(|candidate| candidate.item_id == layer.item_id)
            {
                commands.push(HostCommand::RemoveItemLayer {
                    item_id: layer.item_id.clone(),
                });
            }
        }
    }

    if previous.and_then(|plan| plan.hover_panel.as_ref()) != next.hover_panel.as_ref() {
        match &next.hover_panel {
            Some(panel) => commands.push(HostCommand::ShowHoverPanel(panel.clone())),
            None => commands.push(HostCommand::HideHoverPanel),
        }
    }

    commands
}

fn section_rank(placement: &Option<String>) -> u8 {
    BarSection::from_placement(placement.as_deref())
        .unwrap_or(BarSection::Left)
        .rank()
}

#[cfg(target_os = "macos")]
fn event_payload(
    item_id: String,
    event: EventKind,
    source: &NSEvent,
    button: Option<String>,
    scroll_delta: Option<i32>,
) -> EventPayload {
    let flags = source.modifierFlags();
    let location = source.locationInWindow();
    EventPayload {
        item_id,
        event,
        timestamp_ms: current_timestamp_ms(),
        mouse: crate::ipc::MouseState {
            x: location.x.round() as i32,
            y: location.y.round() as i32,
            button,
            scroll_delta,
        },
        modifiers: crate::ipc::Modifiers {
            shift: flags.contains(NSEventModifierFlags::Shift),
            control: flags.contains(NSEventModifierFlags::Control),
            option: flags.contains(NSEventModifierFlags::Option),
            command: flags.contains(NSEventModifierFlags::Command),
        },
    }
}

#[cfg(target_os = "macos")]
fn synthetic_event_payload(item_id: String, event: EventKind, x: f64, y: f64) -> EventPayload {
    EventPayload {
        item_id,
        event,
        timestamp_ms: current_timestamp_ms(),
        mouse: crate::ipc::MouseState {
            x: x.round() as i32,
            y: y.round() as i32,
            button: None,
            scroll_delta: None,
        },
        modifiers: crate::ipc::Modifiers::default(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use serde_json::json;

    use crate::config::{Config, HoverConfig, ItemConfig, ItemHandlers};
    use crate::ipc::{EventKind, EventPayload, Modifiers, MouseState};

    use super::{
        HostCommand, HostRuntimeState, HoverSurface, InteractiveSnapshot, LayerMutation,
        MockNativeHost, NativeHost, NativeRenderer, NoopRenderer, RenderItemSnapshot, Renderer,
        RendererKind, create_renderer, diff_host_scene, host_scene_plan, parse_hex_color,
    };

    fn test_snapshot(
        id: &str,
        order: usize,
        placement: Option<&str>,
        text: &str,
    ) -> RenderItemSnapshot {
        RenderItemSnapshot {
            id: id.into(),
            order,
            label: None,
            icon: None,
            placement: placement.map(str::to_owned),
            text: text.into(),
            hover: None,
            interactive: InteractiveSnapshot {
                click: false,
                right_click: false,
                scroll: false,
                hover: false,
            },
            data: json!({ "text": text }),
        }
    }

    fn assert_close(left: f64, right: f64) {
        assert!(
            (left - right).abs() < 0.001,
            "expected {left} to be close to {right}"
        );
    }

    #[test]
    fn builds_render_snapshot_from_item_config() {
        let item = ItemConfig {
            id: "cpu".into(),
            label: Some("CPU".into()),
            icon: Some("􀧓".into()),
            placement: Some("left".into()),
            interval: Some(1),
            plugin: None,
            hover: Some(HoverConfig {
                tooltip: Some("CPU usage".into()),
                detail: None,
            }),
            handlers: ItemHandlers {
                click: Some("handle_click".into()),
                ..ItemHandlers::default()
            },
        };

        let snapshot = RenderItemSnapshot::from_item_config(
            &item,
            0,
            json!({ "text": "13%", "usage_percent": 13.0 }),
        );
        assert_eq!(snapshot.text, "13%");
        assert!(snapshot.interactive.click);
        assert!(snapshot.interactive.hover);
    }

    #[test]
    fn noop_renderer_stores_snapshots() {
        let mut renderer = NoopRenderer::default();
        let item = RenderItemSnapshot {
            id: "clock".into(),
            order: 0,
            label: Some("clock".into()),
            icon: None,
            placement: None,
            text: "12:34".into(),
            hover: None,
            interactive: InteractiveSnapshot {
                click: false,
                right_click: false,
                scroll: false,
                hover: false,
            },
            data: json!({ "text": "12:34" }),
        };

        renderer.render_item(&item).expect("render item");
        assert_eq!(renderer.rendered_items(), 1);
        assert_eq!(renderer.snapshot("clock").expect("snapshot").text, "12:34");
    }

    #[test]
    fn factory_creates_native_renderer() {
        let _renderer = create_renderer(RendererKind::Native).expect("native renderer");
    }

    #[test]
    fn native_renderer_tracks_layout_and_hover_target() {
        let host = Box::new(MockNativeHost::default());
        let mut renderer = NativeRenderer::new(host);
        let config = Config {
            bar: crate::config::BarConfig {
                spacing: 3,
                background: None,
                ..crate::config::BarConfig::default()
            },
            ..Config::default()
        };
        renderer.initialize(&config).expect("initialize");
        renderer
            .render_item(&RenderItemSnapshot {
                id: "cpu".into(),
                order: 0,
                label: Some("CPU".into()),
                icon: None,
                placement: Some("left".into()),
                text: "12%".into(),
                hover: Some(HoverSurface {
                    tooltip: Some("CPU usage".into()),
                    detail: None,
                }),
                interactive: InteractiveSnapshot {
                    click: false,
                    right_click: false,
                    scroll: false,
                    hover: true,
                },
                data: json!({ "text": "12%" }),
            })
            .expect("render");

        renderer
            .handle_event(&EventPayload {
                item_id: "cpu".into(),
                event: EventKind::HoverEnter,
                timestamp_ms: 0,
                mouse: MouseState {
                    x: 20,
                    y: 10,
                    button: None,
                    scroll_delta: None,
                },
                modifiers: Modifiers::default(),
            })
            .expect("handle hover");

        let state = renderer.surface_state();
        assert_eq!(state.bar_height, super::BAR_HEIGHT);
        assert_eq!(state.item_spacing, 3.0);
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.active_hover_item.as_deref(), Some("cpu"));
        assert!(state.items[0].frame.width > 0.0);
    }

    #[test]
    fn percentage_width_is_stable_across_digit_changes() {
        let narrow = RenderItemSnapshot {
            id: "cpu".into(),
            order: 0,
            label: None,
            icon: Some("cpu".into()),
            placement: Some("left".into()),
            text: "9%".into(),
            hover: None,
            interactive: InteractiveSnapshot {
                click: false,
                right_click: false,
                scroll: false,
                hover: false,
            },
            data: json!({ "text": "9%", "usage_percent": 9 }),
        };
        let mut wider = narrow.clone();
        wider.text = "10%".into();
        wider.data = json!({ "text": "10%", "usage_percent": 10 });

        assert_eq!(
            super::measure_item_width(&narrow),
            super::measure_item_width(&wider)
        );
    }

    #[test]
    fn section_rank_treats_middle_as_center() {
        assert_eq!(super::section_rank(&Some("left".into())), 0);
        assert_eq!(super::section_rank(&Some("middle".into())), 1);
        assert_eq!(super::section_rank(&Some("center".into())), 1);
        assert_eq!(super::section_rank(&Some("right".into())), 2);
        assert_eq!(super::section_rank(&Some("unknown".into())), 0);
        assert_eq!(super::section_rank(&None), 0);
    }

    #[test]
    fn native_renderer_preserves_order_within_sections() {
        let mut renderer = NativeRenderer::new(Box::new(MockNativeHost::default()));
        renderer.initialize(&Config::default()).expect("initialize");

        for snapshot in [
            RenderItemSnapshot {
                id: "right".into(),
                order: 4,
                label: None,
                icon: None,
                placement: Some("right".into()),
                text: "right".into(),
                hover: None,
                interactive: InteractiveSnapshot {
                    click: false,
                    right_click: false,
                    scroll: false,
                    hover: false,
                },
                data: json!({ "text": "right" }),
            },
            RenderItemSnapshot {
                id: "left-2".into(),
                order: 2,
                label: None,
                icon: None,
                placement: Some("left".into()),
                text: "left-2".into(),
                hover: None,
                interactive: InteractiveSnapshot {
                    click: false,
                    right_click: false,
                    scroll: false,
                    hover: false,
                },
                data: json!({ "text": "left-2" }),
            },
            RenderItemSnapshot {
                id: "middle".into(),
                order: 3,
                label: None,
                icon: None,
                placement: Some("middle".into()),
                text: "middle".into(),
                hover: None,
                interactive: InteractiveSnapshot {
                    click: false,
                    right_click: false,
                    scroll: false,
                    hover: false,
                },
                data: json!({ "text": "middle" }),
            },
            RenderItemSnapshot {
                id: "left-1".into(),
                order: 1,
                label: None,
                icon: None,
                placement: None,
                text: "left-1".into(),
                hover: None,
                interactive: InteractiveSnapshot {
                    click: false,
                    right_click: false,
                    scroll: false,
                    hover: false,
                },
                data: json!({ "text": "left-1" }),
            },
        ] {
            renderer.render_item(&snapshot).expect("render item");
        }

        let ids = renderer
            .surface_state()
            .items
            .iter()
            .map(|item| item.snapshot.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["left-1", "left-2", "middle", "right"]);
    }

    #[test]
    fn layout_positions_left_middle_and_right_sections_independently() {
        let mut renderer = NativeRenderer::new(Box::new(MockNativeHost::default()));
        renderer
            .initialize(&Config {
                bar: crate::config::BarConfig {
                    spacing: 4,
                    ..crate::config::BarConfig::default()
                },
                ..Config::default()
            })
            .expect("initialize");
        renderer.state.set_layout_geometry(super::LayoutGeometry {
            bar_width: 300.0,
            safe_left: 10.0,
            safe_right: 20.0,
            center_reserved: None,
        });

        renderer
            .render_item(&test_snapshot("left", 0, Some("left"), "L"))
            .expect("render left");
        renderer
            .render_item(&test_snapshot("middle", 1, Some("middle"), "M"))
            .expect("render middle");
        renderer
            .render_item(&test_snapshot("right", 2, Some("right"), "R"))
            .expect("render right");

        let item = |id: &str| {
            renderer
                .surface_state()
                .items
                .iter()
                .find(|item| item.snapshot.id == id)
                .expect("item")
                .frame
                .clone()
        };
        let left = item("left");
        let middle = item("middle");
        let right = item("right");

        assert_close(left.x, 14.0);
        assert_close(middle.x + (middle.width / 2.0), 145.0);
        assert_close(right.x + right.width, 276.0);
    }

    #[test]
    fn layout_splits_middle_items_around_reserved_center_gap() {
        let mut renderer = NativeRenderer::new(Box::new(MockNativeHost::default()));
        renderer
            .initialize(&Config {
                bar: crate::config::BarConfig {
                    spacing: 4,
                    ..crate::config::BarConfig::default()
                },
                ..Config::default()
            })
            .expect("initialize");
        renderer.state.set_layout_geometry(super::LayoutGeometry {
            bar_width: 300.0,
            safe_left: 0.0,
            safe_right: 0.0,
            center_reserved: Some(super::CenterReservedRange {
                start: 125.0,
                end: 175.0,
            }),
        });

        renderer
            .render_item(&test_snapshot("middle-left", 0, Some("middle"), "L"))
            .expect("render middle left");
        renderer
            .render_item(&test_snapshot("middle-right", 1, Some("middle"), "R"))
            .expect("render middle right");

        for item in &renderer.surface_state().items {
            let frame = &item.frame;
            assert!(
                frame.x + frame.width <= 125.0 || frame.x >= 175.0,
                "{} overlaps the reserved center gap: {:?}",
                item.snapshot.id,
                frame
            );
        }
    }

    #[test]
    fn host_scene_plan_uses_full_layout_width() {
        let mut renderer = NativeRenderer::new(Box::new(MockNativeHost::default()));
        renderer.initialize(&Config::default()).expect("initialize");
        renderer.state.set_layout_geometry(super::LayoutGeometry {
            bar_width: 320.0,
            safe_left: 0.0,
            safe_right: 0.0,
            center_reserved: None,
        });
        renderer
            .render_item(&test_snapshot("left", 0, Some("left"), "L"))
            .expect("render left");

        let plan = host_scene_plan(&renderer.state.scene());
        assert_eq!(plan.window.width, 320.0);
    }

    #[test]
    fn center_reserved_range_uses_configured_notch_width() {
        assert_eq!(super::center_reserved_range(300.0, 0, 8, true), None);
        assert_eq!(super::center_reserved_range(300.0, 80, 8, false), None);
        assert_eq!(
            super::center_reserved_range(300.0, 80, 8, true),
            Some(super::CenterReservedRange {
                start: 102.0,
                end: 198.0,
            })
        );
    }

    #[test]
    fn screen_layout_geometry_reserves_notch_only_on_notched_builtin_screen() {
        let config = Config {
            bar: crate::config::BarConfig {
                notch_width: 80,
                ..crate::config::BarConfig::default()
            },
            ..Config::default()
        };
        let screen = super::ScreenGeometry {
            display_id: 1,
            full: super::WindowFrame {
                x: 100.0,
                y: 0.0,
                width: 300.0,
                height: 200.0,
            },
            visible: super::WindowFrame {
                x: 100.0,
                y: 0.0,
                width: 300.0,
                height: 180.0,
            },
            builtin: true,
            notch_height: 24.0,
        };

        let layout = super::screen_layout_geometry(&config, screen.clone());
        assert_eq!(layout.bar_width, 300.0);
        assert_eq!(
            layout.center_reserved,
            Some(super::CenterReservedRange {
                start: 102.0,
                end: 198.0,
            })
        );

        let external = super::ScreenGeometry {
            builtin: false,
            ..screen
        };
        assert_eq!(
            super::screen_layout_geometry(&config, external).center_reserved,
            None
        );
    }

    #[test]
    fn anchor_bar_frame_uses_screen_origin_and_builtin_notch_settings() {
        let frame = super::WindowFrame {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 28.0,
        };
        let screen = super::ScreenGeometry {
            display_id: 1,
            full: super::WindowFrame {
                x: 1440.0,
                y: -200.0,
                width: 1280.0,
                height: 900.0,
            },
            visible: super::WindowFrame {
                x: 1440.0,
                y: -200.0,
                width: 1280.0,
                height: 860.0,
            },
            builtin: true,
            notch_height: 24.0,
        };

        assert_eq!(
            super::anchor_bar_frame_for_screen(&frame, screen, 2, 32),
            super::WindowFrame {
                x: 1440.0,
                y: 662.0,
                width: 1280.0,
                height: 32.0,
            }
        );
    }

    #[test]
    fn anchor_bar_frame_ignores_implausible_visible_top_after_hotplug() {
        let frame = super::WindowFrame {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 28.0,
        };
        let screen = super::ScreenGeometry {
            display_id: 1,
            full: super::WindowFrame {
                x: 0.0,
                y: 0.0,
                width: 2048.0,
                height: 1280.0,
            },
            visible: super::WindowFrame {
                x: 0.0,
                y: 0.0,
                width: 2048.0,
                height: 170.0,
            },
            builtin: false,
            notch_height: 0.0,
        };

        assert_eq!(
            super::anchor_bar_frame_for_screen(&frame, screen, 0, 0),
            super::WindowFrame {
                x: 0.0,
                y: 1252.0,
                width: 2048.0,
                height: 28.0,
            }
        );
    }

    struct ChangingLayoutHost {
        calls: AtomicUsize,
        scenes: Arc<Mutex<Vec<super::BarScene>>>,
    }

    impl NativeHost for ChangingLayoutHost {
        fn initialize(&mut self, _config: &Config) -> Result<(), crate::error::BarrsError> {
            Ok(())
        }

        fn layout_geometry(
            &self,
            _config: &Config,
            _bar_height: f64,
        ) -> Result<super::LayoutGeometry, crate::error::BarrsError> {
            let calls = self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(super::LayoutGeometry {
                bar_width: if calls == 0 { 100.0 } else { 240.0 },
                safe_left: 0.0,
                safe_right: 0.0,
                center_reserved: None,
            })
        }

        fn present(&mut self, scene: &super::BarScene) -> Result<(), crate::error::BarrsError> {
            self.scenes.lock().expect("scenes").push(scene.clone());
            Ok(())
        }
    }

    #[test]
    fn native_renderer_refreshes_layout_before_publishing_scene() {
        let scenes = Arc::new(Mutex::new(Vec::new()));
        let host = ChangingLayoutHost {
            calls: AtomicUsize::new(0),
            scenes: Arc::clone(&scenes),
        };
        let mut renderer = NativeRenderer::new(Box::new(host));
        renderer.initialize(&Config::default()).expect("initialize");

        renderer
            .render_item(&test_snapshot("clock", 0, Some("right"), "12:00"))
            .expect("render");

        let scenes = scenes.lock().expect("scenes");
        assert_eq!(scenes.last().expect("scene").bar_width, 240.0);
    }

    #[test]
    fn native_renderer_republishes_scene_when_polled_layout_changes() {
        let scenes = Arc::new(Mutex::new(Vec::new()));
        let host = ChangingLayoutHost {
            calls: AtomicUsize::new(0),
            scenes: Arc::clone(&scenes),
        };
        let mut renderer = NativeRenderer::new(Box::new(host));
        renderer.initialize(&Config::default()).expect("initialize");

        let events = renderer.drain_events().expect("drain events");

        assert!(events.is_empty());
        let scenes = scenes.lock().expect("scenes");
        assert_eq!(scenes.last().expect("scene").bar_width, 240.0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn display_reconfiguration_callback_marks_relevant_changes_pending() {
        let pending = AtomicBool::new(false);

        unsafe {
            super::display_reconfiguration_callback(
                1,
                super::CGDisplayChangeSummaryFlags::MovedFlag,
                &pending as *const AtomicBool as *mut std::ffi::c_void,
            );
        }
        assert!(pending.load(Ordering::Acquire));

        pending.store(false, Ordering::Release);
        unsafe {
            super::display_reconfiguration_callback(
                1,
                super::CGDisplayChangeSummaryFlags::BeginConfigurationFlag,
                &pending as *const AtomicBool as *mut std::ffi::c_void,
            );
        }
        assert!(!pending.load(Ordering::Acquire));
    }

    #[test]
    fn layout_handles_missing_sections_and_zero_spacing() {
        let mut renderer = NativeRenderer::new(Box::new(MockNativeHost::default()));
        renderer
            .initialize(&Config {
                bar: crate::config::BarConfig {
                    spacing: 0,
                    ..crate::config::BarConfig::default()
                },
                ..Config::default()
            })
            .expect("initialize");
        renderer.state.set_layout_geometry(super::LayoutGeometry {
            bar_width: 200.0,
            safe_left: 0.0,
            safe_right: 0.0,
            center_reserved: None,
        });

        renderer
            .render_item(&test_snapshot("left-only", 0, Some("left"), "L"))
            .expect("render left");

        let state = renderer.surface_state();
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].snapshot.id, "left-only");
        assert_eq!(state.items[0].frame.x, 0.0);
    }

    #[test]
    fn layout_keeps_wide_middle_section_outside_reserved_gap() {
        let mut renderer = NativeRenderer::new(Box::new(MockNativeHost::default()));
        renderer
            .initialize(&Config {
                bar: crate::config::BarConfig {
                    spacing: 4,
                    ..crate::config::BarConfig::default()
                },
                ..Config::default()
            })
            .expect("initialize");
        renderer.state.set_layout_geometry(super::LayoutGeometry {
            bar_width: 120.0,
            safe_left: 0.0,
            safe_right: 0.0,
            center_reserved: Some(super::CenterReservedRange {
                start: 50.0,
                end: 70.0,
            }),
        });

        for index in 0..4 {
            renderer
                .render_item(&test_snapshot(
                    &format!("middle-{index}"),
                    index,
                    Some("middle"),
                    "ABCD",
                ))
                .expect("render middle");
        }

        for item in &renderer.surface_state().items {
            let frame = &item.frame;
            assert!(frame.x >= 0.0);
            assert!(frame.x + frame.width <= 120.0);
            assert!(
                frame.x + frame.width <= 50.0 || frame.x >= 70.0,
                "{} overlaps the reserved center gap: {:?}",
                item.snapshot.id,
                frame
            );
        }
    }

    #[test]
    fn updating_percentage_item_does_not_shift_following_items() {
        let mut renderer = NativeRenderer::new(Box::new(MockNativeHost::default()));
        renderer
            .initialize(&Config {
                bar: crate::config::BarConfig {
                    spacing: 0,
                    background: None,
                    ..crate::config::BarConfig::default()
                },
                ..Config::default()
            })
            .expect("initialize");

        let cpu = RenderItemSnapshot {
            id: "cpu".into(),
            order: 0,
            label: None,
            icon: Some("cpu".into()),
            placement: Some("left".into()),
            text: "9%".into(),
            hover: None,
            interactive: InteractiveSnapshot {
                click: false,
                right_click: false,
                scroll: false,
                hover: false,
            },
            data: json!({ "text": "9%", "usage_percent": 9 }),
        };
        let gpu = RenderItemSnapshot {
            id: "gpu".into(),
            order: 1,
            label: None,
            icon: Some("gpu".into()),
            placement: Some("left".into()),
            text: "27%".into(),
            hover: None,
            interactive: InteractiveSnapshot {
                click: false,
                right_click: false,
                scroll: false,
                hover: false,
            },
            data: json!({ "text": "27%", "utilization_percent": 27 }),
        };

        renderer.render_item(&cpu).expect("render cpu");
        renderer.render_item(&gpu).expect("render gpu");
        let gpu_x = renderer
            .surface_state()
            .items
            .iter()
            .find(|item| item.snapshot.id == "gpu")
            .expect("gpu")
            .frame
            .x;

        let mut updated_cpu = cpu;
        updated_cpu.text = "10%".into();
        updated_cpu.data = json!({ "text": "10%", "usage_percent": 10 });
        renderer.render_item(&updated_cpu).expect("update cpu");

        let updated_gpu_x = renderer
            .surface_state()
            .items
            .iter()
            .find(|item| item.snapshot.id == "gpu")
            .expect("gpu")
            .frame
            .x;
        assert_eq!(updated_gpu_x, gpu_x);
    }

    #[test]
    fn workspace_width_matches_segment_layout() {
        let snapshot = RenderItemSnapshot {
            id: "workspaces".into(),
            order: 0,
            label: None,
            icon: Some("spaces".into()),
            placement: Some("left".into()),
            text: "1 2 3".into(),
            hover: None,
            interactive: InteractiveSnapshot {
                click: false,
                right_click: false,
                scroll: false,
                hover: false,
            },
            data: json!({
                "text": "1 2 3",
                "workspaces": [
                    { "name": "1", "is_current": true, "has_windows": true },
                    { "name": "2", "is_current": false, "has_windows": true },
                    { "name": "3", "is_current": false, "has_windows": false }
                ]
            }),
        };

        let width = super::measure_item_width(&snapshot);
        let expected = (super::ITEM_HORIZONTAL_PADDING * 2.0)
            + super::ICON_WIDTH
            + super::ICON_TEXT_SPACING
            + (3.0 * super::CHARACTER_WIDTH)
            + (2.0 * super::DEFAULT_ITEM_SPACING);

        assert_eq!(width, expected);
    }

    #[test]
    fn native_renderer_publishes_hover_scene() {
        let mut renderer = NativeRenderer::new(Box::new(MockNativeHost::default()));
        renderer
            .initialize(&Config {
                bar: crate::config::BarConfig {
                    spacing: 4,
                    background: None,
                    ..crate::config::BarConfig::default()
                },
                ..Config::default()
            })
            .expect("initialize");
        renderer
            .render_item(&RenderItemSnapshot {
                id: "time".into(),
                order: 0,
                label: Some("time".into()),
                icon: None,
                placement: Some("right".into()),
                text: "12:34".into(),
                hover: Some(HoverSurface {
                    tooltip: Some("Current time".into()),
                    detail: Some("Thursday".into()),
                }),
                interactive: InteractiveSnapshot {
                    click: false,
                    right_click: false,
                    scroll: false,
                    hover: true,
                },
                data: json!({ "text": "12:34" }),
            })
            .expect("render");
        renderer
            .handle_event(&EventPayload {
                item_id: "time".into(),
                event: EventKind::HoverEnter,
                timestamp_ms: 0,
                mouse: MouseState {
                    x: 30,
                    y: 5,
                    button: None,
                    scroll_delta: None,
                },
                modifiers: Modifiers::default(),
            })
            .expect("hover");

        let scene = renderer.state.scene();
        let hover = scene.hover.expect("hover scene");
        assert_eq!(hover.item_id, "time");
        assert_eq!(hover.tooltip.as_deref(), Some("Current time"));
        assert_eq!(hover.detail.as_deref(), Some("Thursday"));
    }

    #[test]
    fn parses_hex_background_colors() {
        assert_eq!(parse_hex_color("#000000"), Some((0.0, 0.0, 0.0, 1.0)));
        assert_eq!(parse_hex_color("#FFFFFFFF"), Some((1.0, 1.0, 1.0, 1.0)));
        assert!(parse_hex_color("000000").is_none());
        assert!(parse_hex_color("#12345").is_none());
    }

    #[test]
    fn mock_host_stores_last_scene() {
        let mut host = MockNativeHost::default();
        let scene = super::BarScene {
            bar_height: 28.0,
            bar_width: 100.0,
            items: Vec::new(),
            hover: None,
        };
        host.present(&scene).expect("present");
        assert_eq!(host.last_scene().expect("scene").bar_height, 28.0);
        assert!(matches!(
            host.last_commands(),
            [HostCommand::ConfigureWindow(_)]
        ));
        assert_eq!(host.runtime().window.as_ref().expect("window").height, 28.0);
    }

    #[test]
    fn host_scene_plan_builds_hover_panel_and_layers() {
        let scene = super::BarScene {
            bar_height: 28.0,
            bar_width: 100.0,
            items: vec![super::PositionedItemSnapshot {
                snapshot: RenderItemSnapshot {
                    id: "cpu".into(),
                    order: 0,
                    label: Some("CPU".into()),
                    icon: None,
                    placement: Some("left".into()),
                    text: "10%".into(),
                    hover: Some(HoverSurface {
                        tooltip: Some("CPU".into()),
                        detail: Some("Usage".into()),
                    }),
                    interactive: InteractiveSnapshot {
                        click: false,
                        right_click: false,
                        scroll: false,
                        hover: true,
                    },
                    data: json!({ "text": "10%" }),
                },
                frame: super::ItemFrame {
                    x: 10.0,
                    width: 48.0,
                    height: 28.0,
                },
            }],
            hover: Some(super::HoverPresentation {
                item_id: "cpu".into(),
                anchor_x: 34.0,
                anchor_y: 28.0,
                tooltip: Some("CPU".into()),
                detail: Some("Usage".into()),
            }),
        };

        let plan = host_scene_plan(&scene);
        assert_eq!(plan.item_layers.len(), 1);
        assert_eq!(plan.window.height, 28.0);
        assert_eq!(plan.hover_panel.expect("hover").item_id, "cpu");
    }

    #[test]
    fn host_scene_diff_emits_remove_and_hover_hide() {
        let previous = super::HostScenePlan {
            window: super::WindowFrame {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 28.0,
            },
            item_layers: vec![super::ItemLayerPlan {
                item_id: "cpu".into(),
                frame: super::ItemFrame {
                    x: 10.0,
                    width: 40.0,
                    height: 28.0,
                },
                icon: None,
                text_segments: vec![super::TextLayerPlan {
                    value: "10%".into(),
                    x: 22.0,
                    y: 14.0,
                    tone: super::TextTone::Secondary,
                }],
                hoverable: true,
            }],
            hover_panel: Some(super::HoverPanelPlan {
                item_id: "cpu".into(),
                anchor_x: 30.0,
                anchor_y: 28.0,
                title: Some("CPU".into()),
                detail: None,
            }),
        };
        let next = super::HostScenePlan {
            window: super::WindowFrame {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 28.0,
            },
            item_layers: Vec::new(),
            hover_panel: None,
        };

        let commands = diff_host_scene(Some(&previous), &next);
        assert!(commands.contains(&HostCommand::RemoveItemLayer {
            item_id: "cpu".into()
        }));
        assert!(commands.contains(&HostCommand::HideHoverPanel));
    }

    #[test]
    fn host_runtime_applies_layer_and_hover_commands() {
        let mut runtime = HostRuntimeState::default();
        runtime.apply(&[
            HostCommand::ConfigureWindow(super::WindowFrame {
                x: 0.0,
                y: 0.0,
                width: 160.0,
                height: 28.0,
            }),
            HostCommand::UpsertItemLayer(LayerMutation {
                item_id: "cpu".into(),
                layer: super::ItemLayerPlan {
                    item_id: "cpu".into(),
                    frame: super::ItemFrame {
                        x: 10.0,
                        width: 40.0,
                        height: 28.0,
                    },
                    icon: None,
                    text_segments: vec![super::TextLayerPlan {
                        value: "10%".into(),
                        x: 22.0,
                        y: 14.0,
                        tone: super::TextTone::Secondary,
                    }],
                    hoverable: true,
                },
                is_new: true,
            }),
            HostCommand::ShowHoverPanel(super::HoverPanelPlan {
                item_id: "cpu".into(),
                anchor_x: 30.0,
                anchor_y: 28.0,
                title: Some("CPU".into()),
                detail: Some("Usage".into()),
            }),
        ]);

        assert_eq!(runtime.window.as_ref().expect("window").width, 160.0);
        assert!(runtime.item_layers.contains_key("cpu"));
        assert_eq!(
            runtime
                .hover_panel
                .as_ref()
                .and_then(|panel| panel.title.as_deref()),
            Some("CPU")
        );

        runtime.apply(&[
            HostCommand::RemoveItemLayer {
                item_id: "cpu".into(),
            },
            HostCommand::HideHoverPanel,
        ]);

        assert!(!runtime.item_layers.contains_key("cpu"));
        assert!(runtime.hover_panel.is_none());
    }

    #[test]
    fn host_diff_marks_new_layers() {
        let next = super::HostScenePlan {
            window: super::WindowFrame {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 28.0,
            },
            item_layers: vec![super::ItemLayerPlan {
                item_id: "cpu".into(),
                frame: super::ItemFrame {
                    x: 10.0,
                    width: 40.0,
                    height: 28.0,
                },
                icon: None,
                text_segments: vec![super::TextLayerPlan {
                    value: "10%".into(),
                    x: 22.0,
                    y: 14.0,
                    tone: super::TextTone::Secondary,
                }],
                hoverable: true,
            }],
            hover_panel: None,
        };

        let commands = diff_host_scene(None, &next);
        assert!(commands.iter().any(|command| matches!(
            command,
            HostCommand::UpsertItemLayer(LayerMutation { item_id, is_new: true, .. })
            if item_id == "cpu"
        )));
    }
}
