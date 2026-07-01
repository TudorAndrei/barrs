use std::fs;
use std::path::{Path, PathBuf};

use mlua::{Function, Lua, LuaSerdeExt, Value};
use serde::{Deserialize, Serialize};

use crate::error::BarrsError;
use crate::ipc::default_socket_path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub socket_path: Option<PathBuf>,
    #[serde(default)]
    pub bar: BarConfig,
    #[serde(default)]
    pub items: Vec<ItemConfig>,
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
    #[serde(default)]
    pub notch_offset: u32,
    #[serde(default)]
    pub notch_display_height: u32,
}

fn default_spacing() -> u32 {
    6
}

fn default_notch_width() -> u32 {
    200
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            spacing: default_spacing(),
            background: None,
            notch_width: default_notch_width(),
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

    pub fn rank(self) -> u8 {
        match self {
            Self::Left => 0,
            Self::Middle => 1,
            Self::Right => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemConfig {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub placement: Option<String>,
    #[serde(default, alias = "refresh_secs")]
    pub interval: Option<u64>,
    #[serde(default)]
    pub plugin: Option<PluginBinding>,
    #[serde(default)]
    pub hover: Option<HoverConfig>,
    #[serde(default)]
    pub handlers: ItemHandlers,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginBinding {
    pub kind: PluginKind,
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Cpu,
    Time,
    Date,
    Battery,
    Gpu,
    RiftWorkspaces,
    RiftLayout,
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

pub fn load_config(path: &Path) -> Result<Config, BarrsError> {
    let source = fs::read_to_string(path)?;
    let lua = Lua::new();
    let value: Value = lua.load(&source).set_name(path.to_string_lossy()).eval()?;
    let mut config: Config = lua.from_value(value)?;
    if config.socket_path.is_none() {
        config.socket_path = Some(default_socket_path());
    }
    validate_config(&config)?;
    validate_handlers(&lua, &config)?;
    Ok(config)
}

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
    for (index, item) in config.items.iter().enumerate() {
        if config.items[index + 1..]
            .iter()
            .any(|candidate| candidate.id == item.id)
        {
            return Err(BarrsError::InvalidConfig(format!(
                "duplicate item id {}",
                item.id
            )));
        }
    }
    for item in &config.items {
        let Some(plugin) = &item.plugin else {
            continue;
        };
        if plugin.kind == PluginKind::Date
            && plugin
                .format
                .as_ref()
                .is_some_and(|format| format.contains('\0'))
        {
            return Err(BarrsError::InvalidConfig(format!(
                "date format for item {} must not contain NUL bytes",
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

    use tempfile::tempdir;

    use super::{BarConfig, BarSection, PluginKind, load_config};

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
        assert_eq!(config.bar.notch_offset, 0);
        assert_eq!(config.bar.notch_display_height, 0);
        assert_eq!(
            config.items[0].plugin.as_ref().expect("plugin").kind,
            PluginKind::Time
        );
    }

    #[test]
    fn bar_config_uses_notch_defaults() {
        let bar = BarConfig::default();

        assert_eq!(bar.spacing, 6);
        assert_eq!(bar.notch_width, 200);
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
        let plugin = config.items[0].plugin.as_ref().expect("plugin");
        assert_eq!(plugin.kind, PluginKind::Date);
        assert_eq!(plugin.format.as_deref(), Some("%Y-%m-%d"));
    }

    #[test]
    fn loads_date_plugin_without_format() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("barrs.lua");
        fs::write(
            &path,
            r#"
return {
  items = {
    {
      id = "date",
      plugin = { kind = "date" }
    }
  }
}
"#,
        )
        .expect("write config");

        let config = load_config(&path).expect("date config should load");
        let plugin = config.items[0].plugin.as_ref().expect("plugin");
        assert_eq!(plugin.kind, PluginKind::Date);
        assert!(plugin.format.is_none());
    }

    #[test]
    fn rejects_date_format_with_nul_byte() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("barrs.lua");
        fs::write(
            &path,
            r#"
return {
  items = {
    {
      id = "date",
      plugin = { kind = "date", format = "%Y\0%m" }
    }
  }
}
"#,
        )
        .expect("write config");

        let error = load_config(&path).expect_err("NUL date format should fail");
        assert!(error.to_string().contains("date format"));
        assert!(error.to_string().contains("NUL"));
    }
}
