//! Typed data source of each bar item.
//!
//! The config schema keeps `plugin = { kind, format, update }` (ADR-0020).
//! [`ItemSource::from_config`] converts that binding once, after
//! deserialization, so a source cannot hold a field that its kind rejects.

use std::time::Duration;

use serde_json::json;

use crate::config::{PluginBinding, PluginKind};
use crate::content::ItemContent;
use crate::error::BarrsError;
use crate::plugin;
use crate::rift::{RiftBackendKind, RiftSnapshot};

const DEFAULT_DATE_FORMAT: &str = "%Y-%m-%d";
const LUA_UPDATE_INTERVAL: Duration = Duration::from_secs(5);
const RIFT_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Where the data of one bar item comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemSource {
    /// A built-in system query that runs in the daemon process.
    System(SystemPlugin),
    /// A built-in view of the Rift window manager state.
    Rift(RiftPlugin),
    /// A global Lua function that the Lua executor calls.
    Lua { update: String },
    /// Fixed text and icon from the item config.
    Label { text: String, icon: Option<String> },
}

/// Built-in system query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemPlugin {
    Cpu,
    Time,
    Date { format: String },
    Battery,
    Gpu,
}

/// Built-in Rift view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiftPlugin {
    Workspaces,
    Layout,
}

impl ItemSource {
    /// Converts the deserialized plugin binding of item `item_id`.
    ///
    /// An item without a plugin shows its label, or its id when the label is
    /// absent.
    ///
    /// # Errors
    ///
    /// Returns [`BarrsError::InvalidConfig`] when a non-Lua binding sets
    /// `update`, a Lua binding has no `update`, or a date format contains NUL.
    /// Only date bindings use `format`; other kinds ignore it.
    pub fn from_config(
        item_id: &str,
        label: Option<&str>,
        icon: Option<&str>,
        binding: Option<PluginBinding>,
    ) -> Result<Self, BarrsError> {
        let Some(PluginBinding {
            kind,
            format,
            update,
        }) = binding
        else {
            return Ok(Self::Label {
                text: label.unwrap_or(item_id).to_owned(),
                icon: icon.map(str::to_owned),
            });
        };
        if kind != PluginKind::Lua && update.is_some() {
            return Err(BarrsError::InvalidConfig(format!(
                "item {item_id} sets update, which only lua plugins accept"
            )));
        }
        let source = match kind {
            PluginKind::Cpu => Self::System(SystemPlugin::Cpu),
            PluginKind::Time => Self::System(SystemPlugin::Time),
            PluginKind::Battery => Self::System(SystemPlugin::Battery),
            PluginKind::Gpu => Self::System(SystemPlugin::Gpu),
            PluginKind::Date => {
                let format = format.unwrap_or_else(|| DEFAULT_DATE_FORMAT.to_owned());
                if format.contains('\0') {
                    return Err(BarrsError::InvalidConfig(format!(
                        "date format for item {item_id} must not contain NUL bytes"
                    )));
                }
                Self::System(SystemPlugin::Date { format })
            }
            PluginKind::RiftWorkspaces => Self::Rift(RiftPlugin::Workspaces),
            PluginKind::RiftLayout => Self::Rift(RiftPlugin::Layout),
            PluginKind::Lua => match update {
                None => {
                    return Err(BarrsError::InvalidConfig(format!(
                        "lua plugin for item {item_id} must set update to a global function name"
                    )));
                }
                Some(update) if update.trim().is_empty() => {
                    return Err(BarrsError::InvalidConfig(format!(
                        "lua plugin for item {item_id} must not have an empty update"
                    )));
                }
                Some(update) => Self::Lua { update },
            },
        };
        Ok(source)
    }

    /// Refresh interval when the item config sets no `interval`.
    ///
    /// `None` means the item is not polled. Rift items are polled only on the
    /// CLI backend; the Mach backend pushes events instead.
    #[must_use]
    pub fn default_interval(&self, backend: RiftBackendKind) -> Option<Duration> {
        match self {
            Self::System(SystemPlugin::Time) => Some(Duration::from_secs(1)),
            Self::System(SystemPlugin::Date { .. }) => Some(Duration::from_mins(30)),
            Self::System(SystemPlugin::Cpu | SystemPlugin::Gpu) => Some(Duration::from_secs(2)),
            Self::System(SystemPlugin::Battery) => Some(Duration::from_secs(10)),
            Self::Rift(_) => (backend == RiftBackendKind::Cli).then_some(RIFT_POLL_INTERVAL),
            Self::Lua { .. } => Some(LUA_UPDATE_INTERVAL),
            Self::Label { .. } => None,
        }
    }

    /// Whether producing the data reads a Rift snapshot.
    #[must_use]
    pub const fn needs_rift(&self) -> bool {
        matches!(self, Self::Rift(_))
    }

    /// The global Lua function that produces the data, for a Lua source.
    ///
    /// The daemon sends this function to the Lua executor instead of calling
    /// [`Self::content`].
    #[must_use]
    pub fn lua_update(&self) -> Option<&str> {
        match self {
            Self::Lua { update } => Some(update),
            _ => None,
        }
    }

    /// Produces the content that the daemon can show now.
    ///
    /// A Lua source returns unavailable content, which stays until the
    /// executor returns the first update. A Rift source without `rift` also
    /// returns unavailable content.
    ///
    /// # Errors
    ///
    /// Returns an error when the local time or date cannot be formatted.
    pub fn content(&self, rift: Option<&RiftSnapshot>) -> Result<ItemContent, BarrsError> {
        let content = match self {
            Self::System(SystemPlugin::Cpu) => plugin::cpu(),
            Self::System(SystemPlugin::Time) => plugin::time()?,
            Self::System(SystemPlugin::Date { format }) => plugin::date(format)?,
            Self::System(SystemPlugin::Battery) => plugin::battery(),
            Self::System(SystemPlugin::Gpu) => plugin::gpu(),
            Self::Rift(RiftPlugin::Workspaces) => plugin::rift_workspaces(rift),
            Self::Rift(RiftPlugin::Layout) => plugin::rift_layout(rift),
            Self::Lua { .. } => ItemContent::Unavailable,
            Self::Label { text, icon } => {
                ItemContent::plain_with_fields(text.clone(), [("icon", json!(icon))])
            }
        };
        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use crate::config::{PluginBinding, PluginKind};
    use crate::content::{ItemContent, Workspace};
    use crate::error::BarrsError;
    use crate::rift::{RiftBackendKind, RiftSnapshot, RiftWorkspace};

    use super::{ItemSource, RiftPlugin, SystemPlugin};

    fn binding(kind: PluginKind) -> PluginBinding {
        PluginBinding {
            kind,
            format: None,
            update: None,
        }
    }

    fn source(binding: Option<PluginBinding>) -> Result<ItemSource, BarrsError> {
        ItemSource::from_config("item", Some("label"), Some("icon"), binding)
    }

    fn date(format: &str) -> ItemSource {
        ItemSource::System(SystemPlugin::Date {
            format: format.into(),
        })
    }

    fn lua() -> ItemSource {
        ItemSource::Lua {
            update: "usage".into(),
        }
    }

    fn label() -> ItemSource {
        ItemSource::Label {
            text: "label".into(),
            icon: None,
        }
    }

    #[test]
    fn converts_plugin_bindings() {
        let cases = [
            (None, label_with_icon()),
            (
                Some(binding(PluginKind::Cpu)),
                ItemSource::System(SystemPlugin::Cpu),
            ),
            (
                Some(binding(PluginKind::Time)),
                ItemSource::System(SystemPlugin::Time),
            ),
            (Some(binding(PluginKind::Date)), date("%Y-%m-%d")),
            (
                Some(PluginBinding {
                    format: Some("%Y".into()),
                    ..binding(PluginKind::Date)
                }),
                date("%Y"),
            ),
            (
                Some(binding(PluginKind::Battery)),
                ItemSource::System(SystemPlugin::Battery),
            ),
            (
                Some(binding(PluginKind::Gpu)),
                ItemSource::System(SystemPlugin::Gpu),
            ),
            (
                Some(binding(PluginKind::RiftWorkspaces)),
                ItemSource::Rift(RiftPlugin::Workspaces),
            ),
            (
                Some(binding(PluginKind::RiftLayout)),
                ItemSource::Rift(RiftPlugin::Layout),
            ),
            (
                Some(PluginBinding {
                    update: Some("usage".into()),
                    ..binding(PluginKind::Lua)
                }),
                lua(),
            ),
        ];
        for (binding, expected) in cases {
            assert_eq!(
                source(binding.clone()).expect("valid"),
                expected,
                "{binding:?}"
            );
        }
    }

    fn label_with_icon() -> ItemSource {
        ItemSource::Label {
            text: "label".into(),
            icon: Some("icon".into()),
        }
    }

    #[test]
    fn label_source_falls_back_to_item_id() {
        assert_eq!(
            ItemSource::from_config("clock", None, None, None).expect("valid"),
            ItemSource::Label {
                text: "clock".into(),
                icon: None,
            }
        );
    }

    #[test]
    fn rejects_fields_that_the_kind_does_not_accept() {
        let cases = [
            (binding(PluginKind::Lua), "must set update"),
            (
                PluginBinding {
                    update: Some(" ".into()),
                    ..binding(PluginKind::Lua)
                },
                "must not have an empty update",
            ),
            (
                PluginBinding {
                    update: Some("usage".into()),
                    ..binding(PluginKind::Cpu)
                },
                "only lua plugins accept",
            ),
            (
                PluginBinding {
                    format: Some("%Y\0%m".into()),
                    ..binding(PluginKind::Date)
                },
                "must not contain NUL bytes",
            ),
        ];
        for (binding, expected) in cases {
            let error = source(Some(binding.clone())).expect_err("invalid binding");
            assert!(
                matches!(&error, BarrsError::InvalidConfig(message)
                    if message.contains("item item") && message.contains(expected)),
                "{binding:?}: {error}"
            );
        }
    }

    #[test]
    fn default_intervals_depend_on_the_source_and_backend() {
        let secs = |value| Some(Duration::from_secs(value));
        let cases = [
            (ItemSource::System(SystemPlugin::Time), secs(1), secs(1)),
            (ItemSource::System(SystemPlugin::Cpu), secs(2), secs(2)),
            (ItemSource::System(SystemPlugin::Gpu), secs(2), secs(2)),
            (
                ItemSource::System(SystemPlugin::Battery),
                secs(10),
                secs(10),
            ),
            (date("%Y"), secs(1800), secs(1800)),
            (lua(), secs(5), secs(5)),
            (
                ItemSource::Rift(RiftPlugin::Workspaces),
                Some(Duration::from_millis(250)),
                None,
            ),
            (
                ItemSource::Rift(RiftPlugin::Layout),
                Some(Duration::from_millis(250)),
                None,
            ),
            (label(), None, None),
        ];
        for (source, cli, mach) in cases {
            assert_eq!(
                source.default_interval(RiftBackendKind::Cli),
                cli,
                "{source:?}"
            );
            assert_eq!(
                source.default_interval(RiftBackendKind::Mach),
                mach,
                "{source:?}"
            );
        }
    }

    #[test]
    fn only_rift_sources_need_rift_and_only_lua_sources_have_an_update() {
        let cases = [
            (ItemSource::System(SystemPlugin::Time), false, None),
            (ItemSource::System(SystemPlugin::Cpu), false, None),
            (date("%Y"), false, None),
            (ItemSource::Rift(RiftPlugin::Workspaces), true, None),
            (ItemSource::Rift(RiftPlugin::Layout), true, None),
            (lua(), false, Some("usage")),
            (label(), false, None),
        ];
        for (source, needs_rift, update) in cases {
            assert_eq!(source.needs_rift(), needs_rift, "{source:?}");
            assert_eq!(source.lua_update(), update, "{source:?}");
        }
    }

    #[test]
    fn placeholder_sources_report_unavailable() {
        for source in [
            lua(),
            ItemSource::Rift(RiftPlugin::Workspaces),
            ItemSource::Rift(RiftPlugin::Layout),
        ] {
            assert_eq!(
                source.content(None).expect("content"),
                ItemContent::Unavailable,
                "{source:?}"
            );
        }
    }

    #[test]
    fn label_source_shows_its_text_and_icon() {
        assert_eq!(
            label_with_icon().content(None).expect("content"),
            ItemContent::plain_with_fields("label", [("icon", json!("icon"))])
        );
    }

    #[test]
    fn system_sources_report_current_values() {
        let cpu = ItemSource::System(SystemPlugin::Cpu)
            .content(None)
            .expect("cpu")
            .data();
        let text = cpu["text"].as_str().expect("cpu text");
        assert!(text.ends_with('%') || text == "—");

        let time = ItemSource::System(SystemPlugin::Time)
            .content(None)
            .expect("time")
            .data();
        let text = time["text"].as_str().expect("time text");
        assert_eq!(text.len(), 5);
        assert_eq!(text.chars().nth(2), Some(':'));
        assert_eq!(time["timezone"], "local");

        let date = date("%Y").content(None).expect("date").data();
        let text = date["text"].as_str().expect("date text");
        assert_eq!(text.len(), 4);
        assert!(text.chars().all(|ch| ch.is_ascii_digit()));
        assert_eq!(date["timezone"], "local");
        assert_eq!(date["format"], "%Y");
    }

    #[test]
    fn date_source_returns_formatting_error() {
        let error = date(&"x".repeat(256))
            .content(None)
            .expect_err("format exceeds output buffer");
        assert!(matches!(
            error,
            BarrsError::Unsupported(message) if message == "failed to format local time"
        ));
    }

    #[test]
    fn rift_sources_read_the_rift_snapshot() {
        let workspace = |name: &str, is_current, window_count| RiftWorkspace {
            name: name.into(),
            is_current,
            window_count,
        };
        let rift = RiftSnapshot {
            current_workspace: "2".into(),
            workspaces: vec![
                workspace("1", false, 3),
                workspace("2", true, 4),
                workspace("3", false, 0),
            ],
            layout: "bsp".into(),
            window_count: 4,
        };

        let entry = |name: &str, is_current, window_count| Workspace {
            name: name.into(),
            is_current,
            window_count,
        };
        assert_eq!(
            ItemSource::Rift(RiftPlugin::Workspaces)
                .content(Some(&rift))
                .expect("workspaces"),
            ItemContent::Workspaces {
                current: "2".into(),
                workspaces: vec![
                    entry("1", false, 3),
                    entry("2", true, 4),
                    entry("3", false, 0),
                ],
            }
        );
        assert_eq!(
            ItemSource::Rift(RiftPlugin::Layout)
                .content(Some(&rift))
                .expect("layout"),
            ItemContent::plain_with_fields("bsp", [("window_count", json!(4))])
        );
    }
}
