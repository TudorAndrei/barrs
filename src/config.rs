use std::fs;
use std::path::{Path, PathBuf};

use mlua::{Function, Lua, LuaSerdeExt, Value};
use serde::{Deserialize, Serialize};

use crate::error::BarrsError;
use crate::ipc::DEFAULT_SOCKET_PATH;

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
        self.socket_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            socket_path: Some(PathBuf::from(DEFAULT_SOCKET_PATH)),
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
}

fn default_spacing() -> u32 {
    6
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            spacing: default_spacing(),
            background: None,
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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Cpu,
    Time,
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
        config.socket_path = Some(PathBuf::from(DEFAULT_SOCKET_PATH));
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

    use super::{PluginKind, load_config};

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
        assert_eq!(
            config.items[0].plugin.as_ref().expect("plugin").kind,
            PluginKind::Time
        );
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
}
