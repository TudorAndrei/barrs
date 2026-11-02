//! Compare the retained old per-item path with production batching.
//! No native windows are created.
use super::*;
use crate::measurements::{compare, config};
use std::hint::black_box;

fn snapshots(config: &Config, text: &str) -> Vec<RenderItemSnapshot> {
    config
        .items
        .iter()
        .enumerate()
        .map(|(order, item)| {
            RenderItemSnapshot::from_item_config(item, order, ItemContent::plain(text))
        })
        .collect()
}

// Retain the old successful update path as the behavior and timing reference.
fn old_render_item(renderer: &mut NativeRenderer, update: &RenderItemSnapshot) {
    let height = renderer.state.bar_height.max(1.0);
    renderer.state.bar_height = height;
    renderer
        .state
        .items
        .retain(|item| item.snapshot.id != update.id);
    renderer
        .state
        .items
        .push(PositionedItemSnapshot::unplaced(update.clone(), height));
    renderer.state.arrange();
    renderer.publish_scene().expect("mock publish");
}

#[test]
#[ignore = "manual data-layout benchmark"]
fn measure_renderer_batch() {
    for count in [1, 6, 64, 256] {
        let config = config(count);
        let update_counts = [1, count.min(3), count];
        for (case, update_count) in update_counts.into_iter().enumerate() {
            if update_counts[..case].contains(&update_count) {
                continue;
            }
            let initial = snapshots(&config, "ready");
            let updates = [snapshots(&config, "value-a"), snapshots(&config, "value-b")];
            let mut current = NativeRenderer::default();
            let mut candidate = NativeRenderer::default();
            current.reconcile(&config, &initial).expect("initial scene");
            candidate
                .reconcile(&config, &initial)
                .expect("initial scene");
            for snapshot in &updates[0][..update_count] {
                old_render_item(&mut current, snapshot);
            }
            candidate
                .render_items(&updates[0][..update_count])
                .expect("batch");
            assert_eq!(current.state, candidate.state);
            assert_eq!(
                host_scene_plan(&current.state.scene()),
                host_scene_plan(&candidate.state.scene())
            );
            let mut old_turn = 0usize;
            let mut new_turn = 0usize;
            compare(
                "renderer",
                count,
                &format!("{update_count}_updates"),
                if count >= 64 { 12 } else { 200 },
                || {
                    old_turn ^= 1;
                    for snapshot in black_box(&updates[old_turn][..update_count]) {
                        old_render_item(&mut current, snapshot);
                    }
                    black_box(&current.state);
                },
                || {
                    new_turn ^= 1;
                    candidate
                        .render_items(black_box(&updates[new_turn][..update_count]))
                        .expect("batch");
                    black_box(&candidate.state);
                },
            );
            assert_eq!(current.state, candidate.state);
            assert_eq!(
                host_scene_plan(&current.state.scene()),
                host_scene_plan(&candidate.state.scene())
            );
        }
    }
}

#[test]
#[ignore = "manual data-layout benchmark"]
fn measure_pointer_bounds() {
    for count in [1, 6, 64, 1024] {
        let config = config(count);
        let mut state = NativeSurfaceState {
            bar_height: BAR_HEIGHT,
            items: snapshots(&config, "ready")
                .into_iter()
                .map(|snapshot| PositionedItemSnapshot::unplaced(snapshot, BAR_HEIGHT))
                .collect(),
            ..NativeSurfaceState::default()
        };
        state.arrange();
        let bounds: Vec<ItemFrame> = state.items.iter().map(|item| item.frame.clone()).collect();
        let mut points: Vec<_> = state
            .items
            .iter()
            .map(|item| (item.frame.x + item.frame.width / 2.0, 10.0))
            .collect();
        points.extend([(-1.0, 10.0), (1.0, -1.0), (0.0, 0.0)]);
        let old_hit = |(x, y)| {
            state
                .items
                .iter()
                .find_map(|item| item.frame.contains(x, y).then(|| item.snapshot.id.clone()))
        };
        let new_hit = |(x, y)| {
            bounds
                .iter()
                .position(|frame| frame.contains(x, y))
                .map(|index| state.items[index].snapshot.id.clone())
        };
        for point in &points {
            assert_eq!(old_hit(*point), new_hit(*point));
        }
        let mut old_index = 0usize;
        let mut new_index = 0usize;
        compare(
            "pointer",
            count,
            "mixed_hits",
            20_000,
            || {
                let point = black_box(points[old_index % points.len()]);
                old_index = old_index.wrapping_add(1);
                black_box(old_hit(point));
            },
            || {
                let point = black_box(points[new_index % points.len()]);
                new_index = new_index.wrapping_add(1);
                black_box(new_hit(point));
            },
        );
    }
}
