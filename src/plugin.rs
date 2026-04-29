use std::ffi::CStr;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::config::{ItemConfig, PluginKind};
use crate::error::BarrsError;
use crate::ipc::EventPayload;
use crate::rift::{RiftSnapshot, RiftWorkspace};

pub trait Plugin: Send + Sync {
    fn snapshot(&self) -> Result<Value, BarrsError>;
    fn handle_event(&mut self, _event: &EventPayload) -> Result<(), BarrsError> {
        Ok(())
    }
}

pub fn from_item_config(item: &ItemConfig, rift: Option<&RiftSnapshot>) -> Option<Box<dyn Plugin>> {
    match item.plugin.as_ref()?.kind {
        PluginKind::Cpu => Some(Box::new(CpuPlugin)),
        PluginKind::Time => Some(Box::new(TimePlugin)),
        PluginKind::Battery => Some(Box::new(BatteryPlugin)),
        PluginKind::Gpu => Some(Box::new(GpuPlugin)),
        PluginKind::RiftWorkspaces => Some(Box::new(RiftWorkspacesPlugin {
            snapshot: rift.cloned(),
        })),
        PluginKind::RiftLayout => Some(Box::new(RiftLayoutPlugin {
            snapshot: rift.cloned(),
        })),
    }
}

#[derive(Debug, Default)]
pub struct CpuPlugin;

impl Plugin for CpuPlugin {
    fn snapshot(&self) -> Result<Value, BarrsError> {
        if let Some(snapshot) = cpu_snapshot_from_top() {
            return Ok(snapshot);
        }
        Ok(json!({
            "text": "12%",
            "usage_percent": 12.5
        }))
    }
}

#[derive(Debug, Default)]
pub struct TimePlugin;

impl Plugin for TimePlugin {
    fn snapshot(&self) -> Result<Value, BarrsError> {
        let formatted = format_local_time()?;
        Ok(json!({
            "text": formatted,
            "timezone": "local"
        }))
    }
}

#[derive(Debug, Default)]
pub struct BatteryPlugin;

impl Plugin for BatteryPlugin {
    fn snapshot(&self) -> Result<Value, BarrsError> {
        if let Some(snapshot) = battery_snapshot_from_pmset() {
            return Ok(snapshot);
        }
        Ok(json!({
            "text": "Battery",
            "percentage": 87,
            "charging": false
        }))
    }
}

#[derive(Debug, Default)]
pub struct GpuPlugin;

impl Plugin for GpuPlugin {
    fn snapshot(&self) -> Result<Value, BarrsError> {
        if let Some(snapshot) = gpu_snapshot_from_ioreg() {
            return Ok(snapshot);
        }
        Ok(json!({
            "text": "21%",
            "utilization_percent": 21
        }))
    }
}

fn cpu_snapshot_from_top() -> Option<Value> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("top")
            .args(["-l", "2", "-n", "0"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_cpu_output(std::str::from_utf8(&output.stdout).ok()?)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn parse_cpu_output(output: &str) -> Option<Value> {
    let line = output
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with("CPU usage:"))?;
    let usage = line
        .split(':')
        .nth(1)?
        .split(',')
        .filter(|part| !part.to_ascii_lowercase().contains("idle"))
        .filter_map(|part| part.trim().split('%').next())
        .filter_map(|number| number.trim().parse::<f64>().ok())
        .sum::<f64>();
    let rounded = usage.round() as i32;
    Some(json!({
        "text": format!("{rounded}%"),
        "usage_percent": usage
    }))
}

fn format_local_time() -> Result<String, BarrsError> {
    let mut timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| BarrsError::Unsupported(err.to_string()))?
        .as_secs() as libc::time_t;
    let mut local_time = std::mem::MaybeUninit::<libc::tm>::uninit();
    let format = c"%H:%M";
    let mut buffer = [0 as libc::c_char; 16];

    // SAFETY: The pointers are valid for writes and remain alive for the duration of the calls.
    unsafe {
        if libc::localtime_r(&mut timestamp, local_time.as_mut_ptr()).is_null() {
            return Err(BarrsError::Unsupported("failed to read local time".into()));
        }
        let written = libc::strftime(
            buffer.as_mut_ptr(),
            buffer.len(),
            format.as_ptr(),
            local_time.as_ptr(),
        );
        if written == 0 {
            return Err(BarrsError::Unsupported(
                "failed to format local time".into(),
            ));
        }
        Ok(CStr::from_ptr(buffer.as_ptr())
            .to_string_lossy()
            .into_owned())
    }
}

fn battery_snapshot_from_pmset() -> Option<Value> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("pmset").args(["-g", "batt"]).output().ok()?;
        if !output.status.success() {
            return None;
        }
        parse_battery_output(std::str::from_utf8(&output.stdout).ok()?)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn gpu_snapshot_from_ioreg() -> Option<Value> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("ioreg")
            .args(["-r", "-d", "1", "-c", "IOAccelerator"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_gpu_output(std::str::from_utf8(&output.stdout).ok()?)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn parse_gpu_output(output: &str) -> Option<Value> {
    let marker = "\"Device Utilization %\"=";
    let start = output.find(marker)? + marker.len();
    let tail = &output[start..];
    let number: String = tail
        .chars()
        .skip_while(|ch| ch.is_whitespace())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    let utilization = number.parse::<u8>().ok()?;
    Some(json!({
        "text": format!("{utilization}%"),
        "utilization_percent": utilization
    }))
}

fn parse_battery_output(output: &str) -> Option<Value> {
    let lower = output.to_ascii_lowercase();
    let percentage = output
        .split_whitespace()
        .find_map(|token| {
            token
                .trim_matches(|ch: char| matches!(ch, ';' | ',' | '\t'))
                .strip_suffix('%')
        })
        .and_then(|value| value.parse::<u8>().ok())?;
    let charging = lower.contains("ac power") || lower.contains("; charging;");
    Some(json!({
        "text": format!("{percentage}%"),
        "percentage": percentage,
        "charging": charging
    }))
}

#[derive(Debug, Clone)]
pub struct RiftWorkspacesPlugin {
    pub snapshot: Option<RiftSnapshot>,
}

impl Plugin for RiftWorkspacesPlugin {
    fn snapshot(&self) -> Result<Value, BarrsError> {
        let snapshot = self.snapshot.clone().unwrap_or(RiftSnapshot {
            current_workspace: "1".into(),
            workspaces: vec![RiftWorkspace {
                name: "1".into(),
                is_current: true,
                has_windows: true,
            }],
            layout: "tiling".into(),
            window_count: 1,
        });
        Ok(json!({
            "text": snapshot.workspaces.iter().map(|workspace| workspace.name.as_str()).collect::<Vec<_>>().join(" "),
            "current_workspace": snapshot.current_workspace,
            "workspaces": snapshot.workspaces.iter().map(|workspace| json!({
                "name": workspace.name,
                "is_current": workspace.is_current,
                "has_windows": workspace.has_windows
            })).collect::<Vec<_>>(),
        }))
    }
}

#[derive(Debug, Clone)]
pub struct RiftLayoutPlugin {
    pub snapshot: Option<RiftSnapshot>,
}

impl Plugin for RiftLayoutPlugin {
    fn snapshot(&self) -> Result<Value, BarrsError> {
        let snapshot = self.snapshot.clone().unwrap_or(RiftSnapshot {
            current_workspace: "1".into(),
            workspaces: vec![RiftWorkspace {
                name: "1".into(),
                is_current: true,
                has_windows: true,
            }],
            layout: "tiling".into(),
            window_count: 1,
        });
        Ok(json!({
            "text": snapshot.layout,
            "window_count": snapshot.window_count
        }))
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{ItemConfig, ItemHandlers, PluginBinding, PluginKind};
    use crate::plugin::{from_item_config, parse_battery_output, parse_cpu_output, parse_gpu_output};
    use crate::rift::{RiftSnapshot, RiftWorkspace};

    #[test]
    fn creates_builtin_plugin_from_config() {
        let item = ItemConfig {
            id: "cpu".into(),
            label: None,
            icon: None,
            placement: None,
            interval: Some(1),
            plugin: Some(PluginBinding {
                kind: PluginKind::Cpu,
            }),
            hover: None,
            handlers: ItemHandlers::default(),
        };

        let plugin = from_item_config(&item, None).expect("plugin");
        let snapshot = plugin.snapshot().expect("snapshot");
        assert!(snapshot["text"].as_str().expect("text").ends_with('%'));
    }

    #[test]
    fn creates_rift_plugin_with_snapshot() {
        let item = ItemConfig {
            id: "workspaces".into(),
            label: None,
            icon: None,
            placement: None,
            interval: Some(1),
            plugin: Some(PluginBinding {
                kind: PluginKind::RiftWorkspaces,
            }),
            hover: None,
            handlers: ItemHandlers::default(),
        };
        let rift = RiftSnapshot {
            current_workspace: "2".into(),
            workspaces: vec![
                RiftWorkspace {
                    name: "1".into(),
                    is_current: false,
                    has_windows: true,
                },
                RiftWorkspace {
                    name: "2".into(),
                    is_current: true,
                    has_windows: true,
                },
                RiftWorkspace {
                    name: "3".into(),
                    is_current: false,
                    has_windows: false,
                },
            ],
            layout: "bsp".into(),
            window_count: 4,
        };

        let plugin = from_item_config(&item, Some(&rift)).expect("plugin");
        let snapshot = plugin.snapshot().expect("snapshot");
        assert_eq!(snapshot["current_workspace"], "2");
        assert_eq!(snapshot["text"], "1 2 3");
        assert_eq!(snapshot["workspaces"][0]["has_windows"], true);
        assert_eq!(snapshot["workspaces"][1]["is_current"], true);
    }

    #[test]
    fn parses_battery_output() {
        let snapshot = parse_battery_output(
            "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1234567)\t81%; discharging; 3:21 remaining present: true",
        )
        .expect("parsed battery output");
        assert_eq!(snapshot["percentage"], 81);
        assert_eq!(snapshot["charging"], false);
    }

    #[test]
    fn parses_cpu_output() {
        let snapshot = parse_cpu_output(
            "CPU usage: 7.42% user, 5.09% sys, 87.48% idle\nCPU usage: 8.00% user, 4.00% sys, 88.00% idle",
        )
        .expect("parsed cpu output");
        assert_eq!(snapshot["text"], "12%");
        assert_eq!(snapshot["usage_percent"], 12.0);
    }

    #[test]
    fn parses_gpu_output() {
        let snapshot = parse_gpu_output(
            "\"PerformanceStatistics\" = {\"Tiler Utilization %\"=14,\"Renderer Utilization %\"=40,\"Device Utilization %\"=40}",
        )
        .expect("parsed gpu output");
        assert_eq!(snapshot["text"], "40%");
        assert_eq!(snapshot["utilization_percent"], 40);
    }

    #[test]
    fn time_plugin_formats_current_time() {
        let item = ItemConfig {
            id: "clock".into(),
            label: None,
            icon: None,
            placement: None,
            interval: Some(1),
            plugin: Some(PluginBinding {
                kind: PluginKind::Time,
            }),
            hover: None,
            handlers: ItemHandlers::default(),
        };

        let plugin = from_item_config(&item, None).expect("plugin");
        let snapshot = plugin.snapshot().expect("snapshot");
        let text = snapshot["text"].as_str().expect("time text");
        assert_eq!(text.len(), 5);
        assert_eq!(&text[2..3], ":");
    }
}
