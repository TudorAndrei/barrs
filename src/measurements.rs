//! Shared fixtures and timing for manual data-layout measurements.
use std::hint::black_box;
use std::time::Instant;

use crate::config::{Config, ItemConfig, ItemHandlers};
use crate::source::ItemSource;

pub fn config(count: usize) -> Config {
    Config {
        items: (0..count)
            .map(|index| ItemConfig {
                id: format!("item-{index}"),
                label: Some("Status".into()),
                icon: Some("*".into()),
                placement: Some("left".into()),
                interval: None,
                source: ItemSource::Label {
                    text: "ready".into(),
                    icon: None,
                },
                hover: None,
                handlers: ItemHandlers::default(),
            })
            .collect(),
        ..Config::default()
    }
}

pub fn compare(
    area: &str,
    count: usize,
    workload: &str,
    iterations: u32,
    mut current: impl FnMut(),
    mut candidate: impl FnMut(),
) {
    fn time(iterations: u32, operation: &mut impl FnMut()) -> f64 {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        start.elapsed().as_secs_f64() * 1e9 / f64::from(iterations)
    }
    for _ in 0..16 {
        current();
        candidate();
    }
    let mut old = Vec::new();
    let mut new = Vec::new();
    for round in 0..9 {
        let (a, b) = if round % 2 == 0 {
            (
                time(iterations, &mut current),
                time(iterations, &mut candidate),
            )
        } else {
            let b = time(iterations, &mut candidate);
            (time(iterations, &mut current), b)
        };
        old.push(black_box(a));
        new.push(black_box(b));
    }
    old.sort_by(f64::total_cmp);
    new.sort_by(f64::total_cmp);
    println!(
        "MEASURE {}",
        serde_json::json!({
            "area": area, "items": count, "workload": workload,
            "iterations": iterations, "rounds": 9,
            "current_ns": old[4], "candidate_ns": new[4],
            "current_range": [old[0], old[8]], "candidate_range": [new[0], new[8]],
            "current_samples": old, "candidate_samples": new,
        })
    );
}
