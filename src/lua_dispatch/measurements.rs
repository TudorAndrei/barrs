//! Retained set operations versus production state transitions.
//! No Lua calls are timed.
use crate::measurements::{compare, config};
use std::collections::HashSet;
use std::hint::black_box;

use super::UpdateState;

fn set_cycle(ids: &[String], running: &mut HashSet<String>, again: &mut HashSet<String>) -> usize {
    for id in ids {
        if !running.contains(id) {
            running.insert(id.clone());
        }
    }
    // Two events while running must request only one repeat.
    for _ in 0..2 {
        for id in ids {
            if running.contains(id) {
                again.insert(id.clone());
            }
        }
    }
    let mut repeats = 0usize;
    for id in ids {
        running.remove(id);
        if again.remove(id) {
            repeats = repeats.wrapping_add(1);
            running.insert(id.clone());
        }
    }
    for id in ids {
        running.remove(id);
        black_box(again.remove(id));
    }
    repeats
}

fn array_cycle(states: &mut [UpdateState]) -> usize {
    for state in &mut *states {
        black_box(black_box(state).request());
    }
    for _ in 0..2 {
        for state in &mut *states {
            black_box(black_box(state).request_now());
        }
    }
    let mut repeats = 0usize;
    for state in &mut *states {
        if black_box(state).complete() {
            repeats = repeats.wrapping_add(1);
        }
    }
    for state in states {
        black_box(black_box(state).complete());
    }
    repeats
}

#[test]
#[ignore = "manual data-layout benchmark"]
fn measure_lua_state() {
    for count in [1, 6, 64, 1024] {
        let ids: Vec<_> = config(count)
            .items
            .into_iter()
            .map(|item| item.id)
            .collect();
        let mut running = HashSet::new();
        let mut again = HashSet::new();
        let mut states = vec![UpdateState::Idle; count];
        assert_eq!(set_cycle(&ids, &mut running, &mut again), count);
        assert_eq!(array_cycle(&mut states), count);
        assert!(running.is_empty() && again.is_empty());
        assert!(states.iter().all(|state| *state == UpdateState::Idle));
        compare(
            "lua_state",
            count,
            "coalesced_cycle",
            if count > 64 { 100 } else { 2_000 },
            || {
                black_box(set_cycle(black_box(&ids), &mut running, &mut again));
            },
            || {
                black_box(array_cycle(black_box(&mut states)));
            },
        );
    }
}
