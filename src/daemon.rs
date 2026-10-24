use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{self, Duration, Instant};

use crate::config::{Config, ItemConfig};
use crate::content::ItemContent;
use crate::error::BarrsError;
use crate::ipc::{
    EventPayload, Request, Response, read_request_frame, send_request, write_response,
};
use crate::lua_dispatch::{Dispatched, LuaDispatch};
use crate::lua_executor::LuaExecutor;
use crate::producer::{Completion, ProducedItem, RiftUse, SnapshotProducer};
use crate::render::{RenderItemSnapshot, Renderer};
use crate::rift::{RiftBackend, RiftBackendKind, RiftSubscription};

const EVENT_TICK_MS: u64 = 16;
const POLL_TICK_MS: u64 = 250;
const PENDING_IPC_REQUESTS: usize = 64;
/// Longest time the daemon waits at stop for connection tasks to send replies.
const CONNECTION_DRAIN_MS: u64 = 250;

pub struct Daemon<R: Renderer> {
    config_path: PathBuf,
    state: DaemonState<R>,
    producer: SnapshotProducer<Purpose>,
    /// Whether a scheduled refresh is producing snapshots.
    refresh_in_flight: bool,
    /// A loaded config that waits for its snapshots before it replaces the
    /// current one.
    staged_reload: Option<StagedReload>,
    reload_ticket: u64,
}

/// Answers the item trigger request that delivered an event; renderer events
/// have none.
type EventReply = Option<oneshot::Sender<Response>>;

async fn serve_connection(mut stream: UnixStream, sender: mpsc::Sender<IncomingRequest>) {
    let request = match read_request_frame(&mut stream).await {
        Ok(request) => request,
        Err(error) => {
            let _ = write_response(&mut stream, &error.response()).await;
            return;
        }
    };
    let (response_sender, response_receiver) = oneshot::channel();
    if sender
        .send(IncomingRequest {
            request,
            response: response_sender,
        })
        .await
        .is_err()
    {
        let _ = write_response(
            &mut stream,
            &Response::Error {
                message: "daemon stopped before handling request".into(),
            },
        )
        .await;
        return;
    }
    let response = response_receiver.await.unwrap_or_else(|_| Response::Error {
        message: "daemon stopped before responding".into(),
    });
    let _ = write_response(&mut stream, &response).await;
}

/// Why the daemon requested a snapshot production, and what it does with
/// the result.
enum Purpose {
    /// The first render, before the daemon reports readiness.
    Startup,
    /// A refresh of scheduled items that are due.
    Scheduled { epoch: u64, item_ids: Vec<String> },
    /// A refresh of one item after an event; `reply` answers a trigger request.
    Event {
        epoch: u64,
        reply: Option<oneshot::Sender<Response>>,
    },
    /// The complete snapshot set of the staged reload with `ticket`.
    Reload { ticket: u64 },
}

/// A loaded config and Lua runtime that replace the current ones when their
/// snapshots are ready and the renderer reconciles them (ADR-0010).
struct StagedReload {
    ticket: u64,
    config: Config,
    lua: LuaExecutor,
    backend: RiftBackendKind,
    reply: oneshot::Sender<Response>,
}

struct IncomingRequest {
    request: Request,
    response: oneshot::Sender<Response>,
}

struct DaemonState<R: Renderer> {
    config: Config,
    /// Runs the Lua calls of the current config and owns the config epoch.
    lua: LuaDispatch<EventReply>,
    backend: RiftBackendKind,
    rift_subscription: Option<RiftSubscription>,
    renderer: R,
    item_states: HashMap<String, RenderItemSnapshot>,
    refresh_deadlines: HashMap<String, Instant>,
}

impl<R: Renderer> Daemon<R> {
    /// Creates a daemon from a config, the Lua executor that loaded it, and
    /// the Rift backend that its snapshots read.
    ///
    /// # Errors
    ///
    /// Returns an error if the daemon state cannot be initialized.
    pub fn new_loaded(
        config_path: PathBuf,
        config: Config,
        lua: LuaExecutor,
        renderer: R,
        rift_backend: Arc<dyn RiftBackend>,
    ) -> Result<Self, BarrsError> {
        let producer = SnapshotProducer::new(rift_backend);
        let backend = producer.backend_kind();
        let rift_subscription = producer.subscribe(&config);
        let state = DaemonState {
            refresh_deadlines: build_refresh_deadlines(&config, Instant::now(), backend),
            config,
            lua: LuaDispatch::new(lua),
            backend,
            rift_subscription,
            renderer,
            item_states: HashMap::new(),
        };
        Ok(Self {
            config_path,
            state,
            producer,
            refresh_in_flight: false,
            staged_reload: None,
            reload_ticket: 0,
        })
    }

    #[cfg(test)]
    fn new(config_path: PathBuf, config: Config, renderer: R) -> Result<Self, BarrsError> {
        let (lua, _) = LuaExecutor::start(&config_path)?;
        Self::new_loaded(
            config_path,
            config,
            lua,
            renderer,
            Arc::new(crate::rift::FakeRiftBackend::default()),
        )
    }

    /// Serves IPC requests and refreshes bar items until a stop request arrives.
    ///
    /// # Errors
    ///
    /// Returns an error if the socket cannot be bound or removed, or if the
    /// initial item refresh fails.
    pub async fn run(mut self) -> Result<(), BarrsError> {
        let socket_path = self.state.config.socket_path();
        let listener = bind_daemon_socket(&socket_path).await?;
        if let Err(error) = self.refresh_all_items().await {
            cleanup_socket(&socket_path)?;
            return Err(error);
        }
        signal_startup_ready();
        let (request_sender, mut request_receiver) = mpsc::channel(PENDING_IPC_REQUESTS);
        let mut event_tick = time::interval(Duration::from_millis(EVENT_TICK_MS));
        let mut poll_tick = time::interval(Duration::from_millis(POLL_TICK_MS));
        let mut connections = tokio::task::JoinSet::new();

        loop {
            tokio::select! {
                completion = self.producer.next() => {
                    if let Err(error) = self.apply_completion(completion) {
                        eprintln!("barrs: applying snapshots failed: {error}");
                    }
                }
                Some(_) = connections.join_next(), if !connections.is_empty() => {}
                dispatched = self.state.lua.next() => {
                    if let Err(error) = self.apply_dispatched(dispatched) {
                        eprintln!("barrs: applying lua result failed: {error}");
                    }
                }
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _)) => {
                            let sender = request_sender.clone();
                            connections.spawn(serve_connection(stream, sender));
                        }
                        Err(error) => eprintln!("barrs: accept failed: {error}"),
                    }
                }
                incoming = request_receiver.recv() => {
                    let Some(incoming) = incoming else {
                        continue;
                    };
                    let stop = matches!(incoming.request, Request::Stop);
                    self.handle_request(incoming.request, incoming.response);
                    if stop {
                        break;
                    }
                }
                _ = event_tick.tick() => {
                    if let Err(error) = self.process_renderer_events() {
                        eprintln!("barrs: renderer event processing failed: {error}");
                    }
                    if let Err(error) = self.process_rift_events(Instant::now()) {
                        eprintln!("barrs: rift event processing failed: {error}");
                    }
                }
                _ = poll_tick.tick() => {
                    self.refresh_due_items();
                }
            }
        }

        // The runtime can end when `run` returns, so let connection tasks send
        // their replies first. Closing the queue answers requests not yet handled.
        drop(request_receiver);
        let _ = time::timeout(Duration::from_millis(CONNECTION_DRAIN_MS), async {
            while connections.join_next().await.is_some() {}
        })
        .await;
        cleanup_socket(&socket_path)?;
        Ok(())
    }

    /// Answers one IPC request through `reply`.
    ///
    /// A reload answers when its snapshots are applied; an item trigger
    /// answers when its handler ends, or when the item's new snapshot is
    /// shown.
    fn handle_request(&mut self, request: Request, reply: oneshot::Sender<Response>) {
        let response = match request {
            Request::Ping => Ok(Response::Pong),
            Request::Stop => Ok(Response::Ok {
                message: "stopping".into(),
            }),
            Request::Reload => match LuaExecutor::start(&self.config_path) {
                Ok((lua, config)) => {
                    self.stage_reload(lua, config, reply);
                    return;
                }
                Err(error) => Err(error),
            },
            Request::Status => {
                let state = &self.state;
                Ok(Response::Status {
                    running: true,
                    items: state.config.items.len(),
                    backend: state.backend,
                    config_path: self.config_path.clone(),
                })
            }
            Request::DumpState => Ok(Response::State(json!(self.state.item_states))),
            Request::RiftBackend => Ok(Response::RiftBackend {
                backend: self.state.backend,
            }),
            Request::TriggerItem { payload } => {
                self.deliver_event(&payload, Some(reply));
                return;
            }
        };
        send_reply(reply, response);
    }

    /// Starts producing the snapshots of a loaded config. The config replaces
    /// the current one when they arrive; a newer reload supersedes this one.
    fn stage_reload(&mut self, lua: LuaExecutor, config: Config, reply: oneshot::Sender<Response>) {
        self.reload_ticket = self.reload_ticket.wrapping_add(1);
        let ticket = self.reload_ticket;
        self.producer
            .request(&config, &all_item_ids(&config), Purpose::Reload { ticket });
        let staged = StagedReload {
            ticket,
            config,
            lua,
            backend: self.producer.backend_kind(),
            reply,
        };
        if let Some(superseded) = self.staged_reload.replace(staged) {
            send_reply(
                superseded.reply,
                Err(BarrsError::DaemonRejected(
                    "reload superseded by a newer reload".into(),
                )),
            );
        }
    }

    /// Replaces the current config with a staged one whose snapshots are ready.
    ///
    /// A snapshot or reconcile error keeps the current config and state.
    fn commit_reload(
        &mut self,
        config: Config,
        lua: LuaExecutor,
        backend: RiftBackendKind,
        production: (Vec<ProducedItem>, RiftUse),
    ) -> Result<(), BarrsError> {
        let (items, rift) = production;
        let next_states = items
            .into_iter()
            .map(|(_, snapshot)| snapshot)
            .collect::<Result<Vec<_>, _>>()?;
        let state = &mut self.state;
        state.renderer.reconcile(&config, &next_states)?;
        state.backend = backend;
        state.config = config;
        state.lua.replace(lua);
        let now = Instant::now();
        state.refresh_deadlines = build_refresh_deadlines(&state.config, now, state.backend);
        state.rift_subscription = self.producer.subscribe(&state.config);
        state.item_states = index_snapshots(next_states);
        self.producer.record(rift, now);
        Ok(())
    }

    /// Produces and renders every item before the daemon reports readiness.
    async fn refresh_all_items(&mut self) -> Result<(), BarrsError> {
        let item_ids = all_item_ids(&self.state.config);
        self.producer
            .request(&self.state.config, &item_ids, Purpose::Startup);
        loop {
            let completion = self.producer.next().await;
            let startup = matches!(
                &completion,
                Completion::Items(production) if matches!(production.tag, Purpose::Startup)
            );
            let result = self.apply_completion(completion);
            if startup {
                return result;
            }
            if let Err(error) = result {
                eprintln!("barrs: applying snapshots failed: {error}");
            }
        }
    }

    fn refresh_due_items(&mut self) {
        let state = &mut self.state;
        let (lua_items, due_items): (Vec<String>, Vec<String>) =
            due_item_ids(&state.refresh_deadlines, Instant::now())
                .into_iter()
                .partition(|item_id| {
                    item_by_id(&state.config, item_id)
                        .is_some_and(|item| item.source.lua_update().is_some())
                });
        // A due item that is still in flight keeps its deadline, so it updates
        // once more after the running call ends and its interval passes.
        state.lua.request_updates(&state.config, &lua_items);

        if !can_start_refresh(self.refresh_in_flight, &due_items) {
            return;
        }
        self.refresh_in_flight = true;
        let purpose = Purpose::Scheduled {
            epoch: state.lua.epoch(),
            item_ids: due_items.clone(),
        };
        self.producer.request(&state.config, &due_items, purpose);
    }

    /// Applies a finished Lua call: answers the event that a handler
    /// processed, or renders the content that an update function produced.
    fn apply_dispatched(&mut self, dispatched: Dispatched<EventReply>) -> Result<(), BarrsError> {
        match dispatched {
            Dispatched::Handled {
                tag: reply,
                item_id,
                result,
                refresh,
            } => {
                if refresh {
                    let purpose = Purpose::Event {
                        epoch: self.state.lua.epoch(),
                        reply,
                    };
                    self.producer
                        .request(&self.state.config, &[item_id], purpose);
                } else {
                    answer_event(reply, result);
                }
                Ok(())
            }
            Dispatched::Updated { item_id, content } => self.show_update(item_id, content),
        }
    }

    /// Renders the content of one Lua update function call.
    fn show_update(&mut self, item_id: String, content: ItemContent) -> Result<(), BarrsError> {
        let state = &mut self.state;
        let Some((order, item)) = state
            .config
            .items
            .iter()
            .enumerate()
            .find(|(_, item)| item.id == item_id)
        else {
            return Ok(());
        };
        let snapshot = RenderItemSnapshot::from_item_config(item, order, content);
        advance_refresh_deadlines(
            &mut state.refresh_deadlines,
            &state.config,
            state.backend,
            std::slice::from_ref(&item_id),
            Instant::now(),
        );
        state.renderer.render_item(&snapshot)?;
        state.item_states.insert(item_id, snapshot);
        Ok(())
    }

    /// Applies a finished production for the purpose that requested it.
    ///
    /// Results for an older config generation or a superseded reload are
    /// discarded.
    fn apply_completion(&mut self, completion: Completion<Purpose>) -> Result<(), BarrsError> {
        let Completion::Items(production) = completion else {
            return Ok(());
        };
        let (purpose, items, rift) = production.into_parts();
        match purpose {
            Purpose::Startup => {
                let snapshots = items
                    .into_iter()
                    .map(|(_, snapshot)| snapshot)
                    .collect::<Result<Vec<_>, _>>()?;
                let state = &mut self.state;
                state.renderer.reconcile(&state.config, &snapshots)?;
                state.item_states = index_snapshots(snapshots);
                self.producer.record(rift, Instant::now());
                Ok(())
            }
            Purpose::Scheduled { epoch, item_ids } => {
                self.refresh_in_flight = false;
                if !refresh_epoch_matches(self.state.lua.epoch(), epoch) {
                    return Ok(());
                }
                let state = &mut self.state;
                for (item_id, snapshot) in items {
                    match snapshot {
                        Ok(snapshot) => {
                            state.renderer.render_item(&snapshot)?;
                            state.item_states.insert(item_id, snapshot);
                        }
                        Err(error) => eprintln!("barrs: snapshot for {item_id} failed: {error}"),
                    }
                }
                let now = Instant::now();
                advance_refresh_deadlines(
                    &mut state.refresh_deadlines,
                    &state.config,
                    state.backend,
                    &item_ids,
                    now,
                );
                self.producer.record(rift, now);
                Ok(())
            }
            Purpose::Event { epoch, reply } => {
                let result = if refresh_epoch_matches(self.state.lua.epoch(), epoch) {
                    self.show_event_items(items, rift)
                } else {
                    Ok(())
                };
                match reply {
                    Some(reply) => {
                        send_reply(reply, result.map(|()| event_delivered()));
                        Ok(())
                    }
                    None => result,
                }
            }
            Purpose::Reload { ticket } => {
                let Some(staged) = self.staged_reload.take_if(|staged| staged.ticket == ticket)
                else {
                    return Ok(());
                };
                let result =
                    self.commit_reload(staged.config, staged.lua, staged.backend, (items, rift));
                send_reply(
                    staged.reply,
                    result.map(|()| Response::Ok {
                        message: "reloaded".into(),
                    }),
                );
                Ok(())
            }
        }
    }

    fn show_event_items(
        &mut self,
        items: Vec<ProducedItem>,
        rift: RiftUse,
    ) -> Result<(), BarrsError> {
        let state = &mut self.state;
        for (item_id, snapshot) in items {
            let snapshot = snapshot?;
            state.renderer.render_item(&snapshot)?;
            state.item_states.insert(item_id, snapshot);
        }
        self.producer.record(rift, Instant::now());
        Ok(())
    }

    fn process_renderer_events(&mut self) -> Result<(), BarrsError> {
        for event in self.state.renderer.drain_events()? {
            self.deliver_event(&event, None);
        }
        Ok(())
    }

    fn process_rift_events(&mut self, now: Instant) -> Result<(), BarrsError> {
        let events = self
            .state
            .rift_subscription
            .as_ref()
            .map(RiftSubscription::drain)
            .unwrap_or_default();
        self.producer.on_rift_events(&events, now);
        let state = &mut self.state;
        for snapshot in self.producer.due_rift_renders(&state.config, now)? {
            state.renderer.render_item(&snapshot)?;
            state.item_states.insert(snapshot.id.clone(), snapshot);
        }
        Ok(())
    }

    /// Delivers an item event to the renderer and queues its Lua handler.
    ///
    /// Nothing here waits for Lua. The handler result, and the item refresh
    /// that a click or scroll asks for, come back through
    /// [`Self::apply_dispatched`], which answers `reply`.
    fn deliver_event(&mut self, payload: &EventPayload, reply: EventReply) {
        let state = &mut self.state;
        let Some(item) = item_by_id(&state.config, &payload.item_id) else {
            let error = BarrsError::InvalidConfig(format!("unknown item {}", payload.item_id));
            answer_event(reply, Err(error));
            return;
        };
        if let Err(error) = state.renderer.handle_event(payload) {
            answer_event(reply, Err(error));
            return;
        }
        state.lua.deliver(item, payload, reply);
    }
}

/// Answers the request that delivered an event, or logs an event error when
/// no request waits.
fn answer_event(reply: EventReply, result: Result<(), BarrsError>) {
    match reply {
        Some(reply) => send_reply(reply, result.map(|()| event_delivered())),
        None => {
            if let Err(error) = result {
                eprintln!("barrs: event handling failed: {error}");
            }
        }
    }
}

fn send_reply(reply: oneshot::Sender<Response>, result: Result<Response, BarrsError>) {
    let response = result.unwrap_or_else(|error| Response::Error {
        message: error.to_string(),
    });
    // The client may have gone; nothing waits for this reply then.
    let _ = reply.send(response);
}

fn event_delivered() -> Response {
    Response::Ok {
        message: "event delivered".into(),
    }
}

fn all_item_ids(config: &Config) -> Vec<String> {
    config.items.iter().map(|item| item.id.clone()).collect()
}

fn index_snapshots(snapshots: Vec<RenderItemSnapshot>) -> HashMap<String, RenderItemSnapshot> {
    snapshots
        .into_iter()
        .map(|snapshot| (snapshot.id.clone(), snapshot))
        .collect()
}

fn cleanup_socket(path: &Path) -> Result<(), BarrsError> {
    if !path.exists() {
        return Ok(());
    }

    let file_type = fs::symlink_metadata(path)?.file_type();
    if file_type.is_socket() {
        fs::remove_file(path)?;
        return Ok(());
    }

    Err(BarrsError::InvalidConfig(format!(
        "refusing to remove non-socket path {}",
        path.display()
    )))
}

async fn bind_daemon_socket(path: &Path) -> Result<UnixListener, BarrsError> {
    if path.exists() {
        reclaim_stale_socket(path).await?;
    }
    match UnixListener::bind(path) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == ErrorKind::AddrInUse => {
            reclaim_stale_socket(path).await?;
            UnixListener::bind(path).map_err(BarrsError::from)
        }
        Err(error) => Err(error.into()),
    }
}

async fn reclaim_stale_socket(path: &Path) -> Result<(), BarrsError> {
    let file_type = fs::symlink_metadata(path)?.file_type();
    if !file_type.is_socket() {
        return Err(BarrsError::InvalidConfig(format!(
            "refusing to remove non-socket path {}",
            path.display()
        )));
    }
    match time::timeout(
        Duration::from_millis(250),
        send_request(path, &Request::Ping),
    )
    .await
    {
        Ok(Ok(_)) => return Err(BarrsError::DaemonAlreadyRunning),
        Ok(Err(BarrsError::DaemonUnavailable)) | Err(_) => {}
        Ok(Err(error)) => return Err(error),
    }
    fs::remove_file(path)?;
    Ok(())
}

/// Line the daemon prints on stdout once its socket accepts requests.
pub const STARTUP_READY_LINE: &str = "barrs-ready";

fn signal_startup_ready() {
    if env::var_os("BARRS_STARTUP_READY").is_some() {
        let _ = writeln!(std::io::stdout(), "{STARTUP_READY_LINE}");
        let _ = std::io::stdout().flush();
    }
}

fn build_refresh_deadlines(
    config: &Config,
    now: Instant,
    backend: RiftBackendKind,
) -> HashMap<String, Instant> {
    config
        .items
        .iter()
        .filter_map(|item| {
            if item.source.lua_update().is_some() {
                // Lua items show a placeholder until their first update, so run it at once.
                return Some((item.id.clone(), now));
            }
            item_refresh_interval(item, backend)
                .and_then(|refresh_interval| now.checked_add(refresh_interval))
                .map(|deadline| (item.id.clone(), deadline))
        })
        .collect()
}

fn due_item_ids(deadlines: &HashMap<String, Instant>, now: Instant) -> Vec<String> {
    deadlines
        .iter()
        .filter(|(_, deadline)| **deadline <= now)
        .map(|(item_id, _)| item_id.clone())
        .collect()
}

const fn can_start_refresh(refresh_is_pending: bool, due_items: &[String]) -> bool {
    !refresh_is_pending && !due_items.is_empty()
}

const fn refresh_epoch_matches(current_epoch: u64, pending_epoch: u64) -> bool {
    current_epoch == pending_epoch
}

fn advance_refresh_deadlines(
    deadlines: &mut HashMap<String, Instant>,
    config: &Config,
    backend: RiftBackendKind,
    item_ids: &[String],
    now: Instant,
) {
    for item_id in item_ids {
        if let Some(item) = config.items.iter().find(|item| item.id == *item_id)
            && let Some(refresh_interval) = item_refresh_interval(item, backend)
            && let Some(deadline) = now.checked_add(refresh_interval)
        {
            deadlines.insert(item_id.clone(), deadline);
        }
    }
}

fn item_refresh_interval(item: &ItemConfig, backend: RiftBackendKind) -> Option<Duration> {
    item.interval
        .map(|interval| Duration::from_secs(interval.max(1)))
        .or_else(|| item.source.default_interval(backend))
}

fn item_by_id<'a>(config: &'a Config, item_id: &str) -> Option<&'a ItemConfig> {
    config.items.iter().find(|item| item.id == item_id)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::sync::{
        Arc,
        atomic::AtomicBool,
        atomic::{AtomicUsize, Ordering},
    };

    use tempfile::tempdir;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;
    use tokio::task::JoinHandle;

    use crate::config::{Config, ItemConfig, ItemHandlers, load_config};
    use crate::ipc::{Request, Response, default_socket_path, send_request};
    use crate::render::{NativeRenderer, NoopRenderer, Renderer};
    use crate::rift::FakeRiftBackend;
    use crate::source::{ItemSource, SystemPlugin};

    use super::Daemon;

    struct CountingRenderer {
        renders: Arc<AtomicUsize>,
    }

    struct FailingReconcileRenderer {
        fail_initialization: Arc<AtomicBool>,
        initializations: Arc<AtomicUsize>,
    }

    impl Renderer for FailingReconcileRenderer {
        fn initialize(
            &mut self,
            _config: &crate::config::Config,
        ) -> Result<(), crate::error::BarrsError> {
            if self.fail_initialization.load(Ordering::SeqCst) {
                return Err(crate::error::BarrsError::Unsupported(
                    "renderer initialization failed".into(),
                ));
            }
            self.initializations.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn render_item(
            &mut self,
            _snapshot: &crate::render::RenderItemSnapshot,
        ) -> Result<(), crate::error::BarrsError> {
            Ok(())
        }
    }

    impl Renderer for CountingRenderer {
        fn initialize(
            &mut self,
            _config: &crate::config::Config,
        ) -> Result<(), crate::error::BarrsError> {
            Ok(())
        }

        fn render_item(
            &mut self,
            _snapshot: &crate::render::RenderItemSnapshot,
        ) -> Result<(), crate::error::BarrsError> {
            self.renders.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// Delivers an event as the renderer would, with no request to answer,
    /// and applies finished Lua calls until its handler ends.
    async fn dispatch<R: Renderer>(daemon: &mut Daemon<R>, payload: crate::ipc::EventPayload) {
        daemon.deliver_event(&payload, None);
        loop {
            let dispatched = daemon.state.lua.next().await;
            let handled = matches!(dispatched, super::Dispatched::Handled { .. });
            daemon
                .apply_dispatched(dispatched)
                .expect("apply lua result");
            if handled {
                return;
            }
        }
    }

    /// Waits for the next finished snapshot production and applies it.
    async fn apply_next<R: Renderer>(daemon: &mut Daemon<R>) {
        let completion = daemon.producer.next().await;
        daemon
            .apply_completion(completion)
            .expect("apply snapshots");
    }

    /// Hands `request` to the daemon and applies finished productions and
    /// Lua calls until the daemon answers it.
    async fn answer<R: Renderer>(daemon: &mut Daemon<R>, request: Request) -> Response {
        let (reply, mut response) = tokio::sync::oneshot::channel();
        daemon.handle_request(request, reply);
        loop {
            match response.try_recv() {
                Ok(response) => return response,
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    tokio::select! {
                        completion = daemon.producer.next() => {
                            daemon.apply_completion(completion).expect("apply snapshots");
                        }
                        dispatched = daemon.state.lua.next() => {
                            daemon.apply_dispatched(dispatched).expect("apply lua result");
                        }
                    }
                }
                Err(error) => panic!("daemon dropped the reply: {error}"),
            }
        }
    }

    async fn reload<R: Renderer>(daemon: &mut Daemon<R>) -> Result<(), String> {
        match answer(daemon, Request::Reload).await {
            Response::Ok { .. } => Ok(()),
            Response::Error { message } => Err(message),
            other => panic!("unexpected reload response {other:?}"),
        }
    }

    fn write_config(path: &Path, socket_path: &Path) {
        fs::write(
            path,
            format!(
                r#"
function handle_click(ctx)
  return true
end

return {{
  socket_path = "{}",
  items = {{
    {{
      id = "clock",
      label = "clock",
      plugin = {{ kind = "time" }},
      handlers = {{ click = "handle_click" }}
    }}
  }}
}}
"#,
                socket_path.display()
            ),
        )
        .expect("write config");
    }

    fn write_refreshing_config(path: &Path, socket_path: &Path) {
        fs::write(
            path,
            format!(
                r#"
return {{
  socket_path = "{}",
  items = {{
    {{
      id = "clock",
      plugin = {{ kind = "time" }},
      interval = 1
    }}
  }}
}}
"#,
                socket_path.display()
            ),
        )
        .expect("write config");
    }

    fn write_hover_config(path: &Path, socket_path: &Path) {
        fs::write(
            path,
            format!(
                r#"
return {{
  socket_path = "{}",
  items = {{
    {{
      id = "clock",
      label = "clock",
      hover = {{ tooltip = "Current time" }}
    }}
  }}
}}
"#,
                socket_path.display()
            ),
        )
        .expect("write config");
    }

    fn write_hover_update_config(path: &Path, socket_path: &Path, marker_path: &Path) {
        fs::write(
            path,
            format!(
                r#"
updates = 0
function record_hover_update(ctx)
  updates = updates + 1
  local marker = io.open([=[{}]=], "w")
  marker:write(tostring(updates))
  marker:close()
end

return {{
  socket_path = "{}",
  items = {{
    {{ id = "clock", label = "clock", hover = {{ tooltip = "Clock" }}, handlers = {{ hover_update = "record_hover_update" }} }}
  }}
}}
"#,
                marker_path.display(),
                socket_path.display()
            ),
        )
        .expect("write config");
    }

    fn write_item_config(path: &Path, socket_path: &Path, item_ids: &[&str]) {
        let items = item_ids
            .iter()
            .map(|id| format!(r#"{{ id = "{id}", label = "{id}" }}"#))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            path,
            format!(
                r#"
return {{
  socket_path = "{}",
  items = {{ {items} }}
}}
"#,
                socket_path.display()
            ),
        )
        .expect("write config");
    }

    fn write_rift_config(path: &Path, socket_path: &Path) {
        fs::write(
            path,
            format!(
                r#"
return {{
  socket_path = "{}",
  items = {{
    {{ id = "workspaces", plugin = {{ kind = "rift_workspaces" }} }},
    {{ id = "layout", plugin = {{ kind = "rift_layout" }} }},
  }}
}}
"#,
                socket_path.display()
            ),
        )
        .expect("write config");
    }

    fn rift_snapshot(current_workspace: &str) -> crate::rift::RiftSnapshot {
        crate::rift::RiftSnapshot {
            current_workspace: current_workspace.into(),
            workspaces: ["1", "2", "3"]
                .into_iter()
                .map(|name| crate::rift::RiftWorkspace {
                    name: name.into(),
                    is_current: name == current_workspace,
                    window_count: 1,
                    has_windows: true,
                })
                .collect(),
            layout: "tiling".into(),
            window_count: 1,
        }
    }

    /// A daemon for the Rift config whose snapshots read `backend`.
    fn rift_daemon(
        config_path: &Path,
        renderer: CountingRenderer,
        backend: &Arc<FakeRiftBackend>,
    ) -> Daemon<CountingRenderer> {
        let (lua, config) = crate::lua_executor::LuaExecutor::start(config_path).expect("load");
        Daemon::new_loaded(
            config_path.to_path_buf(),
            config,
            lua,
            renderer,
            Arc::<FakeRiftBackend>::clone(backend),
        )
        .expect("daemon")
    }

    fn shown_workspace<R: Renderer>(daemon: &Daemon<R>) -> String {
        match &daemon.state.item_states["workspaces"].content {
            crate::content::ItemContent::Workspaces { current, .. } => current.clone(),
            other => panic!("unexpected workspaces content {other:?}"),
        }
    }

    fn write_trigger_config(path: &Path, socket_path: &Path, marker_path: &Path) {
        fs::write(
            path,
            format!(
                r#"
function handle_click(ctx)
  local file = io.open([=[{}]=], "w")
  file:write(ctx.item_id .. ":" .. ctx.event)
  file:close()
end

return {{
  socket_path = "{}",
  items = {{
    {{
      id = "clock",
      label = "clock",
      handlers = {{ click = "handle_click" }}
    }}
  }}
}}
"#,
                marker_path.display(),
                socket_path.display()
            ),
        )
        .expect("write config");
    }

    fn write_counter_config(path: &Path, socket_path: &Path, marker_path: &Path) {
        fs::write(
            path,
            format!(
                r#"
counter = 0

function handle_click(ctx)
  counter = counter + 1
  local file = io.open([=[{}]=], "w")
  file:write(tostring(counter))
  file:close()
end

return {{
  socket_path = "{}",
  items = {{
    {{
      id = "clock",
      label = "clock",
      handlers = {{ click = "handle_click" }}
    }}
  }}
}}
"#,
                marker_path.display(),
                socket_path.display()
            ),
        )
        .expect("write config");
    }

    fn write_evaluation_config(
        path: &Path,
        socket_path: &Path,
        evaluation_path: &Path,
        handler_path: &Path,
    ) {
        fs::write(
            path,
            format!(
                r#"
local evaluation_file = io.open([=[{}]=], "r")
local previous = evaluation_file and tonumber(evaluation_file:read("*a")) or 0
if evaluation_file then evaluation_file:close() end
local generation = previous + 1
local write_evaluation = io.open([=[{}]=], "w")
write_evaluation:write(tostring(generation))
write_evaluation:close()

function record_generation(ctx)
  local handler_file = io.open([=[{}]=], "w")
  handler_file:write(tostring(generation))
  handler_file:close()
end

return {{
  socket_path = "{}",
  items = {{
    {{ id = "generation", label = tostring(generation), handlers = {{ click = "record_generation" }} }}
  }}
}}
"#,
                evaluation_path.display(),
                evaluation_path.display(),
                handler_path.display(),
                socket_path.display(),
            ),
        )
        .expect("write config");
    }

    async fn wait_for_socket(socket_path: &Path) {
        for _ in 0..20 {
            if socket_path.exists() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }

    async fn item_state(socket_path: &Path, item_id: &str) -> serde_json::Value {
        match send_request(socket_path, &Request::DumpState)
            .await
            .expect("dump state")
        {
            Response::State(states) => states[item_id].clone(),
            other => panic!("unexpected response {other:?}"),
        }
    }

    async fn wait_for_item_text(
        socket_path: &Path,
        item_id: &str,
        text: &str,
    ) -> serde_json::Value {
        for _ in 0..160 {
            let state = item_state(socket_path, item_id).await;
            if state["text"] == text {
                return state;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!(
            "item {item_id} never showed {text}: {}",
            item_state(socket_path, item_id).await
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn lua_update_functions_set_item_text() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        fs::write(
            &config_path,
            format!(
                r#"
calls = 0
function usage(ctx)
  calls = calls + 1
  return {{ text = ctx.item_id .. ":" .. calls, used = 42 }}
end
function broken(ctx) error("boom") end
function noop(ctx) end
return {{
  socket_path = "{}",
  items = {{
    {{ id = "memory", interval = 3600, plugin = {{ kind = "lua", update = "usage" }},
       handlers = {{ click = "noop" }} }},
    {{ id = "broken", interval = 3600, plugin = {{ kind = "lua", update = "broken" }} }},
  }}
}}
"#,
                socket_path.display()
            ),
        )
        .expect("write config");

        let config = load_config(&config_path).expect("config");
        let daemon = Daemon::new(config_path.clone(), config, NoopRenderer).expect("daemon");
        let task: JoinHandle<Result<(), crate::error::BarrsError>> = tokio::spawn(daemon.run());
        wait_for_socket(&socket_path).await;

        let memory = wait_for_item_text(&socket_path, "memory", "memory:1").await;
        assert_eq!(memory["data"]["used"], 42);
        let broken = wait_for_item_text(&socket_path, "broken", "—").await;
        assert_eq!(broken["data"]["available"], false);

        send_request(
            &socket_path,
            &Request::TriggerItem {
                payload: crate::ipc::EventPayload::from_trigger(
                    "memory".into(),
                    crate::cli::TriggerEvent::Click,
                ),
            },
        )
        .await
        .expect("trigger");
        wait_for_item_text(&socket_path, "memory", "memory:2").await;

        let _ = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop");
        task.await.expect("join").expect("daemon result");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_trigger_behind_a_slow_lua_update_does_not_block_ipc() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        let started_path = dir.path().join("started");
        fs::write(
            &config_path,
            format!(
                r#"
function slow(ctx)
  local marker = io.open("{}", "w")
  marker:write("1")
  marker:close()
  local start = os.clock()
  while os.clock() - start < 1.5 do end
  return "done"
end
function noop(ctx) end
return {{
  socket_path = "{}",
  items = {{
    {{ id = "slow", interval = 3600, plugin = {{ kind = "lua", update = "slow" }} }},
    {{ id = "button", label = "button", handlers = {{ click = "noop" }} }},
  }}
}}
"#,
                started_path.display(),
                socket_path.display()
            ),
        )
        .expect("write config");

        let config = load_config(&config_path).expect("config");
        let daemon = Daemon::new(config_path.clone(), config, NoopRenderer).expect("daemon");
        let task: JoinHandle<Result<(), crate::error::BarrsError>> = tokio::spawn(daemon.run());
        wait_for_socket(&socket_path).await;
        for _ in 0..80 {
            if started_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        assert!(started_path.exists(), "update function never started");

        // The click handler waits on the Lua thread behind the slow update.
        let trigger_socket = socket_path.clone();
        let trigger = tokio::spawn(async move {
            send_request(
                &trigger_socket,
                &Request::TriggerItem {
                    payload: crate::ipc::EventPayload::from_trigger(
                        "button".into(),
                        crate::cli::TriggerEvent::Click,
                    ),
                },
            )
            .await
        });
        // Let the daemon receive the trigger before the other requests.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let started = std::time::Instant::now();
        let response = send_request(&socket_path, &Request::Ping)
            .await
            .expect("ping");
        assert!(matches!(response, Response::Pong));
        let status = send_request(&socket_path, &Request::Status)
            .await
            .expect("status");
        assert!(matches!(status, Response::Status { items: 2, .. }));
        assert!(
            started.elapsed() < std::time::Duration::from_millis(500),
            "ping and status took {:?} while an update ran",
            started.elapsed()
        );
        assert!(!trigger.is_finished(), "the trigger waits for its handler");

        let response = trigger.await.expect("join").expect("trigger");
        assert!(matches!(response, Response::Ok { .. }));
        wait_for_item_text(&socket_path, "slow", "done").await;

        let _ = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop");
        task.await.expect("join").expect("daemon result");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_accepts_ping_and_stop() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_config(&config_path, &socket_path);

        let config = load_config(&config_path).expect("config");
        let daemon = Daemon::new(config_path.clone(), config, NoopRenderer).expect("daemon");
        let task: JoinHandle<Result<(), crate::error::BarrsError>> = tokio::spawn(daemon.run());

        for _ in 0..20 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        let pong = send_request(&socket_path, &Request::Ping)
            .await
            .expect("ping");
        assert!(matches!(pong, Response::Pong));

        let stop = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop");
        assert!(matches!(stop, Response::Ok { .. }));
        task.await.expect("join").expect("daemon result");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_delivers_trigger_item_requests_to_lua_handlers() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        let marker_path = dir.path().join("trigger.txt");
        write_trigger_config(&config_path, &socket_path, &marker_path);

        let config = load_config(&config_path).expect("config");
        let daemon = Daemon::new(config_path.clone(), config, NoopRenderer).expect("daemon");
        let task: JoinHandle<Result<(), crate::error::BarrsError>> = tokio::spawn(daemon.run());

        wait_for_socket(&socket_path).await;

        let response = send_request(
            &socket_path,
            &Request::TriggerItem {
                payload: crate::ipc::EventPayload::from_trigger(
                    "clock".into(),
                    crate::cli::TriggerEvent::Click,
                ),
            },
        )
        .await
        .expect("trigger");
        assert!(matches!(response, Response::Ok { .. }));
        assert_eq!(
            fs::read_to_string(&marker_path).expect("marker"),
            "clock:click"
        );

        let _ = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop");
        task.await.expect("join").expect("daemon result");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stop_reply_arrives_before_the_daemon_runtime_shuts_down() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_config(&config_path, &socket_path);

        // Like the binary, the daemon runtime ends as soon as `run` returns.
        let daemon_thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            let config = load_config(&config_path).expect("config");
            let daemon = Daemon::new(config_path, config, NoopRenderer).expect("daemon");
            runtime.block_on(daemon.run())
        });
        wait_for_socket(&socket_path).await;

        let response = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop reply");
        assert!(matches!(response, Response::Ok { .. }));
        daemon_thread.join().expect("join").expect("daemon result");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_survives_malformed_requests() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_config(&config_path, &socket_path);

        let config = load_config(&config_path).expect("config");
        let daemon = Daemon::new(config_path.clone(), config, NoopRenderer).expect("daemon");
        let task: JoinHandle<Result<(), crate::error::BarrsError>> = tokio::spawn(daemon.run());

        wait_for_socket(&socket_path).await;

        let mut stream = UnixStream::connect(&socket_path).await.expect("connect");
        stream
            .write_all(b"not json\n")
            .await
            .expect("write request");
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("read response");
        assert!(matches!(
            serde_json::from_str::<Response>(line.trim()).expect("response"),
            Response::Error { .. }
        ));

        let pong = send_request(&socket_path, &Request::Ping)
            .await
            .expect("ping");
        assert!(matches!(pong, Response::Pong));

        let _ = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop");
        task.await.expect("join").expect("daemon result");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_survives_unknown_item_trigger_over_ipc() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        let marker_path = dir.path().join("trigger.txt");
        write_trigger_config(&config_path, &socket_path, &marker_path);

        let config = load_config(&config_path).expect("config");
        let daemon = Daemon::new(config_path.clone(), config, NoopRenderer).expect("daemon");
        let task: JoinHandle<Result<(), crate::error::BarrsError>> = tokio::spawn(daemon.run());

        wait_for_socket(&socket_path).await;

        let response = send_request(
            &socket_path,
            &Request::TriggerItem {
                payload: crate::ipc::EventPayload::from_trigger(
                    "missing".into(),
                    crate::cli::TriggerEvent::Click,
                ),
            },
        )
        .await
        .expect("trigger");
        let Response::Error { message } = response else {
            panic!("expected error response");
        };
        assert!(message.contains("unknown item missing"));

        let pong = send_request(&socket_path, &Request::Ping)
            .await
            .expect("ping");
        assert!(matches!(pong, Response::Pong));

        let _ = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop");
        task.await.expect("join").expect("daemon result");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_survives_reload_with_broken_config() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_config(&config_path, &socket_path);

        let config = load_config(&config_path).expect("config");
        let daemon = Daemon::new(config_path.clone(), config, NoopRenderer).expect("daemon");
        let task: JoinHandle<Result<(), crate::error::BarrsError>> = tokio::spawn(daemon.run());

        wait_for_socket(&socket_path).await;
        fs::write(&config_path, "this is not lua").expect("write broken config");

        let response = send_request(&socket_path, &Request::Reload)
            .await
            .expect("reload");
        assert!(matches!(response, Response::Error { .. }));

        let pong = send_request(&socket_path, &Request::Ping)
            .await
            .expect("ping");
        assert!(matches!(pong, Response::Pong));

        let _ = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop");
        task.await.expect("join").expect("daemon result");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_survives_silent_client() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_refreshing_config(&config_path, &socket_path);

        let config = load_config(&config_path).expect("config");
        let renders = Arc::new(AtomicUsize::new(0));
        let renderer = CountingRenderer {
            renders: Arc::clone(&renders),
        };
        let mut daemon = Daemon::new(config_path.clone(), config, renderer).expect("daemon");
        daemon
            .state
            .refresh_deadlines
            .insert("clock".into(), tokio::time::Instant::now());
        let task: JoinHandle<Result<(), crate::error::BarrsError>> = tokio::spawn(daemon.run());

        wait_for_socket(&socket_path).await;
        let _silent = UnixStream::connect(&socket_path).await.expect("connect");

        let pong = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            send_request(&socket_path, &Request::Ping),
        )
        .await
        .expect("silent client must not delay ping")
        .expect("ping");
        assert!(matches!(pong, Response::Pong));
        tokio::time::timeout(std::time::Duration::from_millis(750), async {
            while renders.load(Ordering::SeqCst) < 2 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("scheduled refresh must complete while a client is silent");

        let mut abandoned = UnixStream::connect(&socket_path).await.expect("connect");
        abandoned
            .write_all(b"{\"type\":\"ping\"}\n")
            .await
            .expect("write request");
        drop(abandoned);

        let _ = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop");
        task.await.expect("join").expect("daemon result");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn lua_handler_state_persists_between_events() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        let marker_path = dir.path().join("counter.txt");
        write_counter_config(&config_path, &socket_path, &marker_path);

        let config = load_config(&config_path).expect("config");
        let daemon = Daemon::new(config_path.clone(), config, NoopRenderer).expect("daemon");
        let task: JoinHandle<Result<(), crate::error::BarrsError>> = tokio::spawn(daemon.run());

        wait_for_socket(&socket_path).await;
        for _ in 0..2 {
            let response = send_request(
                &socket_path,
                &Request::TriggerItem {
                    payload: crate::ipc::EventPayload::from_trigger(
                        "clock".into(),
                        crate::cli::TriggerEvent::Click,
                    ),
                },
            )
            .await
            .expect("trigger");
            assert!(matches!(response, Response::Ok { .. }));
        }
        assert_eq!(fs::read_to_string(&marker_path).expect("marker"), "2");

        let _ = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop");
        task.await.expect("join").expect("daemon result");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn lua_handlers_do_not_reread_config_from_disk() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        let marker_path = dir.path().join("counter.txt");
        write_counter_config(&config_path, &socket_path, &marker_path);

        let config = load_config(&config_path).expect("config");
        let daemon = Daemon::new(config_path.clone(), config, NoopRenderer).expect("daemon");
        let task: JoinHandle<Result<(), crate::error::BarrsError>> = tokio::spawn(daemon.run());

        wait_for_socket(&socket_path).await;
        fs::write(&config_path, "boom").expect("write broken config");
        let response = send_request(
            &socket_path,
            &Request::TriggerItem {
                payload: crate::ipc::EventPayload::from_trigger(
                    "clock".into(),
                    crate::cli::TriggerEvent::Click,
                ),
            },
        )
        .await
        .expect("trigger");
        assert!(matches!(response, Response::Ok { .. }));
        assert_eq!(fs::read_to_string(&marker_path).expect("marker"), "1");

        let _ = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop");
        task.await.expect("join").expect("daemon result");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reload_resets_lua_handler_state() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        let marker_path = dir.path().join("counter.txt");
        write_counter_config(&config_path, &socket_path, &marker_path);

        let config = load_config(&config_path).expect("config");
        let daemon = Daemon::new(config_path.clone(), config, NoopRenderer).expect("daemon");
        let task: JoinHandle<Result<(), crate::error::BarrsError>> = tokio::spawn(daemon.run());

        wait_for_socket(&socket_path).await;
        for _ in 0..2 {
            let _ = send_request(
                &socket_path,
                &Request::TriggerItem {
                    payload: crate::ipc::EventPayload::from_trigger(
                        "clock".into(),
                        crate::cli::TriggerEvent::Click,
                    ),
                },
            )
            .await
            .expect("trigger");
        }
        assert_eq!(fs::read_to_string(&marker_path).expect("marker"), "2");

        let response = send_request(&socket_path, &Request::Reload)
            .await
            .expect("reload");
        assert!(matches!(response, Response::Ok { .. }));
        let _ = send_request(
            &socket_path,
            &Request::TriggerItem {
                payload: crate::ipc::EventPayload::from_trigger(
                    "clock".into(),
                    crate::cli::TriggerEvent::Click,
                ),
            },
        )
        .await
        .expect("trigger");
        assert_eq!(fs::read_to_string(&marker_path).expect("marker"), "1");

        let _ = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop");
        task.await.expect("join").expect("daemon result");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn loaded_config_and_lua_runtime_are_evaluated_once_together() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        let evaluation_path = dir.path().join("evaluations.txt");
        let handler_path = dir.path().join("handler.txt");
        write_evaluation_config(&config_path, &socket_path, &evaluation_path, &handler_path);

        let (lua, config) = crate::lua_executor::LuaExecutor::start(&config_path).expect("load");
        let mut daemon = Daemon::new_loaded(
            config_path.clone(),
            config,
            lua,
            NoopRenderer,
            Arc::new(FakeRiftBackend::default()),
        )
        .expect("daemon");
        assert_eq!(
            fs::read_to_string(&evaluation_path).expect("evaluation"),
            "1"
        );
        daemon.refresh_all_items().await.expect("initial render");
        assert_eq!(daemon.state.item_states["generation"].content.text(), "1");
        dispatch(
            &mut daemon,
            crate::ipc::EventPayload::from_trigger(
                "generation".into(),
                crate::cli::TriggerEvent::Click,
            ),
        )
        .await;
        assert_eq!(fs::read_to_string(&handler_path).expect("handler"), "1");

        reload(&mut daemon).await.expect("reload");
        assert_eq!(
            fs::read_to_string(&evaluation_path).expect("evaluation"),
            "2"
        );
        assert_eq!(daemon.state.item_states["generation"].content.text(), "2");
        dispatch(
            &mut daemon,
            crate::ipc::EventPayload::from_trigger(
                "generation".into(),
                crate::cli::TriggerEvent::Click,
            ),
        )
        .await;
        assert_eq!(fs::read_to_string(&handler_path).expect("handler"), "2");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_click_on_a_rift_consumer_refreshes_the_rift_tracker() {
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join("barrs.lua");
        write_rift_config(&config_path, &dir.path().join("barrs.sock"));
        let backend = Arc::new(FakeRiftBackend::default());
        backend.set(Some(rift_snapshot("1")));
        let renders = Arc::new(AtomicUsize::new(0));
        let renderer = CountingRenderer {
            renders: Arc::clone(&renders),
        };
        let mut daemon = rift_daemon(&config_path, renderer, &backend);
        daemon.refresh_all_items().await.expect("initial render");
        backend.set(Some(rift_snapshot("2")));

        let response = answer(
            &mut daemon,
            Request::TriggerItem {
                payload: crate::ipc::EventPayload::from_trigger(
                    "workspaces".into(),
                    crate::cli::TriggerEvent::Click,
                ),
            },
        )
        .await;

        assert!(matches!(response, Response::Ok { .. }));
        assert_eq!(shown_workspace(&daemon), "2");
        let cached = daemon.producer.rift_snapshot().expect("cached snapshot");
        assert_eq!(cached.current_workspace, "2");
        // The layout item still shows the older snapshot, so both consumers
        // render the cached one once after the debounce.
        let before_poll = renders.load(Ordering::SeqCst);
        let later = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
        daemon.process_rift_events(later).expect("debounced render");
        assert_eq!(renders.load(Ordering::SeqCst), before_poll + 2);
        daemon.process_rift_events(later).expect("no-op poll");
        assert_eq!(renders.load(Ordering::SeqCst), before_poll + 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reload_reads_rift_through_the_injected_backend() {
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join("barrs.lua");
        write_rift_config(&config_path, &dir.path().join("barrs.sock"));
        let backend = Arc::new(FakeRiftBackend::default());
        backend.set(Some(rift_snapshot("1")));
        let renderer = CountingRenderer {
            renders: Arc::new(AtomicUsize::new(0)),
        };
        let mut daemon = rift_daemon(&config_path, renderer, &backend);
        daemon.refresh_all_items().await.expect("initial render");
        assert_eq!(shown_workspace(&daemon), "1");
        backend.set(Some(rift_snapshot("3")));

        reload(&mut daemon).await.expect("reload");

        assert_eq!(shown_workspace(&daemon), "3");
        assert_eq!(backend.fetches(), 2);
        let later = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
        let renders = daemon.state.renderer.renders.load(Ordering::SeqCst);
        daemon.process_rift_events(later).expect("no-op poll");
        assert_eq!(
            daemon.state.renderer.renders.load(Ordering::SeqCst),
            renders
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn snapshots_produced_for_an_older_config_are_discarded() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_item_config(&config_path, &socket_path, &["old"]);
        let config = load_config(&config_path).expect("config");
        let mut daemon = Daemon::new(config_path.clone(), config, NoopRenderer).expect("daemon");
        daemon.refresh_all_items().await.expect("initial render");
        dispatch(
            &mut daemon,
            crate::ipc::EventPayload::from_trigger("old".into(), crate::cli::TriggerEvent::Click),
        )
        .await;
        write_item_config(&config_path, &socket_path, &["new"]);
        let (reply, mut response) = tokio::sync::oneshot::channel();
        daemon.handle_request(Request::Reload, reply);

        // Apply the reload before the click refresh that the old config requested.
        let mut completions = vec![daemon.producer.next().await, daemon.producer.next().await];
        completions.sort_by_key(|completion| {
            !matches!(
                completion,
                super::Completion::Items(production)
                    if matches!(production.tag, super::Purpose::Reload { .. })
            )
        });
        for completion in completions {
            daemon.apply_completion(completion).expect("apply");
        }

        assert!(matches!(response.try_recv(), Ok(Response::Ok { .. })));
        let mut item_ids = daemon.state.item_states.keys().cloned().collect::<Vec<_>>();
        item_ids.sort();
        assert_eq!(item_ids, vec!["new"]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reload_reconciles_removed_and_renamed_renderer_items() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_item_config(&config_path, &socket_path, &["old", "retained"]);

        let config = load_config(&config_path).expect("config");
        let mut daemon =
            Daemon::new(config_path.clone(), config, NativeRenderer::default()).expect("daemon");
        daemon.refresh_all_items().await.expect("initial render");
        write_item_config(&config_path, &socket_path, &["retained", "new"]);

        reload(&mut daemon).await.expect("reload");

        let state = &daemon.state;
        let mut item_ids = state.item_states.keys().cloned().collect::<Vec<_>>();
        item_ids.sort();
        assert_eq!(item_ids, vec!["new", "retained"]);
        let mut rendered_ids = state
            .renderer
            .surface_state()
            .items
            .iter()
            .map(|item| item.snapshot.id.clone())
            .collect::<Vec<_>>();

        rendered_ids.sort();
        assert_eq!(rendered_ids, vec!["new", "retained"]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failed_renderer_reload_retains_the_previous_daemon_state() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_item_config(&config_path, &socket_path, &["old"]);

        let fail_initialization = Arc::new(AtomicBool::new(false));
        let initializations = Arc::new(AtomicUsize::new(0));
        let renderer = FailingReconcileRenderer {
            fail_initialization: Arc::clone(&fail_initialization),
            initializations: Arc::clone(&initializations),
        };
        let config = load_config(&config_path).expect("config");
        let mut daemon = Daemon::new(config_path.clone(), config, renderer).expect("daemon");
        daemon.refresh_all_items().await.expect("initial render");
        assert_eq!(initializations.load(Ordering::SeqCst), 1);

        write_item_config(&config_path, &socket_path, &["retained"]);
        reload(&mut daemon).await.expect("successful reload");
        assert_eq!(initializations.load(Ordering::SeqCst), 2);

        write_item_config(&config_path, &socket_path, &["new"]);
        fail_initialization.store(true, Ordering::SeqCst);
        reload(&mut daemon).await.expect_err("reload must fail");

        let state = &daemon.state;
        assert_eq!(state.config.items[0].id, "retained");
        assert!(state.item_states.contains_key("retained"));
        assert!(!state.item_states.contains_key("old"));
        assert!(!state.item_states.contains_key("new"));

        assert_eq!(initializations.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_rejects_trigger_item_requests_for_unknown_items() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        let marker_path = dir.path().join("trigger.txt");
        write_trigger_config(&config_path, &socket_path, &marker_path);

        let config = load_config(&config_path).expect("config");
        let mut daemon = Daemon::new(config_path.clone(), config, NoopRenderer).expect("daemon");

        let response = answer(
            &mut daemon,
            Request::TriggerItem {
                payload: crate::ipc::EventPayload::from_trigger(
                    "missing".into(),
                    crate::cli::TriggerEvent::HoverEnter,
                ),
            },
        )
        .await;

        let Response::Error { message } = response else {
            panic!("unknown item should fail");
        };
        assert!(message.contains("unknown item missing"));
        assert!(!marker_path.exists());
    }

    #[test]
    fn default_socket_path_is_stable() {
        let socket_path = default_socket_path();

        assert_eq!(
            socket_path.file_name().and_then(|name| name.to_str()),
            Some("barrs.sock")
        );
        assert!(socket_path.starts_with(std::env::temp_dir()));
    }

    #[test]
    fn cleanup_socket_removes_socket_files() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).expect("bind socket");
        drop(listener);

        super::cleanup_socket(&socket_path).expect("cleanup socket");

        assert!(!socket_path.exists());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stale_socket_is_reclaimed_before_binding() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let stale = std::os::unix::net::UnixListener::bind(&socket_path).expect("bind socket");
        drop(stale);

        let listener = super::bind_daemon_socket(&socket_path)
            .await
            .expect("reclaim stale socket");

        assert!(socket_path.exists());
        drop(listener);
        super::cleanup_socket(&socket_path).expect("cleanup socket");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn live_daemon_is_not_replaced_or_initialized_twice() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_config(&config_path, &socket_path);

        let config = load_config(&config_path).expect("config");
        let first = Daemon::new(config_path.clone(), config, NoopRenderer).expect("first daemon");
        let first_task: JoinHandle<Result<(), crate::error::BarrsError>> =
            tokio::spawn(first.run());
        wait_for_socket(&socket_path).await;

        let second_renders = Arc::new(AtomicUsize::new(0));
        let second = Daemon::new(
            config_path.clone(),
            load_config(&config_path).expect("config"),
            CountingRenderer {
                renders: Arc::clone(&second_renders),
            },
        )
        .expect("second daemon");
        let error = second.run().await.expect_err("live daemon must win");
        assert!(matches!(
            error,
            crate::error::BarrsError::DaemonAlreadyRunning
        ));
        assert_eq!(second_renders.load(Ordering::SeqCst), 0);

        assert!(matches!(
            send_request(&socket_path, &Request::Ping)
                .await
                .expect("ping"),
            Response::Pong
        ));
        assert!(matches!(
            send_request(&socket_path, &Request::Stop)
                .await
                .expect("stop"),
            Response::Ok { .. }
        ));
        first_task.await.expect("join").expect("daemon result");
    }

    #[test]
    fn cleanup_socket_refuses_regular_files() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        fs::write(&socket_path, "not a socket").expect("write regular file");

        let error = super::cleanup_socket(&socket_path).expect_err("regular file should fail");

        assert!(
            error
                .to_string()
                .contains("refusing to remove non-socket path")
        );
        assert_eq!(
            fs::read_to_string(&socket_path).expect("regular file"),
            "not a socket"
        );
    }

    #[test]
    fn scheduler_transitions_are_deterministic() {
        let now = tokio::time::Instant::now();
        let mut deadlines = std::collections::HashMap::from([
            ("due".into(), now - std::time::Duration::from_millis(1)),
            ("later".into(), now + std::time::Duration::from_secs(1)),
        ]);
        let mut due = super::due_item_ids(&deadlines, now);
        due.sort();
        assert_eq!(due, vec!["due"]);
        assert!(super::can_start_refresh(false, &due));
        assert!(!super::can_start_refresh(true, &due));
        assert!(!super::can_start_refresh(false, &[]));

        assert!(super::refresh_epoch_matches(4, 4));
        assert!(!super::refresh_epoch_matches(4, 3));

        let config = Config {
            socket_path: None,
            bar: crate::config::BarConfig::default(),
            items: vec![
                scheduled_item("due", ItemSource::System(SystemPlugin::Time)),
                scheduled_item("failed", ItemSource::System(SystemPlugin::Time)),
            ],
        };
        // A partial snapshot failure still advances every attempted item's deadline,
        // avoiding a tight retry loop. `states` would contain only `due` here.
        super::advance_refresh_deadlines(
            &mut deadlines,
            &config,
            crate::rift::RiftBackendKind::Mach,
            &["due".into(), "failed".into()],
            now,
        );
        assert_eq!(deadlines["due"], now + std::time::Duration::from_secs(1));
        assert_eq!(deadlines["failed"], now + std::time::Duration::from_secs(1));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_updates_hover_state_for_items_without_plugins() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_hover_config(&config_path, &socket_path);

        let config = load_config(&config_path).expect("config");
        let mut daemon =
            Daemon::new(config_path.clone(), config, NativeRenderer::default()).expect("daemon");
        daemon.refresh_all_items().await.expect("initial render");

        dispatch(
            &mut daemon,
            crate::ipc::EventPayload {
                item_id: "clock".into(),
                event: crate::ipc::EventKind::HoverEnter,
                timestamp_ms: 0,
                mouse: crate::ipc::MouseState {
                    x: 10,
                    y: 10,
                    button: None,
                    scroll_delta: None,
                },
                modifiers: crate::ipc::Modifiers::default(),
            },
        )
        .await;

        let state = &daemon.state;
        assert_eq!(
            state.renderer.surface_state().active_hover_item.as_deref(),
            Some("clock")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_refreshes_builtin_items_on_click_but_not_hover() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_refreshing_config(&config_path, &socket_path);

        let renders = Arc::new(AtomicUsize::new(0));
        let renderer = CountingRenderer {
            renders: Arc::clone(&renders),
        };

        let config = load_config(&config_path).expect("config");
        let mut daemon = Daemon::new(config_path.clone(), config, renderer).expect("daemon");
        daemon.refresh_all_items().await.expect("initial render");
        assert_eq!(renders.load(Ordering::SeqCst), 1);

        dispatch(
            &mut daemon,
            crate::ipc::EventPayload {
                item_id: "clock".into(),
                event: crate::ipc::EventKind::HoverUpdate,
                timestamp_ms: 0,
                mouse: crate::ipc::MouseState {
                    x: 10,
                    y: 10,
                    button: None,
                    scroll_delta: None,
                },
                modifiers: crate::ipc::Modifiers::default(),
            },
        )
        .await;

        assert_eq!(renders.load(Ordering::SeqCst), 1);

        dispatch(
            &mut daemon,
            crate::ipc::EventPayload::from_trigger("clock".into(), crate::cli::TriggerEvent::Click),
        )
        .await;
        apply_next(&mut daemon).await;
        assert_eq!(renders.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_preserves_snapshot_errors_on_initial_refresh_and_click() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        fs::write(
            &config_path,
            format!(
                r#"
return {{
  socket_path = "{}",
  items = {{
    {{ id = "date", plugin = {{ kind = "date", format = string.rep("x", 256) }} }}
  }}
}}
"#,
                socket_path.display()
            ),
        )
        .expect("write config");
        let (lua, config) = crate::lua_executor::LuaExecutor::start(&config_path).expect("load");
        let renders = Arc::new(AtomicUsize::new(0));
        let renderer = CountingRenderer {
            renders: Arc::clone(&renders),
        };
        let mut daemon = Daemon::new_loaded(
            config_path,
            config,
            lua,
            renderer,
            Arc::new(FakeRiftBackend::default()),
        )
        .expect("daemon");

        let initial_error = daemon
            .refresh_all_items()
            .await
            .expect_err("format exceeds buffer");
        assert!(matches!(
            initial_error,
            crate::error::BarrsError::Unsupported(message) if message == "failed to format local time"
        ));
        let click_response = answer(
            &mut daemon,
            Request::TriggerItem {
                payload: crate::ipc::EventPayload::from_trigger(
                    "date".into(),
                    crate::cli::TriggerEvent::Click,
                ),
            },
        )
        .await;
        assert!(matches!(
            click_response,
            Response::Error { message }
                if message == "unsupported operation: failed to format local time"
        ));
        assert_eq!(renders.load(Ordering::SeqCst), 0);
        assert!(daemon.state.item_states.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_delivers_unchanged_hover_updates_to_lua_handlers() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        let marker_path = dir.path().join("hover-updates.txt");
        write_hover_update_config(&config_path, &socket_path, &marker_path);

        let config = load_config(&config_path).expect("config");
        let mut daemon =
            Daemon::new(config_path.clone(), config, NativeRenderer::default()).expect("daemon");
        daemon.refresh_all_items().await.expect("initial render");
        let event = crate::ipc::EventPayload {
            item_id: "clock".into(),
            event: crate::ipc::EventKind::HoverUpdate,
            timestamp_ms: 0,
            mouse: crate::ipc::MouseState {
                x: 10,
                y: 10,
                button: None,
                scroll_delta: None,
            },
            modifiers: crate::ipc::Modifiers::default(),
        };

        dispatch(&mut daemon, event.clone()).await;
        dispatch(&mut daemon, event).await;

        assert_eq!(fs::read_to_string(&marker_path).expect("marker"), "2");
    }

    fn scheduled_item(id: &str, source: ItemSource) -> ItemConfig {
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

    #[test]
    fn explicit_interval_overrides_the_source_default() {
        let item = ItemConfig {
            interval: Some(7),
            ..scheduled_item(
                "date",
                ItemSource::System(SystemPlugin::Date {
                    format: "%Y".into(),
                }),
            )
        };

        assert_eq!(
            super::item_refresh_interval(&item, crate::rift::RiftBackendKind::Mach),
            Some(std::time::Duration::from_secs(7))
        );
    }
}
