use super::*;
use crate::measurements::{compare, config};
use crate::rift::FakeRiftBackend;
use std::hint::black_box;

fn select_ids(config: &Config, ids: &[String]) -> Vec<(usize, ItemConfig)> {
    // Exact selection expression from SnapshotProducer::request.
    config
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| ids.contains(&item.id))
        .map(|(order, item)| (order, item.clone()))
        .collect()
}

fn select_indices(config: &Config, indices: &[usize]) -> Vec<(usize, ItemConfig)> {
    // Contract: sorted unique indices belonging to this config epoch.
    indices
        .iter()
        .map(|&order| (order, config.items[order].clone()))
        .collect()
}

fn request_indices(producer: &mut SnapshotProducer<()>, config: &Config, indices: &[usize]) {
    let job = Job {
        seq: producer.take_seq(),
        items: select_indices(config, indices),
        rift: None,
    };
    let sender = producer.sender.clone();
    tokio::task::spawn_blocking(move || {
        let (items, rift) = job.run();
        let _ = sender.send(Message::Items(Production {
            tag: (),
            items,
            rift,
        }));
    });
}

async fn receive(producer: &mut SnapshotProducer<()>) -> Vec<RenderItemSnapshot> {
    let Completion::Items(production) = producer.next().await else {
        panic!("expected items");
    };
    production
        .items
        .into_iter()
        .map(|(_, snapshot)| snapshot.expect("label snapshot"))
        .collect()
}

#[test]
#[ignore = "manual data-layout benchmark"]
fn measure_producer_selection() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    for count in [1, 6, 64, 1024] {
        let config = config(count);
        for (case, stride) in [1, count].into_iter().enumerate() {
            if count == 1 && case == 1 {
                continue;
            }
            let indices: Vec<_> = (0..count).step_by(stride).collect();
            let ids: Vec<_> = indices
                .iter()
                .map(|&index| config.items[index].id.clone())
                .collect();
            let old = select_ids(&config, &ids);
            let new = select_indices(&config, &indices);
            let render = |items| {
                Job {
                    seq: 1,
                    items,
                    rift: None,
                }
                .run()
                .0
                .into_iter()
                .map(|(id, snapshot)| (id, snapshot.expect("snapshot")))
                .collect::<Vec<_>>()
            };
            assert_eq!(render(old), render(new));
            let workload = format!("{}_requested", indices.len());
            compare(
                "producer_select",
                count,
                &workload,
                if count > 64 { 100 } else { 2_000 },
                || {
                    black_box(select_ids(black_box(&config), black_box(&ids)));
                },
                || {
                    black_box(select_indices(black_box(&config), black_box(&indices)));
                },
            );
            compare(
                "producer_resolve",
                count,
                &workload,
                if count > 64 { 100 } else { 2_000 },
                || {
                    black_box(select_ids(black_box(&config), black_box(&ids)));
                },
                || {
                    let resolved: Vec<_> = black_box(&ids)
                        .iter()
                        .filter_map(|id| config.item_with_order(id).map(|(order, _)| order))
                        .collect();
                    black_box(select_indices(black_box(&config), black_box(&resolved)));
                },
            );
            let mut current = SnapshotProducer::new(Arc::new(FakeRiftBackend::default()));
            let mut candidate = SnapshotProducer::new(Arc::new(FakeRiftBackend::default()));
            runtime.block_on(async {
                current.request(&config, &ids, ());
                request_indices(&mut candidate, &config, &indices);
                assert_eq!(receive(&mut current).await, receive(&mut candidate).await);
            });
            compare(
                "producer_worker",
                count,
                &workload,
                if count > 64 { 20 } else { 200 },
                || {
                    runtime.block_on(async {
                        current.request(black_box(&config), black_box(&ids), ());
                        black_box(receive(&mut current).await);
                    });
                },
                || {
                    runtime.block_on(async {
                        request_indices(&mut candidate, black_box(&config), black_box(&indices));
                        black_box(receive(&mut candidate).await);
                    });
                },
            );
        }
    }
}
