//! The daemon's cached Rift snapshot and its render debounce.

use tokio::time::{Duration, Instant};

use crate::rift::{RiftApplyResult, RiftEvent, RiftSnapshot, apply_event};

/// Time the tracker waits after a Rift change before it asks for a render.
pub const RIFT_DEBOUNCE: Duration = Duration::from_millis(16);

/// What the tracker needs after it applies a batch of Rift events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiftEventsOutcome {
    /// The cached snapshot is current; a change, if any, is debounced.
    Applied,
    /// The tracker has no usable snapshot; the caller must fetch one and
    /// give it to [`RiftTracker::store`], or call [`RiftTracker::invalidate`]
    /// when the fetch fails.
    NeedsSnapshot,
}

/// Caches the latest Rift snapshot and decides when Rift consumers must
/// render it again.
///
/// The tracker remembers the signature of the snapshot that the Rift
/// consumers show (the published signature). A new snapshot whose signature
/// differs starts a debounce; [`Self::poll`] ends every debounce cycle, and
/// asks for a render only when the snapshot differs from the published one.
/// The caller passes the time explicitly, so the transitions are deterministic.
#[derive(Debug, Default)]
pub struct RiftTracker {
    snapshot: Option<RiftSnapshot>,
    published: Option<u64>,
    deadline: Option<Instant>,
}

impl RiftTracker {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            snapshot: None,
            published: None,
            deadline: None,
        }
    }

    /// The cached snapshot, if the tracker has one.
    #[must_use]
    pub const fn snapshot(&self) -> Option<&RiftSnapshot> {
        self.snapshot.as_ref()
    }

    /// Applies Rift events to the cached snapshot.
    ///
    /// A change starts the debounce. Without a cached snapshot, or when an
    /// event cannot be applied exactly, the tracker asks for a fresh snapshot.
    pub fn on_events(&mut self, events: &[RiftEvent], now: Instant) -> RiftEventsOutcome {
        if events.is_empty() {
            return RiftEventsOutcome::Applied;
        }
        let Some(snapshot) = self.snapshot.as_mut() else {
            return RiftEventsOutcome::NeedsSnapshot;
        };
        let mut changed = false;
        for event in events {
            match apply_event(snapshot, event) {
                RiftApplyResult::NoChange => {}
                RiftApplyResult::Updated => changed = true,
                RiftApplyResult::RequiresResync => return RiftEventsOutcome::NeedsSnapshot,
            }
        }
        if changed {
            self.schedule(now);
        }
        RiftEventsOutcome::Applied
    }

    /// Caches a freshly fetched snapshot.
    ///
    /// When it differs from the published snapshot, the debounce starts.
    pub fn store(&mut self, snapshot: RiftSnapshot, now: Instant) {
        let changed = self.published != Some(snapshot.signature());
        self.snapshot = Some(snapshot);
        if changed {
            self.schedule(now);
        } else {
            self.deadline = None;
        }
    }

    /// Drops the cached snapshot after a failed fetch and ends any debounce.
    pub fn invalidate(&mut self) {
        self.snapshot = None;
        self.deadline = None;
    }

    /// Records that every Rift consumer now shows the snapshot with
    /// `signature`.
    ///
    /// The debounce ends when the cache holds that snapshot or is empty.
    /// When the cache holds another snapshot, a debounce starts, so the
    /// consumers render the cached one.
    pub fn mark_published(&mut self, signature: Option<u64>, now: Instant) {
        self.published = signature;
        let cached = self.snapshot.as_ref().map(RiftSnapshot::signature);
        if cached.is_none() || cached == signature {
            self.deadline = None;
        } else if self.deadline.is_none() {
            self.schedule(now);
        }
    }

    /// Ends a due debounce cycle.
    ///
    /// Returns the snapshot that the Rift consumers must render, and records
    /// it as published, only when `has_consumers` is true and the snapshot
    /// differs from the published one. Every other due cycle ends without a
    /// render, so later polls do no work until the next change.
    pub fn poll(&mut self, now: Instant, has_consumers: bool) -> Option<&RiftSnapshot> {
        if self.deadline.is_none_or(|deadline| deadline > now) {
            return None;
        }
        self.deadline = None;
        if !has_consumers {
            return None;
        }
        let signature = self.snapshot.as_ref()?.signature();
        if self.published == Some(signature) {
            return None;
        }
        self.published = Some(signature);
        self.snapshot.as_ref()
    }

    fn schedule(&mut self, now: Instant) {
        self.deadline = now.checked_add(RIFT_DEBOUNCE);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tokio::time::Instant;

    use crate::rift::{RiftEvent, RiftEventKind, RiftSnapshot, RiftWorkspace};

    use super::{RIFT_DEBOUNCE, RiftEventsOutcome, RiftTracker};

    fn snapshot(current: &str) -> RiftSnapshot {
        RiftSnapshot {
            current_workspace: current.into(),
            workspaces: ["1", "2"]
                .into_iter()
                .map(|name| RiftWorkspace {
                    name: name.into(),
                    is_current: name == current,
                    window_count: 1,
                    has_windows: true,
                })
                .collect(),
            layout: "tiling".into(),
            window_count: 1,
        }
    }

    /// Rift names workspaces by their one-based index.
    fn switch_to(workspace_index: u64) -> RiftEvent {
        RiftEvent {
            kind: RiftEventKind::WorkspaceChanged,
            payload: json!({ "workspace_index": workspace_index }),
        }
    }

    fn name_only_event() -> RiftEvent {
        RiftEvent {
            kind: RiftEventKind::WorkspaceChanged,
            payload: json!({ "workspace_name": "Main" }),
        }
    }

    /// A tracker whose consumers show `shown`.
    fn published(shown: &RiftSnapshot, now: Instant) -> RiftTracker {
        let mut tracker = RiftTracker::new();
        tracker.store(shown.clone(), now);
        tracker.mark_published(Some(shown.signature()), now);
        tracker
    }

    #[test]
    fn a_changing_event_renders_once_after_the_debounce() {
        let now = Instant::now();
        let mut tracker = published(&snapshot("1"), now);

        let outcome = tracker.on_events(&[switch_to(1)], now);

        assert_eq!(outcome, RiftEventsOutcome::Applied);
        assert!(tracker.poll(now, true).is_none());
        let due = now + RIFT_DEBOUNCE;
        let rendered = tracker.poll(due, true).expect("render after the debounce");
        assert_eq!(rendered.current_workspace, "2");
        assert!(tracker.poll(due, true).is_none());
    }

    #[test]
    fn a_due_cycle_without_consumers_ends_without_a_render() {
        let now = Instant::now();
        let mut tracker = published(&snapshot("1"), now);
        tracker.store(snapshot("2"), now);

        let due = now + RIFT_DEBOUNCE;
        assert!(tracker.poll(due, false).is_none());

        assert!(tracker.poll(due, true).is_none());
    }

    #[test]
    fn an_unchanged_signature_ends_the_cycle_and_a_later_change_renders() {
        let now = Instant::now();
        let mut tracker = published(&snapshot("1"), now);

        tracker.store(snapshot("1"), now);
        let due = now + RIFT_DEBOUNCE;
        assert!(tracker.poll(due, true).is_none());

        tracker.store(snapshot("2"), due);
        let later = due + RIFT_DEBOUNCE;
        let rendered = tracker.poll(later, true).expect("changed snapshot renders");
        assert_eq!(rendered.current_workspace, "2");
    }

    #[test]
    fn an_inexact_event_or_an_empty_cache_needs_a_fresh_snapshot() {
        let now = Instant::now();
        let mut tracker = RiftTracker::new();
        assert_eq!(
            tracker.on_events(&[switch_to(1)], now),
            RiftEventsOutcome::NeedsSnapshot
        );

        tracker = published(&snapshot("1"), now);
        assert_eq!(
            tracker.on_events(&[name_only_event()], now),
            RiftEventsOutcome::NeedsSnapshot
        );
        tracker.invalidate();

        assert!(tracker.snapshot().is_none());
        assert!(tracker.poll(now + RIFT_DEBOUNCE, true).is_none());
    }

    #[test]
    fn publishing_the_cached_snapshot_ends_its_debounce() {
        let now = Instant::now();
        let mut tracker = RiftTracker::new();
        let fresh = snapshot("2");
        tracker.store(fresh.clone(), now);

        tracker.mark_published(Some(fresh.signature()), now);

        assert!(tracker.poll(now + RIFT_DEBOUNCE, true).is_none());
        assert_eq!(
            tracker.snapshot().map(RiftSnapshot::signature),
            Some(fresh.signature())
        );
    }

    #[test]
    fn publishing_an_older_snapshot_renders_the_cached_one() {
        let now = Instant::now();
        let mut tracker = published(&snapshot("1"), now);
        tracker.store(snapshot("2"), now);
        let due = now + RIFT_DEBOUNCE;
        assert!(tracker.poll(due, true).is_some());

        tracker.mark_published(Some(snapshot("1").signature()), due);

        let rendered = tracker
            .poll(due + RIFT_DEBOUNCE, true)
            .expect("consumers catch up with the cache");
        assert_eq!(rendered.current_workspace, "2");
    }
}
