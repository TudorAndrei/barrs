//! Manual comparison with the scheduler scan before the array conversion.
//! Run with `cargo test --release --lib scheduler_scan_cost -- --ignored --nocapture`.

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Duration;

use super::{Config, Instant, ItemConfig, build_refresh_deadlines, due_item_ids};
use crate::config::ItemHandlers;
use crate::rift::RiftBackendKind;
use crate::source::{ItemSource, SystemPlugin};

// Retain the old scan and classification as an independent behavior oracle.
#[expect(
    clippy::needless_collect,
    reason = "preserve the old scheduler's intermediate allocation for comparison"
)]
fn map_due_items(
    deadlines: &HashMap<String, Instant>,
    config: &Config,
    now: Instant,
) -> (Vec<String>, Vec<String>) {
    let due: Vec<String> = deadlines
        .iter()
        .filter(|(_, deadline)| **deadline <= now)
        .map(|(id, _)| id.clone())
        .collect();
    due.into_iter().partition(|id| {
        config
            .item(id)
            .is_some_and(|item| item.source.lua_update().is_some())
    })
}

fn measure(mut scan: impl FnMut() -> (Vec<String>, Vec<String>), iterations: u32) -> f64 {
    let start = std::time::Instant::now();
    for _ in 0..iterations {
        black_box(scan());
    }
    start.elapsed().as_secs_f64() * 1e9 / f64::from(iterations)
}

#[test]
#[ignore = "manual release-profile timing; reports nanoseconds per scan"]
fn scheduler_scan_cost() {
    for count in [0, 6, 64, 1024] {
        for (workload, due_every, scheduled_every) in [
            ("idle", usize::MAX, 1),
            ("sparse", 8, 1),
            ("all", 1, 1),
            ("mostly_labels", 8, 8),
            ("labels_only", 1, usize::MAX),
        ] {
            let now = Instant::now();
            let config = Config {
                items: (0..count)
                    .map(|index| ItemConfig {
                        id: format!("item-{index}"),
                        label: None,
                        icon: None,
                        placement: None,
                        interval: None,
                        source: if scheduled_every == usize::MAX || index % scheduled_every != 0 {
                            ItemSource::Label {
                                text: "fixed".into(),
                                icon: None,
                            }
                        } else if index % 3 == 0 {
                            ItemSource::Lua {
                                update: "update".into(),
                            }
                        } else {
                            ItemSource::System(SystemPlugin::Time)
                        },
                        hover: None,
                        handlers: ItemHandlers::default(),
                    })
                    .collect(),
                ..Config::default()
            };
            let mut array = build_refresh_deadlines(&config, now, RiftBackendKind::Mach);
            for deadline in &mut array {
                deadline.at = if due_every != usize::MAX && deadline.order % due_every == 0 {
                    now
                } else {
                    now + Duration::from_secs(1)
                };
            }
            let map: HashMap<String, Instant> = array
                .iter()
                .map(|deadline| (config.items[deadline.order].id.clone(), deadline.at))
                .collect();
            let mut expected = map_due_items(&map, &config, now);
            expected.0.sort();
            expected.1.sort();
            let mut actual = due_item_ids(&array, &config, now);
            actual.0.sort();
            actual.1.sort();
            assert_eq!(actual, expected, "{count} {workload}");

            let iterations = if count > 64 { 1_000 } else { 20_000 };
            let mut old_samples = Vec::new();
            let mut new_samples = Vec::new();
            for round in 0..7 {
                let old = || map_due_items(black_box(&map), black_box(&config), black_box(now));
                let new = || due_item_ids(black_box(&array), black_box(&config), black_box(now));
                let (old_ns, new_ns) = if round % 2 == 0 {
                    (measure(old, iterations), measure(new, iterations))
                } else {
                    let new_ns = measure(new, iterations);
                    (measure(old, iterations), new_ns)
                };
                old_samples.push(old_ns);
                new_samples.push(new_ns);
            }
            old_samples.sort_by(f64::total_cmp);
            new_samples.sort_by(f64::total_cmp);
            println!(
                "{count:4} {workload:14} map={:.1} [{:.1},{:.1}] array={:.1} [{:.1},{:.1}] ns/scan",
                old_samples[3],
                old_samples[0],
                old_samples[6],
                new_samples[3],
                new_samples[0],
                new_samples[6]
            );
        }
    }
}
