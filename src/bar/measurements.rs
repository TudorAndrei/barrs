use super::*;
use crate::content::ItemContent;
use crate::measurements::{compare, config};
use crate::render::NoopRenderer;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use std::hint::black_box;

struct FlatShown(Vec<RenderItemSnapshot>);
impl Serialize for FlatShown {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for snapshot in &self.0 {
            map.serialize_entry(&snapshot.id, snapshot)?;
        }
        map.end()
    }
}

#[test]
#[ignore = "manual data-layout benchmark"]
fn measure_shown_snapshots() {
    for count in [1, 6, 64, 1024] {
        let config = config(count);
        let initial: Vec<_> = config
            .items
            .iter()
            .enumerate()
            .map(|(order, item)| {
                RenderItemSnapshot::from_item_config(item, order, ItemContent::plain("ready"))
            })
            .collect();
        let updates: Vec<_> = config
            .items
            .iter()
            .enumerate()
            .map(|(order, item)| {
                RenderItemSnapshot::from_item_config(item, order, ItemContent::plain("changed"))
            })
            .collect();
        let mut current = Bar::new(NoopRenderer);
        current
            .reconcile(&config, initial.clone())
            .expect("initial state");
        let mut candidate = FlatShown(initial);
        let mut renderer = NoopRenderer;
        for update in &updates {
            current.show(update.clone()).expect("show");
            renderer.render_item(update).expect("render");
            candidate.0[update.order] = update.clone();
        }
        assert_eq!(
            serde_json::to_value(current.items()).expect("map JSON"),
            serde_json::to_value(&candidate).expect("array JSON")
        );
        compare(
            "shown_update",
            count,
            "all_items",
            if count > 64 { 100 } else { 2_000 },
            || {
                for update in black_box(&updates) {
                    current.show(update.clone()).expect("show");
                }
                black_box(current.items());
            },
            || {
                for update in black_box(&updates) {
                    renderer.render_item(update).expect("render");
                    candidate.0[update.order] = update.clone();
                }
                black_box(&candidate);
            },
        );
        compare(
            "shown_dump",
            count,
            "JSON_object",
            if count > 64 { 20 } else { 200 },
            || {
                black_box(serde_json::to_value(black_box(current.items())).expect("map JSON"));
            },
            || {
                black_box(serde_json::to_value(black_box(&candidate)).expect("array JSON"));
            },
        );
    }
}
