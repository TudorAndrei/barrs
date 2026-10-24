use std::ffi::{CStr, CString};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::error::BarrsError;
use crate::process::run_command_with_timeout;
use crate::rift::RiftSnapshot;

const SYSTEM_QUERY_TIMEOUT: Duration = Duration::from_secs(2);
const TIME_FORMAT: &str = "%H:%M";
const PLACEHOLDER_TEXT: &str = "—";

/// Snapshot data for an item whose data source cannot report a value.
pub(crate) fn unavailable_snapshot() -> Value {
    json!({
        "text": PLACEHOLDER_TEXT,
        "available": false
    })
}

/// Current CPU usage, or the unavailable placeholder.
pub(crate) fn cpu() -> Value {
    cpu_snapshot_from_top().unwrap_or_else(unavailable_snapshot)
}

/// Current local time.
///
/// # Errors
///
/// Returns an error when the local time cannot be formatted.
pub(crate) fn time() -> Result<Value, BarrsError> {
    Ok(json!({
        "text": format_local_time(TIME_FORMAT)?,
        "timezone": "local"
    }))
}

/// Current local date in the `strftime` `format`.
///
/// # Errors
///
/// Returns an error when the local date cannot be formatted.
pub(crate) fn date(format: &str) -> Result<Value, BarrsError> {
    Ok(json!({
        "text": format_local_time(format)?,
        "timezone": "local",
        "format": format,
    }))
}

/// Current battery charge, or the unavailable placeholder.
pub(crate) fn battery() -> Value {
    battery_snapshot_from_pmset().unwrap_or_else(unavailable_snapshot)
}

/// Current GPU utilization, or the unavailable placeholder.
pub(crate) fn gpu() -> Value {
    gpu_snapshot_from_ioreg().unwrap_or_else(unavailable_snapshot)
}

/// Rift workspaces, or the unavailable placeholder without a snapshot.
pub(crate) fn rift_workspaces(rift: Option<&RiftSnapshot>) -> Value {
    rift.map_or_else(unavailable_snapshot, |snapshot| {
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
    })
}

/// Rift layout, or the unavailable placeholder without a snapshot.
pub(crate) fn rift_layout(rift: Option<&RiftSnapshot>) -> Value {
    rift.map_or_else(unavailable_snapshot, |snapshot| {
        json!({
            "text": snapshot.layout,
            "window_count": snapshot.window_count
        })
    })
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
    use crate::plugin::{
        parse_battery_output, parse_cpu_output, parse_gpu_output, unavailable_snapshot,
    };

    #[test]
    fn unavailable_snapshot_uses_placeholder_text() {
        let snapshot = unavailable_snapshot();
        assert_eq!(snapshot["text"], super::PLACEHOLDER_TEXT);
        assert_eq!(snapshot["available"], false);
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
}
