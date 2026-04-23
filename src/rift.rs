use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::BarrsError;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiftBackendKind {
    Cli,
    Mach,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiftSnapshot {
    pub current_workspace: String,
    pub workspaces: Vec<RiftWorkspace>,
    pub layout: String,
    pub window_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiftWorkspace {
    pub name: String,
    pub is_current: bool,
    pub has_windows: bool,
}

pub trait RiftBackend: Send + Sync {
    fn kind(&self) -> RiftBackendKind;
    fn snapshot(&self) -> Result<RiftSnapshot, BarrsError>;
}

#[derive(Debug, Default)]
pub struct CliRiftBackend;

impl RiftBackend for CliRiftBackend {
    fn kind(&self) -> RiftBackendKind {
        RiftBackendKind::Cli
    }

    fn snapshot(&self) -> Result<RiftSnapshot, BarrsError> {
        cli_snapshot().ok_or_else(|| {
            BarrsError::Unsupported("failed to query workspace state from rift-cli".into())
        })
    }
}

#[derive(Debug, Default)]
pub struct MachRiftBackend;

impl RiftBackend for MachRiftBackend {
    fn kind(&self) -> RiftBackendKind {
        RiftBackendKind::Mach
    }

    fn snapshot(&self) -> Result<RiftSnapshot, BarrsError> {
        cli_snapshot().ok_or_else(|| {
            BarrsError::Unsupported("failed to query workspace state from rift-cli".into())
        })
    }
}

pub fn select_backend() -> Box<dyn RiftBackend> {
    if Path::new("/tmp/rift.sock").exists() {
        Box::new(MachRiftBackend)
    } else {
        Box::new(CliRiftBackend)
    }
}

fn cli_snapshot() -> Option<RiftSnapshot> {
    let workspaces_value = run_rift_cli(["query", "workspaces"])?;
    let mut workspaces = parse_workspaces(&workspaces_value);
    for workspace in &mut workspaces {
        if workspace.window_count.is_none() {
            workspace.window_count = query_window_count(workspace.workspace_id, workspace.space_id);
        }
    }
    workspaces.sort_by_key(|workspace| workspace.index.unwrap_or(usize::MAX));
    let active = select_active_workspace(&workspaces)?;
    let rendered_workspaces = workspaces
        .iter()
        .map(|workspace| RiftWorkspace {
            name: workspace.display_name(),
            is_current: workspace.active,
            has_windows: workspace.window_count.unwrap_or(0) > 0,
        })
        .collect::<Vec<_>>();

    let layout = query_layout(active.space_id, active.workspace_id)
        .or_else(|| active.layout.clone())
        .unwrap_or_else(|| "tiling".into());

    let window_count = active.window_count.unwrap_or_else(|| {
        query_window_count(active.workspace_id, active.space_id).unwrap_or(0)
    });

    Some(RiftSnapshot {
        current_workspace: active.display_name(),
        workspaces: if rendered_workspaces.is_empty() {
            vec![RiftWorkspace {
                name: active.display_name(),
                is_current: true,
                has_windows: window_count > 0,
            }]
        } else {
            rendered_workspaces
        },
        layout,
        window_count,
    })
}

fn run_rift_cli<const N: usize>(args: [&str; N]) -> Option<Value> {
    let output = Command::new("rift-cli").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let value: Value = serde_json::from_str(&stdout).ok()?;
    unwrap_response(value)
}

fn unwrap_response(value: Value) -> Option<Value> {
    if value.is_array() {
        return Some(value);
    }

    let object = value.as_object()?;
    for key in ["data", "payload", "result"] {
        if let Some(inner) = object.get(key) {
            return Some(inner.clone());
        }
    }
    if let Some(success) = object.get("success").and_then(Value::as_object) {
        for key in ["data", "payload", "result"] {
            if let Some(inner) = success.get(key) {
                return Some(inner.clone());
            }
        }
    }
    Some(Value::Object(object.clone()))
}

#[derive(Debug, Clone, Default)]
struct ParsedWorkspace {
    workspace_id: Option<u64>,
    space_id: Option<u64>,
    name: Option<String>,
    index: Option<usize>,
    active: bool,
    visible: bool,
    layout: Option<String>,
    window_count: Option<usize>,
}

impl ParsedWorkspace {
    fn display_name(&self) -> String {
        self.index
            .map(|index| (index + 1).to_string())
            .or_else(|| {
                self.name
                    .clone()
                    .filter(|name| !name.trim().is_empty())
            })
            .or_else(|| self.workspace_id.map(|id| id.to_string()))
            .unwrap_or_else(|| "?".into())
    }
}

fn parse_workspaces(value: &Value) -> Vec<ParsedWorkspace> {
    match value {
        Value::Array(items) => items.iter().map(parse_workspace).collect(),
        Value::Object(map) => {
            if let Some(items) = map.get("workspaces").and_then(Value::as_array) {
                items.iter().map(parse_workspace).collect()
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

fn parse_workspace(value: &Value) -> ParsedWorkspace {
    ParsedWorkspace {
        workspace_id: find_u64(value, &["workspace_id", "id"]),
        space_id: find_u64(value, &["space_id"]),
        name: find_string(value, &["workspace_name", "name", "label", "title"]),
        index: find_u64(value, &["workspace_index", "index"]).map(|value| value as usize),
        active: find_bool(value, &["active", "is_active", "focused", "current"]).unwrap_or(false),
        visible: find_bool(value, &["visible", "is_visible"]).unwrap_or(false),
        layout: find_string(value, &["layout", "layout_mode", "mode"]),
        window_count: find_u64(value, &["window_count"]).map(|value| value as usize),
    }
}

fn select_active_workspace(workspaces: &[ParsedWorkspace]) -> Option<&ParsedWorkspace> {
    workspaces
        .iter()
        .find(|workspace| workspace.active)
        .or_else(|| workspaces.iter().find(|workspace| workspace.visible))
        .or_else(|| workspaces.first())
}

fn query_layout(space_id: Option<u64>, workspace_id: Option<u64>) -> Option<String> {
    let value = if let Some(space_id) = space_id {
        run_rift_cli(["query", "workspace-layout", "--space-id", &space_id.to_string()])?
    } else if let Some(workspace_id) = workspace_id {
        run_rift_cli([
            "query",
            "workspace-layout",
            "--workspace-id",
            &workspace_id.to_string(),
        ])?
    } else {
        return None;
    };
    parse_layout_value(&value)
}

fn parse_layout_value(value: &Value) -> Option<String> {
    if let Some(layout) = find_string(value, &["layout_mode", "layout", "mode"]) {
        return Some(layout);
    }
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => items.iter().find_map(parse_layout_value),
        Value::Object(map) => map.values().find_map(parse_layout_value),
        _ => None,
    }
}

fn query_window_count(workspace_id: Option<u64>, space_id: Option<u64>) -> Option<usize> {
    let value = if let Some(space_id) = space_id {
        run_rift_cli(["query", "windows", "--space-id", &space_id.to_string()])?
    } else {
        run_rift_cli(["query", "windows"])?
    };

    let windows = match value {
        Value::Array(items) => items,
        Value::Object(map) => map
            .get("windows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    Some(
        windows
            .iter()
            .filter(|window| {
                if let Some(target_workspace) = workspace_id {
                    find_u64(window, &["workspace_id"]) == Some(target_workspace)
                } else if let Some(target_space) = space_id {
                    find_u64(window, &["space_id"]) == Some(target_space)
                } else {
                    true
                }
            })
            .count(),
    )
}

fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(text) = object.get(*key).and_then(Value::as_str) {
            return Some(text.to_string());
        }
    }
    None
}

fn find_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(number) = object.get(*key).and_then(Value::as_u64) {
            return Some(number);
        }
    }
    None
}

fn find_bool(value: &Value, keys: &[&str]) -> Option<bool> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(flag) = object.get(*key).and_then(Value::as_bool) {
            return Some(flag);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use serde_json::Value;

    use super::{
        CliRiftBackend, RiftBackend, RiftBackendKind, parse_layout_value, parse_workspaces,
        query_window_count, select_active_workspace, unwrap_response,
    };

    #[test]
    fn cli_backend_snapshot_is_stable() {
        let backend = CliRiftBackend;
        assert_eq!(backend.kind(), RiftBackendKind::Cli);
    }

    #[test]
    fn backend_selection_returns_known_backend() {
        let backend = super::select_backend();
        assert!(matches!(
            backend.kind(),
            RiftBackendKind::Cli | RiftBackendKind::Mach
        ));
    }

    #[test]
    fn unwraps_success_response() {
        let value = unwrap_response(json!({
            "success": {
                "data": [
                    { "workspace_id": 1, "workspace_name": "1", "active": true }
                ]
            }
        }))
        .expect("unwrap response");
        assert!(value.is_array());
    }

    #[test]
    fn parses_active_workspace_list() {
        let workspaces = parse_workspaces(&json!([
            { "workspace_id": 1, "workspace_name": "1", "workspace_index": 0, "visible": true },
            { "workspace_id": 2, "workspace_name": "dev", "workspace_index": 1, "active": true, "space_id": 7 }
        ]));
        let active = select_active_workspace(&workspaces).expect("active workspace");
        assert_eq!(active.display_name(), "2");
        assert_eq!(active.space_id, Some(7));
    }

    #[test]
    fn parses_workspace_window_count() {
        let workspaces = parse_workspaces(&json!([
            { "workspace_index": 0, "name": "1", "window_count": 2 },
            { "workspace_index": 1, "name": "2", "window_count": 0 }
        ]));
        assert_eq!(workspaces[0].window_count, Some(2));
        assert_eq!(workspaces[1].window_count, Some(0));
    }

    #[test]
    fn parses_layout_value_from_nested_shape() {
        let layout = parse_layout_value(&json!({
            "workspace_id": 2,
            "layout_mode": "bsp"
        }))
        .expect("layout");
        assert_eq!(layout, "bsp");
    }

    #[test]
    fn counts_windows_for_workspace() {
        let value = json!([
            { "workspace_id": 2, "space_id": 7 },
            { "workspace_id": 2, "space_id": 7 },
            { "workspace_id": 3, "space_id": 8 }
        ]);
        let count = match value {
            Value::Array(items) => items
                .iter()
                .filter(|window| super::find_u64(window, &["workspace_id"]) == Some(2))
                .count(),
            _ => 0,
        };
        assert_eq!(count, 2);
        let _ = query_window_count(None, None);
    }
}
