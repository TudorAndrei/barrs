//! Typed content that item sources produce and the renderer draws.
//!
//! Producers (`src/plugin.rs`, the label source, and the Lua executor) return
//! an [`ItemContent`]. The renderer chooses the item layout from the variant,
//! not from JSON keys. [`ItemContent::data`] gives the JSON data that
//! `dump-state` shows for the item.

use std::borrow::Cow;

use serde::{Serialize, Serializer};
use serde_json::{Map, Value, json};

/// Text that an item shows when its source cannot report a value.
pub const PLACEHOLDER_TEXT: &str = "—";

/// What one bar item shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemContent {
    /// One text value. `fields` are extra values that only the item data
    /// shows, for example `usage_percent` or the other fields of a Lua table.
    Text {
        text: String,
        fields: Map<String, Value>,
    },
    /// The Rift workspaces, which the renderer draws as one segment each.
    Workspaces {
        current: String,
        workspaces: Vec<Workspace>,
    },
    /// The source cannot report a value now.
    Unavailable,
}

/// One Rift workspace in [`ItemContent::Workspaces`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub name: String,
    pub is_current: bool,
    /// Exact number of windows (ADR-0015).
    pub window_count: usize,
}

impl Workspace {
    /// Whether the workspace has at least one window.
    #[must_use]
    pub const fn has_windows(&self) -> bool {
        self.window_count > 0
    }
}

impl ItemContent {
    /// Text content without extra fields.
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            fields: Map::new(),
        }
    }

    /// Text content with extra data `fields`.
    #[must_use]
    pub fn plain_with_fields<'a>(
        text: impl Into<String>,
        fields: impl IntoIterator<Item = (&'a str, Value)>,
    ) -> Self {
        Self::Text {
            text: text.into(),
            fields: fields
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        }
    }

    /// The text of the item: the text value, the workspace names separated by
    /// spaces, or the placeholder.
    #[must_use]
    pub fn text(&self) -> Cow<'_, str> {
        match self {
            Self::Text { text, .. } => Cow::Borrowed(text),
            Self::Workspaces { workspaces, .. } => Cow::Owned(
                workspaces
                    .iter()
                    .map(|workspace| workspace.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            Self::Unavailable => Cow::Borrowed(PLACEHOLDER_TEXT),
        }
    }

    /// The JSON data of the item, as `dump-state` shows it.
    ///
    /// Every variant has a string `text`. Text content adds its fields, and
    /// unavailable content sets `available` to `false`.
    #[must_use]
    pub fn data(&self) -> Value {
        match self {
            Self::Text { text, fields } => {
                let mut data = fields.clone();
                data.insert("text".into(), Value::String(text.clone()));
                Value::Object(data)
            }
            Self::Workspaces {
                current,
                workspaces,
            } => json!({
                "text": self.text(),
                "current_workspace": current,
                "workspaces": workspaces.iter().map(|workspace| json!({
                    "name": workspace.name,
                    "is_current": workspace.is_current,
                    "window_count": workspace.window_count,
                    "has_windows": workspace.has_windows(),
                })).collect::<Vec<_>>(),
            }),
            Self::Unavailable => json!({ "text": PLACEHOLDER_TEXT, "available": false }),
        }
    }
}

impl Serialize for ItemContent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.data().serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ItemContent, Workspace};

    #[test]
    fn text_content_data_keeps_its_fields() {
        let content = ItemContent::plain_with_fields("12%", [("usage_percent", json!(12.0))]);
        assert_eq!(content.text(), "12%");
        assert_eq!(
            content.data(),
            json!({ "text": "12%", "usage_percent": 12.0 })
        );
    }

    #[test]
    fn workspace_content_joins_names_and_keeps_exact_counts() {
        let workspace = |name: &str, is_current, window_count| Workspace {
            name: name.into(),
            is_current,
            window_count,
        };
        let content = ItemContent::Workspaces {
            current: "2".into(),
            workspaces: vec![workspace("1", false, 3), workspace("2", true, 0)],
        };
        assert_eq!(content.text(), "1 2");
        assert_eq!(
            content.data(),
            json!({
                "text": "1 2",
                "current_workspace": "2",
                "workspaces": [
                    { "name": "1", "is_current": false, "window_count": 3, "has_windows": true },
                    { "name": "2", "is_current": true, "window_count": 0, "has_windows": false },
                ],
            })
        );
    }

    #[test]
    fn unavailable_content_shows_the_placeholder() {
        assert_eq!(ItemContent::Unavailable.text(), "—");
        assert_eq!(
            ItemContent::Unavailable.data(),
            json!({ "text": "—", "available": false })
        );
    }
}
