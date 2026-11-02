//! Lua item dispatch: item events and update requests to the Lua executor of
//! the current config generation, and their results back to the event loop.

use tokio::sync::mpsc;

use crate::config::{Config, ItemConfig};
use crate::content::ItemContent;
use crate::error::BarrsError;
use crate::ipc::{EventKind, EventPayload};
use crate::lua_executor::LuaExecutor;

/// Sends item events and update requests to the Lua executor of one config
/// generation, and returns each finished call through [`Self::next`].
///
/// Nothing here waits for Lua: a slow call holds only the executor thread
/// (ADR-0020), never the caller. The executor runs calls in order, so a
/// handler always runs before the update that its event asks for.
///
/// It owns the config epoch. Results of an older generation's updates are
/// discarded. An item has at most one update queued or running; an update
/// that an event asks for while one is in flight runs once more after it.
/// The tag `T` comes back with the handler result of each delivered event.
pub struct LuaDispatch<T> {
    executor: LuaExecutor,
    epoch: u64,
    /// One state per current config item, indexed by config order.
    updates: Vec<UpdateState>,
    sender: mpsc::UnboundedSender<Message<T>>,
    receiver: mpsc::UnboundedReceiver<Message<T>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateState {
    Idle,
    Running,
    Again,
}

impl UpdateState {
    /// A scheduled request does not queue a duplicate.
    fn request(&mut self) -> bool {
        if *self != Self::Idle {
            return false;
        }
        *self = Self::Running;
        true
    }

    /// An event during a call requests exactly one further call.
    fn request_now(&mut self) -> bool {
        if self.request() {
            true
        } else {
            *self = Self::Again;
            false
        }
    }

    fn complete(&mut self) -> bool {
        let repeat = *self == Self::Again;
        *self = if repeat { Self::Running } else { Self::Idle };
        repeat
    }
}

enum Message<T> {
    Handled {
        epoch: u64,
        order: usize,
        tag: T,
        item_id: String,
        event: EventKind,
        /// The item's update function, when it has one.
        update: Option<String>,
        result: Result<(), BarrsError>,
    },
    Updated {
        epoch: u64,
        order: usize,
        item_id: String,
        function: String,
        outcome: Result<ItemContent, BarrsError>,
    },
}

/// A finished Lua call, as [`LuaDispatch::next`] returns it.
#[derive(Debug)]
pub enum Dispatched<T> {
    /// The handler of a delivered event ended, or the item has no handler
    /// for the event. Events of an older generation also end here, so every
    /// tag comes back.
    Handled {
        tag: T,
        item_id: String,
        result: Result<(), BarrsError>,
        /// Whether the caller must produce a new snapshot of the item: true
        /// after a click, right click, or scroll on an item of the current
        /// generation without an update function, when the handler succeeded.
        refresh: bool,
    },
    /// An update function of the current generation ended. A failed call
    /// gives unavailable content.
    Updated {
        item_id: String,
        content: ItemContent,
    },
}

impl<T: Send + 'static> LuaDispatch<T> {
    #[must_use]
    pub fn new(executor: LuaExecutor, config: &Config) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            executor,
            epoch: 0,
            updates: vec![UpdateState::Idle; config.items.len()],
            sender,
            receiver,
        }
    }

    /// The config epoch: it changes each time [`Self::replace`] starts a new
    /// generation.
    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Starts a new config generation on `executor`.
    ///
    /// Calls already queued on the old executor still run. Their update
    /// results are discarded; their handler results still come back.
    pub fn replace(&mut self, executor: LuaExecutor, config: &Config) {
        self.executor = executor;
        self.epoch = self.epoch.wrapping_add(1);
        self.updates = vec![UpdateState::Idle; config.items.len()];
    }

    /// Queues the handler of `item` for the event in `payload`.
    ///
    /// The handler result comes back with `tag` from [`Self::next`]. A
    /// click, right click, or scroll on an item with an update function makes
    /// its update due as soon as the handler succeeds.
    /// `order` is the index of `item` in the current config. The daemon
    /// resolves this index before it queues the event.
    pub fn deliver(&mut self, order: usize, item: &ItemConfig, payload: &EventPayload, tag: T) {
        let epoch = self.epoch;
        let item_id = item.id.clone();
        let event = payload.event;
        let update = item.source.lua_update().map(str::to_owned);
        let sender = self.sender.clone();
        let done = move |result| {
            let _ = sender.send(Message::Handled {
                epoch,
                order,
                tag,
                item_id,
                event,
                update,
                result,
            });
        };
        let Some(function) = item.handlers.get(event) else {
            done(Ok(()));
            return;
        };
        match serde_json::to_value(payload) {
            Ok(context) => self.executor.call_handler(function, context, done),
            Err(error) => done(Err(error.into())),
        }
    }

    /// Queues Lua updates by index into the current configuration.
    /// Already running, non-Lua, and out-of-range items are skipped.
    pub fn request_updates(&mut self, config: &Config, item_orders: &[usize]) {
        for &order in item_orders {
            let Some(item) = config.items.get(order) else {
                continue;
            };
            let Some(function) = item.source.lua_update() else {
                continue;
            };
            let Some(state) = self.updates.get_mut(order) else {
                continue;
            };
            if state.request() {
                self.submit_update(order, &item.id, function);
            }
        }
    }

    /// Waits for the next finished call of the current generation, or the
    /// next handler result of any generation.
    ///
    /// Cancel-safe: a call that ends while nobody waits stays queued.
    pub async fn next(&mut self) -> Dispatched<T> {
        loop {
            let Some(message) = self.receiver.recv().await else {
                // `self` holds a sender, so the channel cannot close.
                continue;
            };
            match message {
                Message::Handled {
                    epoch,
                    order,
                    tag,
                    item_id,
                    event,
                    update,
                    result,
                } => {
                    let follow_up = epoch == self.epoch && result.is_ok() && !event.is_hover();
                    let refresh = follow_up && update.is_none();
                    if follow_up && let Some(function) = update {
                        self.update_now(order, &item_id, &function);
                    }
                    return Dispatched::Handled {
                        tag,
                        item_id,
                        result,
                        refresh,
                    };
                }
                Message::Updated {
                    epoch,
                    order,
                    item_id,
                    function,
                    outcome,
                } => {
                    if epoch != self.epoch {
                        continue;
                    }
                    let Some(state) = self.updates.get_mut(order) else {
                        continue;
                    };
                    if state.complete() {
                        self.submit_update(order, &item_id, &function);
                    }
                    let content = outcome.unwrap_or_else(|error| {
                        eprintln!("barrs: lua update for {item_id} failed: {error}");
                        ItemContent::Unavailable
                    });
                    return Dispatched::Updated { item_id, content };
                }
            }
        }
    }

    /// Makes an update due now; an update in flight runs once more after it.
    fn update_now(&mut self, order: usize, item_id: &str, function: &str) {
        let Some(state) = self.updates.get_mut(order) else {
            return;
        };
        if state.request_now() {
            self.submit_update(order, item_id, function);
        }
    }

    fn submit_update(&self, order: usize, item_id: &str, function: &str) {
        let epoch = self.epoch;
        let sender = self.sender.clone();
        let id = item_id.to_owned();
        let name = function.to_owned();
        self.executor
            .request_update(item_id, function, move |outcome| {
                let _ = sender.send(Message::Updated {
                    epoch,
                    order,
                    item_id: id,
                    function: name,
                    outcome,
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::time::Duration;

    use tempfile::tempdir;

    use super::{Dispatched, LuaDispatch};
    use crate::cli::TriggerEvent;
    use crate::config::Config;
    use crate::ipc::EventPayload;
    use crate::lua_executor::LuaExecutor;

    /// Starts an executor for one Lua item `status` whose update function
    /// counts its calls, prefixed by `generation`.
    fn start(path: &Path, generation: &str, prefix: &str) -> (LuaExecutor, Config) {
        fs::write(
            path,
            format!(
                r#"
calls = 0
function status(ctx)
  calls = calls + 1
  return "{generation}" .. calls
end
function noop(ctx) end
return {{ items = {{ {prefix} {{
  id = "status",
  plugin = {{ kind = "lua", update = "status" }},
  handlers = {{ click = "noop", hover_update = "noop" }},
}} }} }}
"#
            ),
        )
        .expect("write config");
        LuaExecutor::start(path).expect("start executor")
    }

    fn deliver(dispatch: &mut LuaDispatch<&'static str>, config: &Config, event: TriggerEvent) {
        let payload = EventPayload::from_trigger("status".into(), event);
        dispatch.deliver(0, &config.items[0], &payload, "event");
    }

    async fn next(dispatch: &mut LuaDispatch<&'static str>) -> Dispatched<&'static str> {
        tokio::time::timeout(Duration::from_secs(5), dispatch.next())
            .await
            .expect("a lua call ends")
    }

    async fn next_text(dispatch: &mut LuaDispatch<&'static str>) -> String {
        match next(dispatch).await {
            Dispatched::Updated { item_id, content } => {
                assert_eq!(item_id, "status");
                content.text().into_owned()
            }
            other @ Dispatched::Handled { .. } => panic!("expected an update, got {other:?}"),
        }
    }

    async fn next_handled(dispatch: &mut LuaDispatch<&'static str>) {
        match next(dispatch).await {
            Dispatched::Handled {
                tag,
                result,
                refresh,
                ..
            } => {
                assert_eq!(tag, "event");
                result.expect("handler succeeds");
                assert!(!refresh, "a Lua item updates instead of a refresh");
            }
            other @ Dispatched::Updated { .. } => {
                panic!("expected a handler result, got {other:?}")
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn an_update_in_flight_is_not_requested_again() {
        let dir = tempdir().expect("tempdir");
        let (executor, config) = start(&dir.path().join("barrs.lua"), "v", "");
        let mut dispatch = LuaDispatch::new(executor, &config);
        let due = [0];

        dispatch.request_updates(&config, &due);
        dispatch.request_updates(&config, &due);
        // A hover handler queues behind any update and requests none.
        deliver(&mut dispatch, &config, TriggerEvent::HoverUpdate);

        assert_eq!(next_text(&mut dispatch).await, "v1");
        next_handled(&mut dispatch).await;
        dispatch.request_updates(&config, &due);
        assert_eq!(next_text(&mut dispatch).await, "v2");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_click_makes_the_update_due_after_its_handler() {
        let dir = tempdir().expect("tempdir");
        let (executor, config) = start(&dir.path().join("barrs.lua"), "v", "");
        let mut dispatch = LuaDispatch::new(executor, &config);

        deliver(&mut dispatch, &config, TriggerEvent::Click);
        next_handled(&mut dispatch).await;
        assert_eq!(next_text(&mut dispatch).await, "v1");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_click_during_an_update_runs_it_once_more() {
        let dir = tempdir().expect("tempdir");
        let (executor, config) = start(&dir.path().join("barrs.lua"), "v", "");
        let mut dispatch = LuaDispatch::new(executor, &config);

        deliver(&mut dispatch, &config, TriggerEvent::Click);
        deliver(&mut dispatch, &config, TriggerEvent::Click);
        dispatch.request_updates(&config, &[0]);
        // Both handlers finish before the scheduled update is applied.
        next_handled(&mut dispatch).await;
        next_handled(&mut dispatch).await;
        assert_eq!(next_text(&mut dispatch).await, "v1");
        deliver(&mut dispatch, &config, TriggerEvent::HoverUpdate);
        assert_eq!(next_text(&mut dispatch).await, "v2");
        next_handled(&mut dispatch).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn updates_of_an_older_generation_are_discarded() {
        for prefix in ["", r#"{ id = "label", label = "fixed" },"#] {
            let dir = tempdir().expect("tempdir");
            let (old, config) = start(&dir.path().join("old.lua"), "old", "");
            let (new, new_config) = start(&dir.path().join("new.lua"), "new", prefix);
            let mut dispatch = LuaDispatch::new(old, &config);
            let epoch = dispatch.epoch();

            dispatch.request_updates(&config, &[0]);
            // The old update result is queued before this handler result.
            deliver(&mut dispatch, &config, TriggerEvent::Click);
            dispatch.replace(new, &new_config);
            assert_ne!(dispatch.epoch(), epoch);
            // Also cover a shifted Lua slot, a non-Lua slot, and invalid indices.
            dispatch.request_updates(&new_config, &[0, 1, usize::MAX]);

            let mut texts = Vec::new();
            let mut handled = 0;
            for _ in 0..2 {
                match next(&mut dispatch).await {
                    Dispatched::Updated { content, .. } => texts.push(content.text().into_owned()),
                    Dispatched::Handled {
                        result, refresh, ..
                    } => {
                        result.expect("old handler still answers");
                        assert!(!refresh);
                        handled += 1;
                    }
                }
            }
            assert_eq!((texts, handled), (vec!["new1".to_owned()], 1));
        }
    }
}

#[cfg(test)]
mod measurements;
