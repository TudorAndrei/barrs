use std::ffi::{CStr, CString};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::config::{ItemConfig, PluginKind};
use crate::error::BarrsError;
use crate::process::run_command_with_timeout;
use crate::rift::RiftSnapshot;

const SYSTEM_QUERY_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_DATE_FORMAT: &str = "%Y-%m-%d";
const TIME_FORMAT: &str = "%H:%M";
const PLACEHOLDER_TEXT: &str = "—";

/// Snapshot data for an item whose data source cannot report a value.
pub(crate) fn unavailable_snapshot() -> Value {
    json!({
        "text": PLACEHOLDER_TEXT,
        "available": false
    })
}

/// Returns the current built-in snapshot, or `None` for Lua or no plugin.
///
/// # Errors
///
/// Returns an error when the local time or date cannot be formatted.
pub fn from_item_config(
    item: &ItemConfig,
    rift: Option<&RiftSnapshot>,
) -> Result<Option<Value>, BarrsError> {
    let Some(binding) = item.plugin.as_ref() else {
        return Ok(None);
    };
    let snapshot = match binding.kind {
        PluginKind::Cpu => cpu_snapshot_from_top().unwrap_or_else(unavailable_snapshot),
        PluginKind::Time => json!({
            "text": format_local_time(TIME_FORMAT)?,
            "timezone": "local"
        }),
        PluginKind::Date => {
            let format = binding.format.as_deref().unwrap_or(DEFAULT_DATE_FORMAT);
            json!({
                "text": format_local_time(format)?,
                "timezone": "local",
                "format": format,
            })
        }
        PluginKind::Battery => battery_snapshot_from_pmset().unwrap_or_else(unavailable_snapshot),
        PluginKind::Gpu => gpu_snapshot_from_ioreg().unwrap_or_else(unavailable_snapshot),
        PluginKind::RiftWorkspaces => rift.map_or_else(unavailable_snapshot, |snapshot| {
            json!({
                "text": snapshot.workspaces.iter().map(|workspace| workspace.name.as_str()).collect::<Vec<_>>().join(" "),
                "current_workspace": snapshot.current_workspace,
                "workspaces": snapshot.workspaces.iter().map(|workspace| json!({
                    "name": workspace.name,
                    "is_current": workspace.is_current,
                    "window_count": workspace.window_count,
                    "has_windows": workspace.has_windows
                })).collect::<Vec<_>>(),
            })
        }),
        PluginKind::RiftLayout => rift.map_or_else(unavailable_snapshot, |snapshot| {
            json!({
                "text": snapshot.layout,
                "window_count": snapshot.window_count
            })
        }),
        // The daemon runs Lua update functions on the Lua executor, not as a Rust plugin.
        PluginKind::Lua => return Ok(None),
    };
    Ok(Some(snapshot))
}

fn cpu_snapshot_from_top() -> Option<Value> {
    #[cfg(target_os = "macos")]
    {
        let output =
            run_command_with_timeout("top", &["-l", "2", "-n", "0"], SYSTEM_QUERY_TIMEOUT)?;
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
    let rounded = usage.round();
    Some(json!({
        "text": format!("{rounded:.0}%"),
        "usage_percent": usage
    }))
}

fn format_local_time(format: &str) -> Result<String, BarrsError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| BarrsError::Unsupported(err.to_string()))?
        .as_secs();
    let timestamp = libc::time_t::try_from(seconds)
        .map_err(|_| BarrsError::Unsupported("timestamp out of range".into()))?;
    let mut local_time = std::mem::MaybeUninit::<libc::tm>::uninit();
    let format =
        CString::new(format).map_err(|_| BarrsError::Unsupported("format contains NUL".into()))?;
    let mut buffer: [libc::c_char; 256] = [0; 256];

    // SAFETY: `timestamp` is a valid `time_t`, and `local_time` is valid for writes of one `tm`.
    // Both stay alive for the whole call.
    if unsafe { libc::localtime_r(&raw const timestamp, local_time.as_mut_ptr()) }.is_null() {
        return Err(BarrsError::Unsupported("failed to read local time".into()));
    }
    // SAFETY: `localtime_r` succeeded, so `local_time` is initialized. `buffer` is valid for
    // writes of `buffer.len()` bytes, and `format` is a NUL-terminated C string.
    let written = unsafe {
        libc::strftime(
            buffer.as_mut_ptr(),
            buffer.len(),
            format.as_ptr(),
            local_time.as_ptr(),
        )
    };
    if written == 0 {
        return Err(BarrsError::Unsupported(
            "failed to format local time".into(),
        ));
    }
    // SAFETY: A non-zero `strftime` result means it wrote a NUL-terminated string into `buffer`.
    let formatted = unsafe { CStr::from_ptr(buffer.as_ptr()) };
    Ok(formatted.to_string_lossy().into_owned())
}

fn battery_snapshot_from_pmset() -> Option<Value> {
    #[cfg(target_os = "macos")]
    {
        let output = run_command_with_timeout("pmset", &["-g", "batt"], SYSTEM_QUERY_TIMEOUT)?;
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
        let output = run_command_with_timeout(
            "ioreg",
            &["-r", "-d", "1", "-c", "IOAccelerator"],
            SYSTEM_QUERY_TIMEOUT,
        )?;
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
    let (_, tail) = output.split_once(marker)?;
    let number: String = tail
        .chars()
        .skip_while(|ch| ch.is_whitespace())
        .take_while(char::is_ascii_digit)
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

#[cfg(test)]
mod tests {
    use crate::config::{ItemConfig, ItemHandlers, PluginBinding, PluginKind};
    use crate::error::BarrsError;
    use crate::plugin::{
        from_item_config, parse_battery_output, parse_cpu_output, parse_gpu_output,
        unavailable_snapshot,
    };
    use crate::rift::{RiftSnapshot, RiftWorkspace};

    fn item_with_plugin(kind: PluginKind, format: Option<String>) -> ItemConfig {
        ItemConfig {
            id: "item".into(),
            label: None,
            icon: None,
            placement: None,
            interval: Some(1),
            plugin: Some(PluginBinding {
                kind,
                format,
                update: None,
            }),
            hover: None,
            handlers: ItemHandlers::default(),
        }
    }

    #[test]
    fn creates_builtin_snapshot_from_config() {
        let item = item_with_plugin(PluginKind::Cpu, None);
        let snapshot = from_item_config(&item, None)
            .expect("snapshot")
            .expect("built-in data");
        let text = snapshot["text"].as_str().expect("text");
        assert!(text.ends_with('%') || text == super::PLACEHOLDER_TEXT);
    }

    #[test]
    fn cpu_plugin_reports_unavailable_without_data() {
        let snapshot = unavailable_snapshot();
        assert_eq!(snapshot["text"], super::PLACEHOLDER_TEXT);
        assert_eq!(snapshot["available"], false);
    }

    #[test]
    fn rift_workspaces_plugin_reports_unavailable_without_snapshot() {
        let item = item_with_plugin(PluginKind::RiftWorkspaces, None);
        let snapshot = from_item_config(&item, None)
            .expect("snapshot")
            .expect("built-in data");
        assert_eq!(snapshot["text"], super::PLACEHOLDER_TEXT);
        assert_eq!(snapshot["available"], false);
        assert!(snapshot.get("workspaces").is_none());
    }

    #[test]
    fn rift_layout_plugin_reports_unavailable_without_snapshot() {
        let item = item_with_plugin(PluginKind::RiftLayout, None);
        let snapshot = from_item_config(&item, None)
            .expect("snapshot")
            .expect("built-in data");
        assert_eq!(snapshot["text"], super::PLACEHOLDER_TEXT);
        assert_eq!(snapshot["available"], false);
        assert!(snapshot.get("window_count").is_none());
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
                format: None,
                update: None,
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
                    window_count: 3,
                    has_windows: true,
                },
                RiftWorkspace {
                    name: "2".into(),
                    is_current: true,
                    window_count: 4,
                    has_windows: true,
                },
                RiftWorkspace {
                    name: "3".into(),
                    is_current: false,
                    window_count: 0,
                    has_windows: false,
                },
            ],
            layout: "bsp".into(),
            window_count: 4,
        };

        let snapshot = from_item_config(&item, Some(&rift))
            .expect("snapshot")
            .expect("built-in data");
        assert_eq!(snapshot["current_workspace"], "2");
        assert_eq!(snapshot["text"], "1 2 3");
        assert_eq!(snapshot["workspaces"][0]["has_windows"], true);
        assert_eq!(snapshot["workspaces"][0]["window_count"], 3);
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
                format: None,
                update: None,
            }),
            hover: None,
            handlers: ItemHandlers::default(),
        };

        let snapshot = from_item_config(&item, None)
            .expect("snapshot")
            .expect("built-in data");
        let text = snapshot["text"].as_str().expect("time text");
        assert_eq!(text.len(), 5);
        assert_eq!(text.chars().nth(2), Some(':'));
    }

    #[test]
    fn date_plugin_uses_default_format_from_config() {
        let item = ItemConfig {
            id: "date".into(),
            label: None,
            icon: None,
            placement: None,
            interval: None,
            plugin: Some(PluginBinding {
                kind: PluginKind::Date,
                format: None,
                update: None,
            }),
            hover: None,
            handlers: ItemHandlers::default(),
        };

        let snapshot = from_item_config(&item, None)
            .expect("snapshot")
            .expect("built-in data");
        assert_eq!(snapshot["format"], "%Y-%m-%d");
        assert!(!snapshot["text"].as_str().expect("date text").is_empty());
    }

    #[test]
    fn date_plugin_formats_current_date() {
        let item = item_with_plugin(PluginKind::Date, Some("%Y-%m-%d".into()));
        let snapshot = from_item_config(&item, None)
            .expect("snapshot")
            .expect("built-in data");
        let text = snapshot["text"].as_str().expect("date text");
        assert!(!text.is_empty());
        assert_eq!(snapshot["timezone"], "local");
        assert_eq!(snapshot["format"], "%Y-%m-%d");
    }

    #[test]
    fn date_plugin_uses_known_format() {
        let item = item_with_plugin(PluginKind::Date, Some("%Y".into()));
        let snapshot = from_item_config(&item, None)
            .expect("snapshot")
            .expect("built-in data");
        let text = snapshot["text"].as_str().expect("date text");
        assert_eq!(text.len(), 4);
        assert!(text.chars().all(|ch| ch.is_ascii_digit()));
        assert_eq!(snapshot["format"], "%Y");
    }

    #[test]
    fn date_plugin_returns_formatting_error() {
        let item = item_with_plugin(PluginKind::Date, Some("x".repeat(256)));

        let error = from_item_config(&item, None).expect_err("format exceeds output buffer");
        assert!(matches!(
            error,
            BarrsError::Unsupported(message) if message == "failed to format local time"
        ));
    }
}
