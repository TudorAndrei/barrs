use std::ffi::{CStr, CString};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::content::{ItemContent, Workspace};
use crate::error::BarrsError;
use crate::process::run_command_with_timeout;
use crate::rift::RiftSnapshot;

const SYSTEM_QUERY_TIMEOUT: Duration = Duration::from_secs(2);
const TIME_FORMAT: &str = "%H:%M";

/// Current CPU usage, or unavailable content.
pub(crate) fn cpu() -> ItemContent {
    cpu_snapshot_from_load_ticks().unwrap_or(ItemContent::Unavailable)
}

/// Current local time.
///
/// # Errors
///
/// Returns an error when the local time cannot be formatted.
pub(crate) fn time() -> Result<ItemContent, BarrsError> {
    Ok(ItemContent::plain_with_fields(
        format_local_time(TIME_FORMAT)?,
        [("timezone", json!("local"))],
    ))
}

/// Current local date in the `strftime` `format`.
///
/// # Errors
///
/// Returns an error when the local date cannot be formatted.
pub(crate) fn date(format: &str) -> Result<ItemContent, BarrsError> {
    Ok(ItemContent::plain_with_fields(
        format_local_time(format)?,
        [("timezone", json!("local")), ("format", json!(format))],
    ))
}

/// Current battery charge, or unavailable content.
pub(crate) fn battery() -> ItemContent {
    battery_snapshot_from_pmset().unwrap_or(ItemContent::Unavailable)
}

/// Current GPU utilization, or unavailable content.
pub(crate) fn gpu() -> ItemContent {
    gpu_snapshot_from_ioreg().unwrap_or(ItemContent::Unavailable)
}

/// Rift workspaces, or unavailable content without a snapshot.
pub(crate) fn rift_workspaces(rift: Option<&RiftSnapshot>) -> ItemContent {
    rift.map_or(ItemContent::Unavailable, |snapshot| {
        ItemContent::Workspaces {
            current: snapshot.current_workspace.clone(),
            workspaces: snapshot
                .workspaces
                .iter()
                .map(|workspace| Workspace {
                    name: workspace.name.clone(),
                    is_current: workspace.is_current,
                    window_count: workspace.window_count,
                })
                .collect(),
        }
    })
}

/// Rift layout, or unavailable content without a snapshot.
pub(crate) fn rift_layout(rift: Option<&RiftSnapshot>) -> ItemContent {
    rift.map_or(ItemContent::Unavailable, |snapshot| {
        ItemContent::plain_with_fields(
            snapshot.layout.clone(),
            [("window_count", json!(snapshot.window_count))],
        )
    })
}

fn cpu_snapshot_from_load_ticks() -> Option<ItemContent> {
    let usage = cpu_load::usage_percent()?;
    let rounded = usage.round();
    Some(ItemContent::plain_with_fields(
        format!("{rounded:.0}%"),
        [("usage_percent", json!(usage))],
    ))
}

/// Busy share, in percent, of the CPU ticks between two readings of the
/// host CPU load: user, system, idle, and nice ticks, in that order.
fn cpu_usage_between(previous: [u32; 4], current: [u32; 4]) -> Option<f64> {
    let [user_before, system_before, idle_before, nice_before] = previous;
    let [user_now, system_now, idle_now, nice_now] = current;
    let user = user_now.wrapping_sub(user_before);
    let system = system_now.wrapping_sub(system_before);
    let idle = idle_now.wrapping_sub(idle_before);
    let nice = nice_now.wrapping_sub(nice_before);
    let busy = user.checked_add(system)?.checked_add(nice)?;
    let total = busy.checked_add(idle)?;
    (total > 0).then(|| f64::from(busy) * 100.0 / f64::from(total))
}

/// CPU usage from the host CPU load ticks. Reading the ticks is a single
/// Mach call; it does not start a process.
#[cfg(target_os = "macos")]
mod cpu_load {
    use std::mem::MaybeUninit;
    use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
    use std::time::Duration;

    /// The wait between the first two readings, when no earlier reading exists.
    const FIRST_SAMPLE_WINDOW: Duration = Duration::from_millis(250);

    static HOST: OnceLock<libc::mach_port_t> = OnceLock::new();
    static PREVIOUS: Mutex<Option<[u32; 4]>> = Mutex::new(None);

    /// CPU usage since the previous call, or over a short window on the first
    /// call.
    pub(super) fn usage_percent() -> Option<f64> {
        let stored = *previous_ticks();
        let before = if let Some(ticks) = stored {
            ticks
        } else {
            let ticks = read_ticks()?;
            std::thread::sleep(FIRST_SAMPLE_WINDOW);
            ticks
        };
        let current = read_ticks()?;
        *previous_ticks() = Some(current);
        super::cpu_usage_between(before, current)
    }

    fn previous_ticks() -> MutexGuard<'static, Option<[u32; 4]>> {
        PREVIOUS.lock().unwrap_or_else(PoisonError::into_inner)
    }

    // libc points to the `mach2` crate for `mach_host_self`; this one call does not need it.
    #[allow(deprecated)]
    fn host_port() -> libc::mach_port_t {
        // SAFETY: `mach_host_self` has no preconditions. The caller keeps the port for the
        // process lifetime, so its send right is never released.
        unsafe { libc::mach_host_self() }
    }

    fn read_ticks() -> Option<[u32; 4]> {
        let host = *HOST.get_or_init(host_port);
        let mut info = MaybeUninit::<libc::host_cpu_load_info>::uninit();
        let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
        // SAFETY: `info` is valid for writes of one `host_cpu_load_info`, which is
        // `count` integers, and both stay alive for the whole call.
        let result = unsafe {
            libc::host_statistics(
                host,
                libc::HOST_CPU_LOAD_INFO,
                info.as_mut_ptr().cast(),
                &raw mut count,
            )
        };
        if result != libc::KERN_SUCCESS {
            return None;
        }
        // SAFETY: `host_statistics` succeeded, so it initialized `info`.
        Some(unsafe { info.assume_init() }.cpu_ticks)
    }
}

#[cfg(not(target_os = "macos"))]
mod cpu_load {
    pub(super) const fn usage_percent() -> Option<f64> {
        None
    }
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

fn battery_snapshot_from_pmset() -> Option<ItemContent> {
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

fn gpu_snapshot_from_ioreg() -> Option<ItemContent> {
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

fn parse_gpu_output(output: &str) -> Option<ItemContent> {
    let marker = "\"Device Utilization %\"=";
    let (_, tail) = output.split_once(marker)?;
    let number: String = tail
        .chars()
        .skip_while(|ch| ch.is_whitespace())
        .take_while(char::is_ascii_digit)
        .collect();
    let utilization = number.parse::<u8>().ok()?;
    Some(ItemContent::plain_with_fields(
        format!("{utilization}%"),
        [("utilization_percent", json!(utilization))],
    ))
}

fn parse_battery_output(output: &str) -> Option<ItemContent> {
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
    Some(ItemContent::plain_with_fields(
        format!("{percentage}%"),
        [
            ("percentage", json!(percentage)),
            ("charging", json!(charging)),
        ],
    ))
}

#[cfg(test)]
mod tests {
    use crate::plugin::{cpu_usage_between, parse_battery_output, parse_gpu_output};

    #[test]
    fn parses_battery_output() {
        let snapshot = parse_battery_output(
            "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1234567)\t81%; discharging; 3:21 remaining present: true",
        )
        .expect("parsed battery output")
        .data();
        assert_eq!(snapshot["percentage"], 81);
        assert_eq!(snapshot["charging"], false);
    }

    #[test]
    fn cpu_usage_is_the_busy_share_of_the_ticks_between_readings() {
        let usage = cpu_usage_between([100, 50, 1000, 0], [130, 60, 1050, 10]);

        assert_eq!(usage, Some(50.0));
    }

    #[test]
    fn cpu_usage_counts_ticks_across_counter_wraparound() {
        let usage = cpu_usage_between([u32::MAX, 0, 0, 0], [9, 0, 10, 0]);

        assert_eq!(usage, Some(50.0));
    }

    #[test]
    fn cpu_usage_is_unknown_without_new_ticks() {
        assert_eq!(cpu_usage_between([1, 2, 3, 4], [1, 2, 3, 4]), None);
    }

    #[test]
    fn parses_gpu_output() {
        let snapshot = parse_gpu_output(
            "\"PerformanceStatistics\" = {\"Tiler Utilization %\"=14,\"Renderer Utilization %\"=40,\"Device Utilization %\"=40}",
        )
        .expect("parsed gpu output")
        .data();
        assert_eq!(snapshot["text"], "40%");
        assert_eq!(snapshot["utilization_percent"], 40);
    }
}
