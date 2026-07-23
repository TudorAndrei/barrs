use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mlua::{Lua, LuaSerdeExt};
use serde_json::json;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::{self, Duration, Instant};

use crate::config::{Config, ItemConfig, PluginKind, load_config_with_runtime};
use crate::error::BarrsError;
use crate::ipc::{
    EventPayload, Request, Response, read_request_frame, send_request, write_response,
};
use crate::plugin::from_item_config;
use crate::render::{RenderItemSnapshot, Renderer};
use crate::rift::{
    RiftApplyResult, RiftBackendKind, RiftSnapshot, RiftSubscription, apply_event, select_backend,
    subscribe,
};

const EVENT_TICK_MS: u64 = 16;
const POLL_TICK_MS: u64 = 250;
const RIFT_DEBOUNCE_MS: u64 = 16;
const PENDING_IPC_REQUESTS: usize = 64;

pub struct Daemon<R: Renderer> {
    config_path: PathBuf,
    state: Arc<Mutex<DaemonState<R>>>,
    pending_refresh: Option<tokio::task::JoinHandle<PendingRefresh>>,
}

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
    let response = response_receiver.await.unwrap_or(Response::Error {
        message: "daemon stopped before responding".into(),
    });
    let _ = write_response(&mut stream, &response).await;
}

struct PendingRefresh {
    epoch: u64,
    item_ids: Vec<String>,
    states: Vec<(String, RenderItemSnapshot)>,
}

struct IncomingRequest {
    request: Request,
    response: oneshot::Sender<Response>,
}

struct DaemonState<R: Renderer> {
    config: Config,
    config_epoch: u64,
    lua: Lua,
    backend: RiftBackendKind,
    rift_subscription: Option<RiftSubscription>,
    rift_snapshot: Option<RiftSnapshot>,
    rift_dirty: bool,
    rift_debounce_deadline: Option<Instant>,
    last_rift_signature: Option<u64>,
    renderer: R,
    item_states: HashMap<String, RenderItemSnapshot>,
    refresh_deadlines: HashMap<String, Instant>,
}

impl<R: Renderer> Daemon<R> {
    pub fn new_loaded(
        config_path: PathBuf,
        config: Config,
        lua: Lua,
        renderer: R,
    ) -> Result<Self, BarrsError> {
        let backend = select_backend();
        let backend_kind = backend.kind();
        let rift_subscription = rift_subscription_for(&config);
        let state = DaemonState {
            refresh_deadlines: build_refresh_deadlines(&config, Instant::now(), backend_kind),
            config,
            config_epoch: 0,
            lua,
            backend: backend_kind,
            rift_subscription,
            rift_snapshot: None,
            rift_dirty: false,
            rift_debounce_deadline: None,
            last_rift_signature: None,
            renderer,
            item_states: HashMap::new(),
        };
        Ok(Self {
            config_path,
            state: Arc::new(Mutex::new(state)),
            pending_refresh: None,
        })
    }

    #[cfg(test)]
    fn new(config_path: PathBuf, config: Config, renderer: R) -> Result<Self, BarrsError> {
        let (_, lua) = load_config_with_runtime(&config_path)?;
        Self::new_loaded(config_path, config, lua, renderer)
    }

    pub async fn run(mut self) -> Result<(), BarrsError> {
        let socket_path = {
            let state = self.state.lock().await;
            state.config.socket_path()
        };
        let listener = bind_daemon_socket(&socket_path).await?;
        if let Err(error) = self.refresh_all_items().await {
            cleanup_socket(&socket_path)?;
            return Err(error);
        }
        signal_startup_ready();
        let (request_sender, mut request_receiver) = mpsc::channel(PENDING_IPC_REQUESTS);
        let mut event_tick = time::interval(Duration::from_millis(EVENT_TICK_MS));
        let mut poll_tick = time::interval(Duration::from_millis(POLL_TICK_MS));

        loop {
            tokio::select! {
                result = async { self.pending_refresh.as_mut().expect("pending refresh").await }, if self.pending_refresh.is_some() => {
                    self.pending_refresh = None;
                    match result {
                        Ok(pending) => {
                            if let Err(error) = self.apply_refresh(pending).await {
                                eprintln!("barrs: applying refresh failed: {error}");
                            }
                        }
                        Err(error) => eprintln!("barrs: refresh task failed: {error}"),
                    }
                }
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _)) => {
                            let sender = request_sender.clone();
                            tokio::spawn(async move {
                                serve_connection(stream, sender).await;
                            });
                        }
                        Err(error) => eprintln!("barrs: accept failed: {error}"),
                    }
                }
                incoming = request_receiver.recv() => {
                    let Some(incoming) = incoming else {
                        continue;
                    };
                    let response = match self.handle_request(incoming.request).await {
                        Ok(response) => response,
                        Err(error) => Response::Error {
                            message: error.to_string(),
                        },
                    };
                    let stop = matches!(response, Response::Ok { ref message } if message == "stopping");
                    let _ = incoming.response.send(response);
                    if stop {
                        break;
                    }
                }
                _ = event_tick.tick() => {
                    if let Err(error) = self.process_renderer_events().await {
                        eprintln!("barrs: renderer event processing failed: {error}");
                    }
                    if let Err(error) = self.process_rift_events().await {
                        eprintln!("barrs: rift event processing failed: {error}");
                    }
                }
                _ = poll_tick.tick() => {
                    if let Err(error) = self.refresh_due_items().await {
                        eprintln!("barrs: item refresh failed: {error}");
                    }
                }
            }
        }

        if let Some(pending) = self.pending_refresh.take() {
            pending.abort();
        }
        cleanup_socket(&socket_path)?;
        Ok(())
    }

    async fn handle_request(&mut self, request: Request) -> Result<Response, BarrsError> {
        match request {
            Request::Ping => Ok(Response::Pong),
            Request::Stop => Ok(Response::Ok {
                message: "stopping".into(),
            }),
            Request::Reload => {
                self.reload().await?;
                Ok(Response::Ok {
                    message: "reloaded".into(),
                })
            }
            Request::Status => {
                let state = self.state.lock().await;
                Ok(Response::Status {
                    running: true,
                    items: state.config.items.len(),
                    backend: state.backend,
                    config_path: self.config_path.clone(),
                })
            }
            Request::DumpState => {
                let state = self.state.lock().await;
                Ok(Response::State(json!(state.item_states)))
            }
            Request::RiftBackend => {
                let state = self.state.lock().await;
                Ok(Response::RiftBackend {
                    backend: state.backend,
                })
            }
            Request::TriggerItem { payload } => {
                self.dispatch_event(payload).await?;
                Ok(Response::Ok {
                    message: "event delivered".into(),
                })
            }
        }
    }

    async fn reload(&mut self) -> Result<(), BarrsError> {
        let (config, lua) = load_config_with_runtime(&self.config_path)?;
        let backend = select_backend();
        let backend_kind = backend.kind();
        let rift_snapshot = backend.snapshot().ok();
        let rift_signature = rift_snapshot.as_ref().map(RiftSnapshot::signature);
        let next_states = snapshots_for_config(&config, rift_snapshot.as_ref())?;
        let mut state = self.state.lock().await;
        state.renderer.reconcile(&config, &next_states)?;
        state.config_epoch = state.config_epoch.wrapping_add(1);
        state.backend = backend_kind;
        state.config = config;
        state.lua = lua;
        state.refresh_deadlines =
            build_refresh_deadlines(&state.config, Instant::now(), state.backend);
        state.rift_subscription = rift_subscription_for(&state.config);
        state.rift_snapshot = None;
        state.rift_dirty = false;
        state.rift_debounce_deadline = None;
        state.last_rift_signature = None;
        state.item_states = next_states
            .into_iter()
            .map(|snapshot| (snapshot.id.clone(), snapshot))
            .collect();
        state.rift_snapshot = rift_snapshot;
        state.last_rift_signature = rift_signature;
        Ok(())
    }

    async fn refresh_all_items(&mut self) -> Result<(), BarrsError> {
        let config = {
            let state = self.state.lock().await;
            state.config.clone()
        };
        let rift_snapshot = select_backend().snapshot().ok();
        let rift_signature = rift_snapshot.as_ref().map(RiftSnapshot::signature);
        let next_states = snapshots_for_config(&config, rift_snapshot.as_ref())?;

        let mut state = self.state.lock().await;
        state.renderer.reconcile(&config, &next_states)?;
        state.item_states = next_states
            .into_iter()
            .map(|snapshot| (snapshot.id.clone(), snapshot))
            .collect();
        state.rift_snapshot = rift_snapshot;
        state.rift_dirty = false;
        state.rift_debounce_deadline = None;
        state.last_rift_signature = rift_signature;
        Ok(())
    }

    async fn refresh_due_items(&mut self) -> Result<(), BarrsError> {
        let (due_items, config, epoch, cached_rift_snapshot) = {
            let state = self.state.lock().await;
            let now = Instant::now();
            let due = due_item_ids(&state.refresh_deadlines, now);
            (
                due,
                state.config.clone(),
                state.config_epoch,
                state.rift_snapshot.clone(),
            )
        };

        if !can_start_refresh(self.pending_refresh.is_some(), &due_items) {
            return Ok(());
        }

        let needs_rift = refresh_needs_rift(&config, &due_items);
        self.pending_refresh = Some(tokio::task::spawn_blocking(move || {
            let rift_snapshot = if needs_rift {
                select_backend().snapshot().ok().or(cached_rift_snapshot)
            } else {
                cached_rift_snapshot
            };
            let mut states = Vec::new();
            for (order, item) in config
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| due_items.iter().any(|item_id| item_id == &item.id))
            {
                match snapshot_for_item(item, order, rift_snapshot.as_ref()) {
                    Ok(snapshot) => states.push((item.id.clone(), snapshot)),
                    Err(error) => eprintln!("barrs: snapshot for {} failed: {error}", item.id),
                }
            }
            PendingRefresh {
                epoch,
                item_ids: due_items,
                states,
            }
        }));

        Ok(())
    }

    async fn apply_refresh(&mut self, pending: PendingRefresh) -> Result<(), BarrsError> {
        let mut state = self.state.lock().await;
        if !refresh_epoch_matches(state.config_epoch, pending.epoch) {
            return Ok(());
        }
        for (item_id, snapshot) in pending.states {
            state.renderer.render_item(&snapshot)?;
            state.item_states.insert(item_id, snapshot);
        }
        let now = Instant::now();
        let backend = state.backend;
        let config = state.config.clone();
        advance_refresh_deadlines(
            &mut state.refresh_deadlines,
            &config,
            backend,
            &pending.item_ids,
            now,
        );

        Ok(())
    }

    async fn process_renderer_events(&mut self) -> Result<(), BarrsError> {
        let events = {
            let mut state = self.state.lock().await;
            state.renderer.drain_events()?
        };
        for event in events {
            self.dispatch_event(event).await?;
        }
        Ok(())
    }

    async fn process_rift_events(&mut self) -> Result<(), BarrsError> {
        let now = Instant::now();
        let mut state = self.state.lock().await;
        let events = state
            .rift_subscription
            .as_ref()
            .map(|subscription| subscription.drain())
            .unwrap_or_default();

        if !events.is_empty() {
            if state.rift_snapshot.is_none() {
                state.rift_snapshot = select_backend().snapshot().ok();
            }
            let mut changed = false;
            let mut requires_resync = false;
            if let Some(snapshot) = state.rift_snapshot.as_mut() {
                for event in events {
                    match apply_event(snapshot, &event) {
                        RiftApplyResult::NoChange => {}
                        RiftApplyResult::Updated => changed = true,
                        RiftApplyResult::RequiresResync => {
                            requires_resync = true;
                            break;
                        }
                    }
                }
            }
            if requires_resync {
                state.rift_snapshot = select_backend().snapshot().ok();
            }
            if let Some(transition) = rift_debounce_transition(
                changed,
                requires_resync,
                state.rift_snapshot.is_some(),
                now,
            ) {
                state.rift_dirty = transition.dirty;
                state.rift_debounce_deadline = transition.deadline;
            }
        }

        let should_refresh = rift_debounce_due(state.rift_dirty, state.rift_debounce_deadline, now);
        if !should_refresh {
            return Ok(());
        }

        let item_ids = {
            state
                .config
                .items
                .iter()
                .filter(|item| is_rift_item(item))
                .map(|item| item.id.clone())
                .collect::<Vec<_>>()
        };
        let rift_snapshot = state.rift_snapshot.clone();
        drop(state);

        if item_ids.is_empty() {
            let mut state = self.state.lock().await;
            finish_rift_debounce(&mut state);
            return Ok(());
        }

        let Some(rift_snapshot) = rift_snapshot else {
            let mut state = self.state.lock().await;
            finish_rift_debounce(&mut state);
            return Ok(());
        };
        let next_signature = rift_snapshot.signature();
        let last_signature = {
            let state = self.state.lock().await;
            state.last_rift_signature
        };
        if !rift_signature_changed(last_signature, next_signature) {
            let mut state = self.state.lock().await;
            finish_rift_debounce(&mut state);
            return Ok(());
        }

        self.refresh_selected_items_with_rift(&item_ids, Some(&rift_snapshot))
            .await?;

        let mut state = self.state.lock().await;
        finish_rift_debounce(&mut state);
        state.last_rift_signature = Some(next_signature);
        Ok(())
    }

    async fn refresh_selected_items_with_rift(
        &mut self,
        item_ids: &[String],
        rift_snapshot: Option<&RiftSnapshot>,
    ) -> Result<(), BarrsError> {
        let config = {
            let state = self.state.lock().await;
            state.config.clone()
        };
        let mut next_states = HashMap::new();

        for (order, item) in config
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item_ids.iter().any(|item_id| item_id == &item.id))
        {
            let snapshot = snapshot_for_item(item, order, rift_snapshot)?;
            next_states.insert(item.id.clone(), snapshot);
        }

        let mut state = self.state.lock().await;
        for (item_id, snapshot) in next_states {
            state.renderer.render_item(&snapshot)?;
            state.item_states.insert(item_id, snapshot);
        }
        Ok(())
    }

    async fn dispatch_event(&mut self, payload: EventPayload) -> Result<(), BarrsError> {
        let (config, cached_rift_snapshot) = {
            let state = self.state.lock().await;
            (state.config.clone(), state.rift_snapshot.clone())
        };
        let item = config
            .items
            .iter()
            .find(|item| item.id == payload.item_id)
            .ok_or_else(|| BarrsError::InvalidConfig(format!("unknown item {}", payload.item_id)))?
            .clone();
        let order = config
            .items
            .iter()
            .position(|candidate| candidate.id == item.id)
            .unwrap_or(0);

        {
            let state = self.state.lock().await;
            invoke_lua_handler(&state.lua, &item, &payload)?;
        }

        {
            let mut state = self.state.lock().await;
            state.renderer.handle_event(&payload)?;
        }

        if !matches!(
            payload.event,
            crate::ipc::EventKind::HoverEnter
                | crate::ipc::EventKind::HoverLeave
                | crate::ipc::EventKind::HoverUpdate
        ) {
            let rift_snapshot = if is_rift_item(&item) {
                select_backend().snapshot().ok().or(cached_rift_snapshot)
            } else {
                cached_rift_snapshot
            };
            if let Some(mut plugin) = from_item_config(&item, rift_snapshot.as_ref()) {
                plugin.handle_event(&payload)?;
                let snapshot =
                    RenderItemSnapshot::from_item_config(&item, order, plugin.snapshot()?);
                let mut state = self.state.lock().await;
                state.renderer.render_item(&snapshot)?;
                state.item_states.insert(item.id.clone(), snapshot);
            }
        }

        Ok(())
    }
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
        Ok(Ok(Response::Pong)) => return Err(BarrsError::DaemonAlreadyRunning),
        Ok(Ok(_)) => return Err(BarrsError::DaemonAlreadyRunning),
        Ok(Err(BarrsError::DaemonUnavailable)) | Err(_) => {}
        Ok(Err(error)) => return Err(error),
    }
    fs::remove_file(path)?;
    Ok(())
}

fn signal_startup_ready() {
    if env::var_os("BARRS_STARTUP_READY").is_some() {
        let _ = std::io::stdout().write_all(b"barrs-ready\n");
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
            item_refresh_interval(item, backend)
                .map(|refresh_interval| (item.id.clone(), now + refresh_interval))
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

fn can_start_refresh(refresh_is_pending: bool, due_items: &[String]) -> bool {
    !refresh_is_pending && !due_items.is_empty()
}

fn refresh_epoch_matches(current_epoch: u64, pending_epoch: u64) -> bool {
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
        {
            deadlines.insert(item_id.clone(), now + refresh_interval);
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct RiftDebounceTransition {
    dirty: bool,
    deadline: Option<Instant>,
}

fn rift_debounce_transition(
    changed: bool,
    requires_resync: bool,
    snapshot_available: bool,
    now: Instant,
) -> Option<RiftDebounceTransition> {
    if requires_resync {
        return Some(RiftDebounceTransition {
            dirty: snapshot_available,
            deadline: Some(now + Duration::from_millis(RIFT_DEBOUNCE_MS)),
        });
    }
    changed.then(|| RiftDebounceTransition {
        dirty: true,
        deadline: Some(now + Duration::from_millis(RIFT_DEBOUNCE_MS)),
    })
}

fn rift_debounce_due(dirty: bool, deadline: Option<Instant>, now: Instant) -> bool {
    dirty && deadline.is_some_and(|deadline| deadline <= now)
}

fn rift_signature_changed(last_signature: Option<u64>, next_signature: u64) -> bool {
    last_signature != Some(next_signature)
}

fn item_refresh_interval(item: &ItemConfig, backend: RiftBackendKind) -> Option<Duration> {
    item.interval
        .map(|interval| Duration::from_secs(interval.max(1)))
        .or(match item.plugin.as_ref().map(|plugin| plugin.kind) {
            Some(PluginKind::Time) => Some(Duration::from_secs(1)),
            Some(PluginKind::Date) => Some(Duration::from_secs(30 * 60)),
            Some(PluginKind::Cpu | PluginKind::Gpu) => Some(Duration::from_secs(2)),
            Some(PluginKind::Battery) => Some(Duration::from_secs(10)),
            Some(PluginKind::RiftWorkspaces | PluginKind::RiftLayout) => {
                if backend == RiftBackendKind::Cli {
                    Some(Duration::from_millis(250))
                } else {
                    None
                }
            }
            _ => None,
        })
}

fn is_rift_item(item: &ItemConfig) -> bool {
    matches!(
        item.plugin.as_ref().map(|plugin| plugin.kind),
        Some(PluginKind::RiftWorkspaces | PluginKind::RiftLayout)
    )
}

fn rift_subscription_for(config: &Config) -> Option<RiftSubscription> {
    has_rift_items(config).then(subscribe).flatten()
}

fn has_rift_items(config: &Config) -> bool {
    config.items.iter().any(is_rift_item)
}

fn finish_rift_debounce<R: Renderer>(state: &mut DaemonState<R>) {
    state.rift_dirty = false;
    state.rift_debounce_deadline = None;
}

fn refresh_needs_rift(config: &Config, due_items: &[String]) -> bool {
    config
        .items
        .iter()
        .filter(|item| due_items.iter().any(|item_id| item_id == &item.id))
        .any(is_rift_item)
}

fn snapshots_for_config(
    config: &Config,
    rift_snapshot: Option<&RiftSnapshot>,
) -> Result<Vec<RenderItemSnapshot>, BarrsError> {
    config
        .items
        .iter()
        .enumerate()
        .map(|(order, item)| snapshot_for_item(item, order, rift_snapshot))
        .collect()
}

fn snapshot_for_item(
    item: &ItemConfig,
    order: usize,
    rift_snapshot: Option<&RiftSnapshot>,
) -> Result<RenderItemSnapshot, BarrsError> {
    if let Some(plugin) = from_item_config(item, rift_snapshot) {
        return Ok(RenderItemSnapshot::from_item_config(
            item,
            order,
            plugin.snapshot()?,
        ));
    }

    Ok(RenderItemSnapshot::from_item_config(
        item,
        order,
        json!({
            "text": item.label.clone().unwrap_or_else(|| item.id.clone()),
            "icon": item.icon,
        }),
    ))
}

fn invoke_lua_handler(
    lua: &Lua,
    item: &ItemConfig,
    payload: &EventPayload,
) -> Result<(), BarrsError> {
    let handler_name = match payload.event {
        crate::ipc::EventKind::Click => item.handlers.click.as_ref(),
        crate::ipc::EventKind::RightClick => item.handlers.right_click.as_ref(),
        crate::ipc::EventKind::Scroll => item.handlers.scroll.as_ref(),
        crate::ipc::EventKind::HoverEnter => item.handlers.hover_enter.as_ref(),
        crate::ipc::EventKind::HoverLeave => item.handlers.hover_leave.as_ref(),
        crate::ipc::EventKind::HoverUpdate => item.handlers.hover_update.as_ref(),
    };
    let Some(handler_name) = handler_name else {
        return Ok(());
    };

    let globals = lua.globals();
    let func: mlua::Function = globals
        .get(handler_name.as_str())
        .map_err(|_| BarrsError::InvalidConfig(format!("missing handler {handler_name}")))?;
    let ctx = lua.to_value(payload)?;
    func.call::<()>(ctx)?;
    Ok(())
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

    use crate::config::{Config, ItemConfig, ItemHandlers, PluginBinding, PluginKind, load_config};
    use crate::ipc::{Request, Response, default_socket_path, send_request};
    use crate::render::{NativeRenderer, NoopRenderer, Renderer};

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
  items = {{ {{ id = "workspaces", plugin = {{ kind = "rift_workspaces" }} }} }}
}}
"#,
                socket_path.display()
            ),
        )
        .expect("write config");
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

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_accepts_ping_and_stop() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_config(&config_path, &socket_path);

        let config = load_config(&config_path).expect("config");
        let daemon =
            Daemon::new(config_path.clone(), config, NoopRenderer::default()).expect("daemon");
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
        let daemon =
            Daemon::new(config_path.clone(), config, NoopRenderer::default()).expect("daemon");
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
    async fn daemon_survives_malformed_requests() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_config(&config_path, &socket_path);

        let config = load_config(&config_path).expect("config");
        let daemon =
            Daemon::new(config_path.clone(), config, NoopRenderer::default()).expect("daemon");
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
        let daemon =
            Daemon::new(config_path.clone(), config, NoopRenderer::default()).expect("daemon");
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
        let daemon =
            Daemon::new(config_path.clone(), config, NoopRenderer::default()).expect("daemon");
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
        let daemon = Daemon::new(config_path.clone(), config, renderer).expect("daemon");
        daemon
            .state
            .lock()
            .await
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
        let daemon =
            Daemon::new(config_path.clone(), config, NoopRenderer::default()).expect("daemon");
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
        let daemon =
            Daemon::new(config_path.clone(), config, NoopRenderer::default()).expect("daemon");
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
        let daemon =
            Daemon::new(config_path.clone(), config, NoopRenderer::default()).expect("daemon");
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

        let (config, lua) = crate::config::load_config_with_runtime(&config_path).expect("load");
        let mut daemon =
            Daemon::new_loaded(config_path.clone(), config, lua, NoopRenderer::default())
                .expect("daemon");
        assert_eq!(
            fs::read_to_string(&evaluation_path).expect("evaluation"),
            "1"
        );
        daemon.refresh_all_items().await.expect("initial render");
        assert_eq!(
            daemon.state.lock().await.item_states["generation"].text,
            "1"
        );
        daemon
            .dispatch_event(crate::ipc::EventPayload::from_trigger(
                "generation".into(),
                crate::cli::TriggerEvent::Click,
            ))
            .await
            .expect("handler");
        assert_eq!(fs::read_to_string(&handler_path).expect("handler"), "1");

        daemon.reload().await.expect("reload");
        assert_eq!(
            fs::read_to_string(&evaluation_path).expect("evaluation"),
            "2"
        );
        assert_eq!(
            daemon.state.lock().await.item_states["generation"].text,
            "2"
        );
        daemon
            .dispatch_event(crate::ipc::EventPayload::from_trigger(
                "generation".into(),
                crate::cli::TriggerEvent::Click,
            ))
            .await
            .expect("handler");
        assert_eq!(fs::read_to_string(&handler_path).expect("handler"), "2");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rift_debounce_clears_when_no_items_consume_rift() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_config(&config_path, &socket_path);
        let config = load_config(&config_path).expect("config");
        assert!(!super::has_rift_items(&config));
        let mut daemon =
            Daemon::new(config_path.clone(), config, NoopRenderer::default()).expect("daemon");
        {
            let mut state = daemon.state.lock().await;
            state.rift_dirty = true;
            state.rift_debounce_deadline = Some(tokio::time::Instant::now());
            state.rift_snapshot = Some(test_rift_snapshot("one"));
        }

        daemon.process_rift_events().await.expect("process events");

        let state = daemon.state.lock().await;
        assert!(!state.rift_dirty);
        assert_eq!(state.rift_debounce_deadline, None);
        assert!(state.rift_subscription.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rift_debounce_clears_equal_signatures_and_allows_later_change() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_rift_config(&config_path, &socket_path);
        let config = load_config(&config_path).expect("config");
        assert!(super::has_rift_items(&config));
        let mut daemon =
            Daemon::new(config_path.clone(), config, NoopRenderer::default()).expect("daemon");
        let initial = test_rift_snapshot("one");
        {
            let mut state = daemon.state.lock().await;
            state.rift_snapshot = Some(initial.clone());
            state.last_rift_signature = Some(initial.signature());
            state.rift_dirty = true;
            state.rift_debounce_deadline = Some(tokio::time::Instant::now());
        }

        daemon.process_rift_events().await.expect("equal signature");
        {
            let state = daemon.state.lock().await;
            assert!(!state.rift_dirty);
            assert_eq!(state.rift_debounce_deadline, None);
        }

        let changed = test_rift_snapshot("two");
        {
            let mut state = daemon.state.lock().await;
            state.rift_snapshot = Some(changed.clone());
            state.rift_dirty = true;
            state.rift_debounce_deadline = Some(tokio::time::Instant::now());
        }
        daemon
            .process_rift_events()
            .await
            .expect("changed signature");

        let state = daemon.state.lock().await;
        assert!(!state.rift_dirty);
        assert_eq!(state.rift_debounce_deadline, None);
        assert_eq!(state.last_rift_signature, Some(changed.signature()));
    }

    fn test_rift_snapshot(current_workspace: &str) -> crate::rift::RiftSnapshot {
        crate::rift::RiftSnapshot {
            current_workspace: current_workspace.into(),
            workspaces: vec![crate::rift::RiftWorkspace {
                name: current_workspace.into(),
                is_current: true,
                window_count: 1,
                has_windows: true,
            }],
            layout: "tiling".into(),
            window_count: 1,
        }
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

        daemon.reload().await.expect("reload");

        let state = daemon.state.lock().await;
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
        daemon.reload().await.expect("successful reload");
        assert_eq!(initializations.load(Ordering::SeqCst), 2);

        write_item_config(&config_path, &socket_path, &["new"]);
        fail_initialization.store(true, Ordering::SeqCst);
        daemon.reload().await.expect_err("reload must fail");

        let state = daemon.state.lock().await;
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
        let mut daemon =
            Daemon::new(config_path.clone(), config, NoopRenderer::default()).expect("daemon");

        let error = daemon
            .dispatch_event(crate::ipc::EventPayload::from_trigger(
                "missing".into(),
                crate::cli::TriggerEvent::HoverEnter,
            ))
            .await
            .expect_err("unknown item should fail");

        assert!(error.to_string().contains("unknown item missing"));
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
        let first = Daemon::new(config_path.clone(), config, NoopRenderer::default())
            .expect("first daemon");
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
            bar: Default::default(),
            items: vec![
                scheduled_item("due", PluginKind::Time),
                scheduled_item("failed", PluginKind::Time),
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

    #[test]
    fn rift_debounce_transitions_are_deterministic() {
        let now = tokio::time::Instant::now();
        let changed = super::rift_debounce_transition(true, false, true, now)
            .expect("changed event schedules refresh");
        assert!(changed.dirty);
        assert_eq!(
            changed.deadline,
            Some(now + std::time::Duration::from_millis(super::RIFT_DEBOUNCE_MS))
        );
        assert!(!super::rift_debounce_due(true, changed.deadline, now));
        assert!(super::rift_debounce_due(
            true,
            changed.deadline,
            now + std::time::Duration::from_millis(super::RIFT_DEBOUNCE_MS)
        ));

        let resync = super::rift_debounce_transition(false, true, true, now)
            .expect("resync schedules refresh when a snapshot was recovered");
        assert!(resync.dirty);
        assert!(super::rift_debounce_transition(false, false, true, now).is_none());

        assert!(super::rift_signature_changed(Some(1), 2));
        assert!(!super::rift_signature_changed(Some(2), 2));

        // Plan 009 owns terminal cleanup when there are no Rift consumers or a
        // recovered snapshot has the same signature. Do not treat this state as
        // a successful publication in this characterization test.
        let no_snapshot = super::rift_debounce_transition(false, true, false, now)
            .expect("resync still records its debounce deadline");
        assert!(!no_snapshot.dirty);
        assert!(!super::rift_debounce_due(
            no_snapshot.dirty,
            no_snapshot.deadline,
            now + std::time::Duration::from_millis(super::RIFT_DEBOUNCE_MS)
        ));
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

        daemon
            .dispatch_event(crate::ipc::EventPayload {
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
            })
            .await
            .expect("hover event");

        let state = daemon.state.lock().await;
        assert_eq!(
            state.renderer.surface_state().active_hover_item.as_deref(),
            Some("clock")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_does_not_rerender_plugin_items_for_hover_events() {
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

        daemon
            .dispatch_event(crate::ipc::EventPayload {
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
            })
            .await
            .expect("hover update");

        assert_eq!(renders.load(Ordering::SeqCst), 1);
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

        daemon
            .dispatch_event(event.clone())
            .await
            .expect("first update");
        daemon.dispatch_event(event).await.expect("same update");

        assert_eq!(fs::read_to_string(&marker_path).expect("marker"), "2");
    }

    #[test]
    fn rift_items_poll_only_on_cli_backend() {
        let item = crate::config::ItemConfig {
            id: "workspaces".into(),
            label: None,
            icon: None,
            placement: None,
            interval: None,
            plugin: Some(crate::config::PluginBinding {
                kind: crate::config::PluginKind::RiftWorkspaces,
                format: None,
            }),
            hover: None,
            handlers: crate::config::ItemHandlers::default(),
        };

        assert_eq!(
            super::item_refresh_interval(&item, crate::rift::RiftBackendKind::Cli),
            Some(std::time::Duration::from_millis(250))
        );
        assert_eq!(
            super::item_refresh_interval(&item, crate::rift::RiftBackendKind::Mach),
            None
        );
    }

    #[test]
    fn builtin_items_use_default_intervals() {
        let make_item = |kind| crate::config::ItemConfig {
            id: "item".into(),
            label: None,
            icon: None,
            placement: None,
            interval: None,
            plugin: Some(crate::config::PluginBinding { kind, format: None }),
            hover: None,
            handlers: crate::config::ItemHandlers::default(),
        };

        assert_eq!(
            super::item_refresh_interval(
                &make_item(crate::config::PluginKind::Time),
                crate::rift::RiftBackendKind::Mach
            ),
            Some(std::time::Duration::from_secs(1))
        );
        assert_eq!(
            super::item_refresh_interval(
                &make_item(crate::config::PluginKind::Cpu),
                crate::rift::RiftBackendKind::Mach
            ),
            Some(std::time::Duration::from_secs(2))
        );
        assert_eq!(
            super::item_refresh_interval(
                &make_item(crate::config::PluginKind::Gpu),
                crate::rift::RiftBackendKind::Mach
            ),
            Some(std::time::Duration::from_secs(2))
        );
        assert_eq!(
            super::item_refresh_interval(
                &make_item(crate::config::PluginKind::Battery),
                crate::rift::RiftBackendKind::Mach
            ),
            Some(std::time::Duration::from_secs(10))
        );
        assert_eq!(
            super::item_refresh_interval(
                &make_item(crate::config::PluginKind::Date),
                crate::rift::RiftBackendKind::Mach
            ),
            Some(std::time::Duration::from_secs(30 * 60))
        );
    }

    #[test]
    fn identifies_rift_items() {
        let make_item = |kind| ItemConfig {
            id: "item".into(),
            label: None,
            icon: None,
            placement: None,
            interval: None,
            plugin: Some(PluginBinding { kind, format: None }),
            hover: None,
            handlers: ItemHandlers::default(),
        };

        assert!(!super::is_rift_item(&make_item(PluginKind::Time)));
        assert!(!super::is_rift_item(&make_item(PluginKind::Cpu)));
        assert!(!super::is_rift_item(&make_item(PluginKind::Date)));
        assert!(super::is_rift_item(&make_item(PluginKind::RiftWorkspaces)));
        assert!(super::is_rift_item(&make_item(PluginKind::RiftLayout)));
    }

    #[test]
    fn refresh_needs_rift_only_for_due_rift_items() {
        let make_item = |id: &str, kind| ItemConfig {
            id: id.into(),
            label: None,
            icon: None,
            placement: None,
            interval: None,
            plugin: Some(PluginBinding { kind, format: None }),
            hover: None,
            handlers: ItemHandlers::default(),
        };
        let config = Config {
            socket_path: None,
            bar: Default::default(),
            items: vec![
                make_item("time", PluginKind::Time),
                make_item("workspaces", PluginKind::RiftWorkspaces),
            ],
        };

        assert!(!super::refresh_needs_rift(&config, &[]));
        assert!(!super::refresh_needs_rift(&config, &["time".into()]));
        assert!(super::refresh_needs_rift(&config, &["workspaces".into()]));
    }

    fn scheduled_item(id: &str, kind: PluginKind) -> ItemConfig {
        ItemConfig {
            id: id.into(),
            label: None,
            icon: None,
            placement: None,
            interval: None,
            plugin: Some(PluginBinding { kind, format: None }),
            hover: None,
            handlers: ItemHandlers::default(),
        }
    }

    #[test]
    fn explicit_interval_overrides_date_default() {
        let item = crate::config::ItemConfig {
            id: "date".into(),
            label: None,
            icon: None,
            placement: None,
            interval: Some(7),
            plugin: Some(crate::config::PluginBinding {
                kind: crate::config::PluginKind::Date,
                format: None,
            }),
            hover: None,
            handlers: crate::config::ItemHandlers::default(),
        };

        assert_eq!(
            super::item_refresh_interval(&item, crate::rift::RiftBackendKind::Mach),
            Some(std::time::Duration::from_secs(7))
        );
    }
}
