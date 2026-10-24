//! Snapshot production: item sources to renderer snapshots, off the event loop.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::config::{Config, ItemConfig};
use crate::error::BarrsError;
use crate::render::RenderItemSnapshot;
use crate::rift::{RiftBackend, RiftBackendKind, RiftEvent, RiftSnapshot, RiftSubscription};
use crate::rift_tracker::{RiftEventsOutcome, RiftTracker};

/// One item's produced snapshot, or the error its source reported.
pub type ProducedItem = (String, Result<RenderItemSnapshot, BarrsError>);

/// Produces renderer snapshots for configured items and keeps the Rift
/// tracker current.
///
/// Source queries and Rift fetches can block, so they run on blocking tasks.
/// Each [`Self::request`] returns its result through [`Self::next`] as a
/// [`Production`] that carries the caller's tag. Every fetched Rift snapshot
/// goes into the tracker when the caller records the production, so all
/// paths that read Rift keep one cache and published signature.
pub struct SnapshotProducer<T> {
    backend: Arc<dyn RiftBackend>,
    /// Rift events for the Rift consumers of the current config.
    subscription: Option<RiftSubscription>,
    rift: RiftTracker,
    /// Sequence of the next request or Rift resync fetch.
    next_seq: u64,
    /// Sequence of the fetch that last changed the cached Rift snapshot.
    rift_seq: u64,
    resync: Resync,
    sender: mpsc::UnboundedSender<Message<T>>,
    receiver: mpsc::UnboundedReceiver<Message<T>>,
}

enum Message<T> {
    Items(Production<T>),
    Resync {
        seq: u64,
        snapshot: Option<RiftSnapshot>,
    },
}

/// A finished production request or Rift resync fetch.
pub enum Completion<T> {
    Items(Production<T>),
    /// A resync fetch updated the Rift tracker; nothing to render yet.
    RiftResync,
    /// Rift events arrived, or a Rift render debounce is due: call
    /// [`SnapshotProducer::poll_rift`].
    RiftPending,
}

/// The snapshots that one [`SnapshotProducer::request`] produced.
pub struct Production<T> {
    pub tag: T,
    /// Produced items in config order.
    items: Vec<ProducedItem>,
    rift: RiftUse,
}

impl<T> Production<T> {
    /// Splits the production into its tag, items, and the Rift data it read.
    ///
    /// Give the [`RiftUse`] to [`SnapshotProducer::record`] once the items
    /// are shown.
    #[must_use]
    pub fn into_parts(self) -> (T, Vec<ProducedItem>, RiftUse) {
        (self.tag, self.items, self.rift)
    }
}

/// The Rift data that a production read.
#[derive(Debug)]
pub struct RiftUse {
    seq: u64,
    covers_consumers: bool,
    /// The snapshot that the produced items show.
    shown: Option<RiftSnapshot>,
    /// Whether `shown` was fetched for this production, not read from the cache.
    fetched: bool,
}

#[derive(Debug, Default)]
struct Resync {
    in_flight: bool,
    again: bool,
}

struct Job {
    seq: u64,
    items: Vec<(usize, ItemConfig)>,
    rift: Option<RiftJob>,
}

struct RiftJob {
    backend: Arc<dyn RiftBackend>,
    cached: Option<RiftSnapshot>,
    covers_consumers: bool,
}

impl<T: Send + 'static> SnapshotProducer<T> {
    #[must_use]
    pub fn new(backend: Arc<dyn RiftBackend>) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            backend,
            subscription: None,
            rift: RiftTracker::default(),
            next_seq: 1,
            rift_seq: 0,
            resync: Resync::default(),
            sender,
            receiver,
        }
    }

    /// The kind of Rift backend in use now.
    #[must_use]
    pub fn backend_kind(&self) -> RiftBackendKind {
        self.backend.kind()
    }

    /// Subscribes to Rift events when `config` has Rift consumers, and
    /// drops the subscription of the previous config.
    pub fn watch(&mut self, config: &Config) {
        self.subscription = has_rift_consumers(config)
            .then(|| self.backend.subscribe())
            .flatten();
    }

    /// The cached Rift snapshot.
    #[cfg(test)]
    #[must_use]
    pub const fn rift_snapshot(&self) -> Option<&RiftSnapshot> {
        self.rift.snapshot()
    }

    /// Starts producing snapshots for the items of `config` in `item_ids`.
    ///
    /// When one of them reads Rift, the job fetches a fresh snapshot and
    /// falls back to the cached one when the fetch fails.
    pub fn request(&mut self, config: &Config, item_ids: &[String], tag: T) {
        let seq = self.take_seq();
        let items: Vec<(usize, ItemConfig)> = config
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item_ids.contains(&item.id))
            .map(|(order, item)| (order, item.clone()))
            .collect();
        let rift = items
            .iter()
            .any(|(_, item)| item.source.needs_rift())
            .then(|| RiftJob {
                backend: Arc::clone(&self.backend),
                cached: self.rift.snapshot().cloned(),
                covers_consumers: config
                    .items
                    .iter()
                    .filter(|item| item.source.needs_rift())
                    .all(|item| item_ids.contains(&item.id)),
            });
        let job = Job { seq, items, rift };
        let sender = self.sender.clone();
        tokio::task::spawn_blocking(move || {
            let (items, rift) = job.run();
            // The receiver is gone only when the daemon stops.
            let _ = sender.send(Message::Items(Production { tag, items, rift }));
        });
    }

    /// Waits for the next finished request or Rift resync fetch, Rift
    /// event, or due Rift render debounce.
    ///
    /// A resync result or Rift event goes into the Rift tracker before this
    /// returns.
    pub async fn next(&mut self) -> Completion<T> {
        let debounce = self.rift.deadline();
        let message = tokio::select! {
            message = self.receiver.recv() => message,
            Some(event) = next_rift_event(self.subscription.as_mut()) => {
                self.on_rift_events(&[event], Instant::now());
                return Completion::RiftPending;
            }
            () = sleep_until_due(debounce) => return Completion::RiftPending,
        };
        let Some(message) = message else {
            // The producer keeps a sender, so the channel never closes.
            return std::future::pending().await;
        };
        match message {
            Message::Items(production) => Completion::Items(production),
            Message::Resync { seq, snapshot } => {
                self.finish_resync(seq, snapshot, Instant::now());
                Completion::RiftResync
            }
        }
    }

    /// Applies the Rift events received since the last poll, and returns the
    /// snapshots of the Rift consumers of `config` that must render again.
    ///
    /// # Errors
    ///
    /// Returns an error when a Rift consumer cannot produce its content.
    pub fn poll_rift(
        &mut self,
        config: &Config,
        now: Instant,
    ) -> Result<Vec<RenderItemSnapshot>, BarrsError> {
        let events = self
            .subscription
            .as_mut()
            .map(RiftSubscription::drain)
            .unwrap_or_default();
        self.on_rift_events(&events, now);
        self.due_rift_renders(config, now)
    }

    /// Applies Rift events to the tracker, and starts a resync fetch when the
    /// tracker needs a fresh snapshot.
    fn on_rift_events(&mut self, events: &[RiftEvent], now: Instant) {
        if self.rift.on_events(events, now) != RiftEventsOutcome::NeedsSnapshot {
            return;
        }
        if self.resync.in_flight {
            self.resync.again = true;
            return;
        }
        self.start_resync();
    }

    /// Records the Rift data that a shown production read.
    ///
    /// A fetched snapshot newer than the cache replaces it. When the
    /// production covered every Rift consumer, its snapshot becomes the
    /// published one.
    pub fn record(&mut self, rift: RiftUse, now: Instant) {
        let shown = rift.shown.as_ref().map(RiftSnapshot::signature);
        if rift.fetched
            && let Some(fetched) = rift.shown
            && rift.seq > self.rift_seq
        {
            self.rift_seq = rift.seq;
            self.rift.store(fetched, now);
        }
        if rift.covers_consumers {
            self.rift.mark_published(shown, now);
        }
    }

    /// Ends a due Rift debounce cycle, and returns the snapshots of the Rift
    /// consumers when the cached snapshot differs from the published one.
    fn due_rift_renders(
        &mut self,
        config: &Config,
        now: Instant,
    ) -> Result<Vec<RenderItemSnapshot>, BarrsError> {
        let Some(rift) = self.rift.poll(now, has_rift_consumers(config)) else {
            return Ok(Vec::new());
        };
        config
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.source.needs_rift())
            .map(|(order, item)| snapshot_for_item(item, order, Some(rift)))
            .collect()
    }

    fn start_resync(&mut self) {
        let seq = self.take_seq();
        self.resync.in_flight = true;
        let backend = Arc::clone(&self.backend);
        let sender = self.sender.clone();
        tokio::task::spawn_blocking(move || {
            let snapshot = backend.snapshot().ok();
            let _ = sender.send(Message::Resync { seq, snapshot });
        });
    }

    fn finish_resync(&mut self, seq: u64, snapshot: Option<RiftSnapshot>, now: Instant) {
        self.resync.in_flight = false;
        if seq > self.rift_seq {
            self.rift_seq = seq;
            match snapshot {
                Some(snapshot) => self.rift.store(snapshot, now),
                None => self.rift.invalidate(),
            }
        }
        if std::mem::take(&mut self.resync.again) {
            self.start_resync();
        }
    }

    const fn take_seq(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        seq
    }
}

fn has_rift_consumers(config: &Config) -> bool {
    config.items.iter().any(|item| item.source.needs_rift())
}

impl Job {
    fn run(self) -> (Vec<ProducedItem>, RiftUse) {
        let (shown, fetched, covers_consumers) = match self.rift {
            Some(rift) => match rift.backend.snapshot() {
                Ok(fetched) => (Some(fetched), true, rift.covers_consumers),
                Err(_) => (rift.cached, false, rift.covers_consumers),
            },
            None => (None, false, false),
        };
        let items = self
            .items
            .iter()
            .map(|(order, item)| {
                (
                    item.id.clone(),
                    snapshot_for_item(item, *order, shown.as_ref()),
                )
            })
            .collect();
        let rift = RiftUse {
            seq: self.seq,
            covers_consumers,
            shown,
            fetched,
        };
        (items, rift)
    }
}

fn snapshot_for_item(
    item: &ItemConfig,
    order: usize,
    rift: Option<&RiftSnapshot>,
) -> Result<RenderItemSnapshot, BarrsError> {
    let content = item.source.content(rift)?;
    Ok(RenderItemSnapshot::from_item_config(item, order, content))
}

/// The next event of `subscription`; waits forever without one.
async fn next_rift_event(subscription: Option<&mut RiftSubscription>) -> Option<RiftEvent> {
    match subscription {
        Some(subscription) => subscription.recv().await,
        None => std::future::pending().await,
    }
}

/// Waits until `deadline`; waits forever without one.
async fn sleep_until_due(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::time::Instant;

    use crate::config::{BarConfig, Config, ItemConfig, ItemHandlers};
    use crate::rift::{FakeRiftBackend, RiftEvent, RiftEventKind, RiftSnapshot, RiftWorkspace};
    use crate::source::{ItemSource, RiftPlugin};

    use super::{Completion, Production, SnapshotProducer};

    fn item(id: &str, source: ItemSource) -> ItemConfig {
        ItemConfig {
            id: id.into(),
            label: None,
            icon: None,
            placement: None,
            interval: None,
            source,
            hover: None,
            handlers: ItemHandlers::default(),
        }
    }

    fn config() -> Config {
        Config {
            socket_path: None,
            bar: BarConfig::default(),
            items: vec![
                item(
                    "label",
                    ItemSource::Label {
                        text: "hi".into(),
                        icon: None,
                    },
                ),
                item("workspaces", ItemSource::Rift(RiftPlugin::Workspaces)),
            ],
        }
    }

    fn snapshot(current: &str) -> RiftSnapshot {
        RiftSnapshot {
            current_workspace: current.into(),
            workspaces: vec![RiftWorkspace {
                name: current.into(),
                is_current: true,
                window_count: 1,
            }],
            layout: "tiling".into(),
            window_count: 1,
        }
    }

    async fn produce(producer: &mut SnapshotProducer<()>, item_ids: &[&str]) -> Production<()> {
        let item_ids: Vec<String> = item_ids.iter().map(|&id| id.to_owned()).collect();
        producer.request(&config(), &item_ids, ());
        loop {
            match producer.next().await {
                Completion::Items(production) => return production,
                Completion::RiftResync => panic!("no resync was requested"),
                Completion::RiftPending => {}
            }
        }
    }

    fn shown_text(production: Production<()>) -> Vec<String> {
        let ((), items, _) = production.into_parts();
        items
            .into_iter()
            .map(|(_, snapshot)| snapshot.expect("snapshot").content.text().into_owned())
            .collect()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn items_that_do_not_read_rift_do_not_fetch() {
        let backend = Arc::new(FakeRiftBackend::default());
        let mut producer = SnapshotProducer::new(Arc::<FakeRiftBackend>::clone(&backend));

        let production = produce(&mut producer, &["label"]).await;

        assert_eq!(shown_text(production), vec!["hi"]);
        assert_eq!(backend.fetches(), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_failed_fetch_shows_the_cached_snapshot() {
        let backend = Arc::new(FakeRiftBackend::default());
        let mut producer = SnapshotProducer::new(Arc::<FakeRiftBackend>::clone(&backend));
        backend.set(Some(snapshot("7")));
        let ((), _, rift) = produce(&mut producer, &["workspaces"]).await.into_parts();
        producer.record(rift, Instant::now());
        backend.set(None);

        let production = produce(&mut producer, &["workspaces"]).await;

        assert_eq!(shown_text(production), vec!["7"]);
        assert_eq!(backend.fetches(), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn an_event_without_a_cached_snapshot_resyncs_and_renders_after_the_debounce() {
        let backend = Arc::new(FakeRiftBackend::default());
        let mut producer = SnapshotProducer::<()>::new(Arc::<FakeRiftBackend>::clone(&backend));
        backend.set(Some(snapshot("4")));
        let event = RiftEvent {
            kind: RiftEventKind::WorkspaceChanged,
            payload: serde_json::json!({ "workspace_index": 3 }),
        };

        producer.on_rift_events(&[event], Instant::now());

        assert!(matches!(producer.next().await, Completion::RiftResync));
        let due = Instant::now() + crate::rift_tracker::RIFT_DEBOUNCE;
        let rendered = producer
            .due_rift_renders(&config(), due)
            .expect("rift renders");
        assert_eq!(rendered.len(), 1);
        assert!(
            producer
                .due_rift_renders(&config(), due)
                .expect("no-op")
                .is_empty()
        );
    }
}
