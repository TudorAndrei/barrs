use std::fs;
use std::path::{Path, PathBuf};

use mlua::{Function, Lua, LuaSerdeExt, Value};
use serde::{Deserialize, Serialize};

use crate::error::BarrsError;
use crate::ipc::default_socket_path;
use crate::source::ItemSource;

/// Validated configuration in which every item has a typed [`ItemSource`].
#[derive(Debug, Clone)]
pub struct Config {
    pub socket_path: Option<PathBuf>,
    pub bar: BarConfig,
    pub items: Vec<ItemConfig>,
}

/// Lua schema of the whole config, as written in the config.
#[derive(Deserialize)]
struct ConfigDocument {
    #[serde(default)]
    socket_path: Option<PathBuf>,
    #[serde(default)]
    bar: BarConfig,
    #[serde(default)]
    items: Vec<ItemDocument>,
}

impl TryFrom<ConfigDocument> for Config {
    type Error = BarrsError;

    fn try_from(document: ConfigDocument) -> Result<Self, Self::Error> {
        Ok(Self {
            socket_path: document.socket_path,
            bar: document.bar,
            items: document
                .items
                .into_iter()
                .map(ItemConfig::try_from)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl Config {
    pub fn socket_path(&self) -> PathBuf {
        self.socket_path.clone().unwrap_or_else(default_socket_path)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            socket_path: Some(default_socket_path()),
            bar: BarConfig::default(),
            items: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarConfig {
    #[serde(default = "default_spacing")]
    pub spacing: u32,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default = "default_notch_width")]
    pub notch_width: u32,
    #[serde(default = "default_notch_padding")]
    pub notch_padding: u32,
    #[serde(default)]
    pub notch_offset: u32,
    #[serde(default)]
    pub notch_display_height: u32,
}

const fn default_spacing() -> u32 {
    6
}

const fn default_notch_width() -> u32 {
    200
}

const fn default_notch_padding() -> u32 {
    8
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            spacing: default_spacing(),
            background: None,
            notch_width: default_notch_width(),
            notch_padding: default_notch_padding(),
            notch_offset: 0,
            notch_display_height: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarSection {
    Left,
    Middle,
    Right,
}

impl BarSection {
    pub fn from_placement(placement: Option<&str>) -> Option<Self> {
        match placement.map(str::trim).filter(|value| !value.is_empty()) {
            None | Some("left") => Some(Self::Left),
            Some("center" | "middle") => Some(Self::Middle),
            Some("right") => Some(Self::Right),
            Some(_) => None,
        }
    }

    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Left => 0,
            Self::Middle => 1,
            Self::Right => 2,
        }
    }
}

/// One bar item after its plugin binding is converted to an [`ItemSource`].
#[derive(Debug, Clone)]
pub struct ItemConfig {
    pub id: String,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub placement: Option<String>,
    pub interval: Option<u64>,
    pub source: ItemSource,
    pub hover: Option<HoverConfig>,
    pub handlers: ItemHandlers,
}

/// Lua schema of one item, as written in the config.
#[derive(Deserialize)]
struct ItemDocument {
    id: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    placement: Option<String>,
    #[serde(default, alias = "refresh_secs")]
    interval: Option<u64>,
    #[serde(default)]
    plugin: Option<PluginBinding>,
    #[serde(default)]
    hover: Option<HoverConfig>,
    #[serde(default)]
    handlers: ItemHandlers,
}

impl TryFrom<ItemDocument> for ItemConfig {
    type Error = BarrsError;

    fn try_from(document: ItemDocument) -> Result<Self, Self::Error> {
        let source = ItemSource::from_config(
            &document.id,
            document.label.as_deref(),
            document.icon.as_deref(),
            document.plugin,
        )?;
        Ok(Self {
            id: document.id,
            label: document.label,
            icon: document.icon,
            placement: document.placement,
            interval: document.interval,
            source,
            hover: document.hover,
            handlers: document.handlers,
        })
    }
}

/// Lua schema of an item's `plugin` table, before [`ItemSource`] conversion.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginBinding {
    pub kind: PluginKind,
    #[serde(default)]
    pub format: Option<String>,
    /// Global Lua function that a `lua` plugin calls to produce the item text.
    #[serde(default)]
    pub update: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Cpu,
    Time,
    Date,
    Battery,
    Gpu,
    RiftWorkspaces,
    RiftLayout,
    Lua,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoverConfig {
    #[serde(default)]
    pub tooltip: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ItemHandlers {
    #[serde(default)]
    pub click: Option<String>,
    #[serde(default)]
    pub right_click: Option<String>,
    #[serde(default)]
    pub scroll: Option<String>,
    #[serde(default)]
    pub hover_enter: Option<String>,
    #[serde(default)]
    pub hover_leave: Option<String>,
    #[serde(default)]
    pub hover_update: Option<String>,
}

/// Loads the Lua config at `path` and keeps its Lua runtime.
///
/// # Errors
///
/// Returns an error when the file cannot be read, the Lua code fails, or the
/// config is not valid.
pub fn load_config_with_runtime(path: &Path) -> Result<(Config, Lua), BarrsError> {
    let source = fs::read_to_string(path)?;
    let lua = Lua::new();
    let value: Value = lua.load(&source).set_name(path.to_string_lossy()).eval()?;
    let document: ConfigDocument = lua.from_value(value)?;
    let mut config = Config::try_from(document)?;
    if config.socket_path.is_none() {
        config.socket_path = Some(default_socket_path());
    }
    validate_config(&config)?;
    validate_handlers(&lua, &config)?;
    Ok((config, lua))
}

/// Loads and validates the Lua config at `path`.
///
/// # Errors
///
/// Returns an error when the file cannot be read, the Lua code fails, or the
/// config is not valid.
pub fn load_config(path: &Path) -> Result<Config, BarrsError> {
    load_config_with_runtime(path).map(|(config, _)| config)
}

/// Checks item ids and placements.
///
/// # Errors
///
/// Returns [`BarrsError::InvalidConfig`] for the first invalid value.
pub fn validate_config(config: &Config) -> Result<(), BarrsError> {
    if config.items.is_empty() {
        return Err(BarrsError::InvalidConfig(
            "config must define at least one item".into(),
        ));
    }
    for item in &config.items {
        if item.id.trim().is_empty() {
            return Err(BarrsError::InvalidConfig(
                "item ids must not be empty".into(),
            ));
        }
        if BarSection::from_placement(item.placement.as_deref()).is_none() {
            return Err(BarrsError::InvalidConfig(format!(
                "item {} has unsupported placement {}; expected left, middle, center, or right",
                item.id,
                item.placement.as_deref().unwrap_or_default()
            )));
        }
    }
    let mut remaining = config.items.as_slice();
    while let Some((item, rest)) = remaining.split_first() {
        remaining = rest;
        if rest.iter().any(|candidate| candidate.id == item.id) {
            return Err(BarrsError::InvalidConfig(format!(
                "duplicate item id {}",
                item.id
            )));
        }
    }
    Ok(())
}

fn validate_handlers(lua: &Lua, config: &Config) -> Result<(), BarrsError> {
    let globals = lua.globals();
    for item in &config.items {
        for handler in item.handlers.names() {
            globals
                .get::<Function>(handler.as_str())
                .map_err(|_| BarrsError::InvalidConfig(format!("missing handler {handler}")))?;
        }
        if let Some(update) = item.source.lua_update() {
            globals.get::<Function>(update).map_err(|_| {
                BarrsError::InvalidConfig(format!("missing update function {update}"))
            })?;
        }
    }
    Ok(())
}

impl ItemHandlers {
    pub fn names(&self) -> impl Iterator<Item = &String> {
        [
            self.click.as_ref(),
            self.right_click.as_ref(),
            self.scroll.as_ref(),
            self.hover_enter.as_ref(),
            self.hover_leave.as_ref(),
            self.hover_update.as_ref(),
        ]
        .into_iter()
        .flatten()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use mlua::LuaSerdeExt;
    use tempfile::tempdir;

    use super::{BarConfig, BarSection, load_config};
    use crate::source::{ItemSource, SystemPlugin};

    #[test]
    fn loads_lua_config() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("barrs.lua");
        fs::write(
            &path,
            r#"
function handle_click(ctx)
  return true
end

return {
  bar = {},
  items = {
    {
      id = "clock",
      label = "clock",
      interval = 5,
      plugin = { kind = "time" },
      handlers = { click = "handle_click" },
      hover = { tooltip = "Current time" }
    }

  }
}
"#,
        )
        .expect("write config");

        let config = load_config(&path).expect("load config");
        assert_eq!(config.items.len(), 1);
        assert_eq!(config.bar.spacing, 6);
        assert_eq!(config.bar.notch_width, 200);
        assert_eq!(config.bar.notch_padding, 8);
        assert_eq!(config.bar.notch_offset, 0);
        assert_eq!(config.bar.notch_display_height, 0);
        assert_eq!(
            config.items[0].source,
            ItemSource::System(SystemPlugin::Time)
        );
    }

    #[test]
    fn bundled_lua_sample_handler_accepts_a_click_context() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("barrs.lua");
        let (config, lua) = super::load_config_with_runtime(&path).expect("sample config");
        assert!(config.items.iter().any(|item| item.id == "time"));

        let handler: mlua::Function = lua
            .globals()
            .get("record_time_event")
            .expect("sample handler");
        let payload =
            crate::ipc::EventPayload::from_trigger("time".into(), crate::cli::TriggerEvent::Click);
        handler
            .call::<()>(lua.to_value(&payload).expect("serialize context"))
            .expect("sample handler call");

        let event: String = lua
            .globals()
            .get("last_time_event")
            .expect("recorded event");
        assert_eq!(event, "click");
    }

    #[test]
    fn bar_config_uses_notch_defaults() {
        let bar = BarConfig::default();

        assert_eq!(bar.spacing, 6);
        assert_eq!(bar.notch_width, 200);
        assert_eq!(bar.notch_padding, 8);
        assert_eq!(bar.notch_offset, 0);
        assert_eq!(bar.notch_display_height, 0);
    }

    #[test]
    fn loads_bar_notch_settings() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("barrs.lua");
        fs::write(
            &path,
            r#"
return {
  bar = {
    notch_width = 240,
    notch_padding = 12,
    notch_offset = 3,
    notch_display_height = 32,
  },
  items = {
    {
      id = "clock",
      plugin = { kind = "time" }
    }
  }
}
"#,
        )
        .expect("write config");

        let config = load_config(&path).expect("notch config should load");
        assert_eq!(config.bar.notch_width, 240);
        assert_eq!(config.bar.notch_padding, 12);
        assert_eq!(config.bar.notch_offset, 3);
        assert_eq!(config.bar.notch_display_height, 32);
    }

    #[test]
    fn parses_bar_sections() {
        assert_eq!(BarSection::from_placement(None), Some(BarSection::Left));
        assert_eq!(
            BarSection::from_placement(Some("left")),
            Some(BarSection::Left)
        );
        assert_eq!(
            BarSection::from_placement(Some("middle")),
            Some(BarSection::Middle)
        );
        assert_eq!(
            BarSection::from_placement(Some("center")),
            Some(BarSection::Middle)
        );
        assert_eq!(
            BarSection::from_placement(Some("right")),
            Some(BarSection::Right)
        );
        assert_eq!(BarSection::from_placement(Some("floating")), None);
    }

    #[test]
    fn loads_lua_plugin_with_update_function() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("barrs.lua");
        fs::write(
            &path,
            r#"
function memory_usage(ctx) return "42%" end
return {
  items = {
    { id = "memory", plugin = { kind = "lua", update = "memory_usage" } },
  }
}
"#,
        )
        .expect("write config");

        let config = load_config(&path).expect("lua plugin should load");
        assert_eq!(config.items[0].source.lua_update(), Some("memory_usage"));
    }

    #[test]
    fn rejects_lua_plugins_without_a_usable_update_function() {
        let cases = [
            (r#"{ kind = "lua" }"#, "must set update"),
            (
                r#"{ kind = "lua", update = "missing" }"#,
                "missing update function missing",
            ),
        ];
        for (binding, expected) in cases {
            let dir = tempdir().expect("tempdir");
            let path = dir.path().join("barrs.lua");
            fs::write(
                &path,
                format!(
                    "function memory_usage(ctx) return \"42%\" end\n\
                     return {{ items = {{ {{ id = \"memory\", plugin = {binding} }} }} }}\n"
                ),
            )
            .expect("write config");

            let error = load_config(&path).expect_err(binding);
            assert!(error.to_string().contains(expected), "{binding}: {error}");
        }
    }

    #[test]
    fn loads_valid_item_placements() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("barrs.lua");
        fs::write(
            &path,
            r#"
return {
  items = {
    { id = "workspaces", placement = "left" },
    { id = "clock", placement = "middle" },
    { id = "date", placement = "center" },
    { id = "battery", placement = "right" },
  }
}
"#,
        )
        .expect("write config");

        let config = load_config(&path).expect("placements should load");
        assert_eq!(config.items.len(), 4);
    }

    #[test]
    fn rejects_invalid_item_placement() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("barrs.lua");
        fs::write(
            &path,
            r#"
return {
  items = {
    {
      id = "clock",
      placement = "floating"
    }
  }
}
"#,
        )
        .expect("write config");

        let error = load_config(&path).expect_err("invalid placement should fail");
        assert!(error.to_string().contains("clock"));
        assert!(error.to_string().contains("unsupported placement"));
        assert!(error.to_string().contains("floating"));
    }

    #[test]
    fn rejects_missing_handler() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("barrs.lua");
        fs::write(
            &path,
            r#"
return {
  items = {
    {
      id = "clock",
      plugin = { kind = "time" },
      handlers = { click = "missing_handler" }
    }
  }
}
"#,
        )
        .expect("write config");

        let error = load_config(&path).expect_err("missing handler should fail");
        assert!(error.to_string().contains("missing handler"));
    }

    #[test]
    fn allows_empty_bar_config() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("barrs.lua");
        fs::write(
            &path,
            r#"
return {
  bar = {},
  items = {
    {
      id = "clock",
      plugin = { kind = "time" }
    }
  }
}
"#,
        )
        .expect("write config");

        let config = load_config(&path).expect("empty bar config should load");
        assert!(config.bar.background.is_none());
    }

    #[test]
    fn loads_date_plugin_with_format() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("barrs.lua");
        fs::write(
            &path,
            r#"
return {
  items = {
    {
      id = "date",
      plugin = { kind = "date", format = "%Y-%m-%d" }
    }
  }
}
"#,
        )
        .expect("write config");

        let config = load_config(&path).expect("date config should load");
        assert_eq!(
            config.items[0].source,
            ItemSource::System(SystemPlugin::Date {
                format: "%Y-%m-%d".into()
            })
        );
    }
}
