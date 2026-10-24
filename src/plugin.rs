use std::ffi::{CStr, CString};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::content::{ItemContent, Workspace};
use crate::error::BarrsError;
use crate::rift::RiftSnapshot;

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
    battery_snapshot_from_power_sources().unwrap_or(ItemContent::Unavailable)
}

/// Current GPU utilization, or unavailable content.
pub(crate) fn gpu() -> ItemContent {
    gpu_snapshot_from_accelerator().unwrap_or(ItemContent::Unavailable)
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

fn battery_snapshot_from_power_sources() -> Option<ItemContent> {
    let battery = iokit::battery()?;
    let percentage = battery_percentage(battery.current_capacity, battery.max_capacity)?;
    Some(ItemContent::plain_with_fields(
        format!("{percentage}%"),
        [
            ("percentage", json!(percentage)),
            ("charging", json!(battery.charging)),
        ],
    ))
}

fn gpu_snapshot_from_accelerator() -> Option<ItemContent> {
    let utilization = u8::try_from(iokit::gpu_utilization()?).ok()?;
    Some(ItemContent::plain_with_fields(
        format!("{utilization}%"),
        [("utilization_percent", json!(utilization))],
    ))
}

/// Charge in whole percent, rounded the way `pmset` rounds it.
fn battery_percentage(current_capacity: i64, max_capacity: i64) -> Option<u8> {
    let scaled = current_capacity
        .checked_mul(100)?
        .checked_add(max_capacity.checked_div(2)?)?;
    u8::try_from(scaled.checked_div(max_capacity)?).ok()
}

/// The internal battery as the power sources API reports it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct BatteryReading {
    current_capacity: i64,
    max_capacity: i64,
    charging: bool,
}

/// Battery and GPU readings from `IOKit`. They do not start a process.
#[cfg(target_os = "macos")]
mod iokit {
    use objc2_core_foundation::{
        CFArray, CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType,
    };
    use objc2_io_kit::{
        IOIteratorNext, IOObjectRelease, IOPSCopyPowerSourcesInfo, IOPSCopyPowerSourcesList,
        IOPSGetPowerSourceDescription, IORegistryEntryCreateCFProperty,
        IOServiceGetMatchingServices, IOServiceMatching, io_iterator_t, io_service_t,
        kIOMainPortDefault,
    };

    use super::BatteryReading;

    type Properties = CFDictionary<CFType, CFType>;

    /// The `Device Utilization %` statistic of the first GPU that reports it.
    pub(super) fn gpu_utilization() -> Option<i64> {
        // SAFETY: The class name is a NUL-terminated C string.
        let matching = unsafe { IOServiceMatching(c"IOAccelerator".as_ptr()) }?;
        // SAFETY: A mutable dictionary is a dictionary.
        let matching = unsafe { CFRetained::cast_unchecked::<CFDictionary>(matching) };
        // SAFETY: `kIOMainPortDefault` is an initialized constant.
        let main_port = unsafe { kIOMainPortDefault };
        let mut iterator: io_iterator_t = 0;
        // SAFETY: The call consumes `matching`, and `iterator` is valid for writes for the whole
        // call.
        let result =
            unsafe { IOServiceGetMatchingServices(main_port, Some(matching), &raw mut iterator) };
        if result != libc::KERN_SUCCESS {
            return None;
        }
        let mut utilization = None;
        while utilization.is_none() {
            let service = IOIteratorNext(iterator);
            if service == 0 {
                break;
            }
            utilization = service_utilization(service);
            IOObjectRelease(service);
        }
        IOObjectRelease(iterator);
        utilization
    }

    fn service_utilization(service: io_service_t) -> Option<i64> {
        let key = CFString::from_static_str("PerformanceStatistics");
        // SAFETY: `service` is a live registry entry until the caller releases it, and `key` is
        // a CF string.
        let statistics = unsafe { IORegistryEntryCreateCFProperty(service, Some(&key), None, 0) }?;
        let statistics = properties(statistics)?;
        number(&statistics, "Device Utilization %")
    }

    /// The first power source with a capacity, which is the internal battery.
    pub(super) fn battery() -> Option<BatteryReading> {
        let info = IOPSCopyPowerSourcesInfo()?;
        // SAFETY: `info` is the power sources blob that `IOPSCopyPowerSourcesInfo` returned.
        let sources = unsafe { IOPSCopyPowerSourcesList(Some(&info)) }?;
        // SAFETY: Every element of a CF array is a CF object, so `CFType` describes them.
        let sources = unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(sources) };
        (0..sources.len()).find_map(|index| {
            let source = sources.get(index)?;
            // SAFETY: `source` is an element of the list that `info` gave, and `info` is alive.
            let description = unsafe { IOPSGetPowerSourceDescription(Some(&info), Some(&source)) }?;
            let description = properties(description.into())?;
            battery_reading(&description)
        })
    }

    fn battery_reading(description: &Properties) -> Option<BatteryReading> {
        let on_ac_power =
            string(description, "Power Source State").is_some_and(|state| state == "AC Power");
        let charging = boolean(description, "Is Charging").unwrap_or(false);
        Some(BatteryReading {
            current_capacity: number(description, "Current Capacity")?,
            max_capacity: number(description, "Max Capacity")?,
            charging: on_ac_power || charging,
        })
    }

    fn properties(value: CFRetained<CFType>) -> Option<CFRetained<Properties>> {
        let dictionary = value.downcast::<CFDictionary>().ok()?;
        // SAFETY: Every key and value of a CF dictionary is a CF object, so `CFType` describes
        // both.
        Some(unsafe { CFRetained::cast_unchecked::<Properties>(dictionary) })
    }

    fn value(properties: &Properties, key: &'static str) -> Option<CFRetained<CFType>> {
        let key: CFRetained<CFType> = CFString::from_static_str(key).into();
        properties.get(&key)
    }

    fn number(properties: &Properties, key: &'static str) -> Option<i64> {
        value(properties, key)?
            .downcast::<CFNumber>()
            .ok()?
            .as_i64()
    }

    fn boolean(properties: &Properties, key: &'static str) -> Option<bool> {
        Some(
            value(properties, key)?
                .downcast::<CFBoolean>()
                .ok()?
                .as_bool(),
        )
    }

    fn string(properties: &Properties, key: &'static str) -> Option<String> {
        Some(
            value(properties, key)?
                .downcast::<CFString>()
                .ok()?
                .to_string(),
        )
    }
}

#[cfg(not(target_os = "macos"))]
mod iokit {
    use super::BatteryReading;

    pub(super) const fn gpu_utilization() -> Option<i64> {
        None
    }

    pub(super) const fn battery() -> Option<BatteryReading> {
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::plugin::{battery_percentage, cpu_usage_between};

    #[test]
    fn battery_percentage_rounds_the_capacity_share() {
        assert_eq!(battery_percentage(81, 100), Some(81));
        assert_eq!(battery_percentage(4321, 5000), Some(86));
        assert_eq!(battery_percentage(1, 0), None);
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
}
