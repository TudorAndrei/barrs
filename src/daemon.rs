use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mlua::{Lua, LuaSerdeExt};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tokio::time::{self, Duration, Instant};

use crate::config::{Config, ItemConfig, PluginKind, load_config};
use crate::error::BarrsError;
use crate::ipc::{EventPayload, Request, Response};
use crate::plugin::from_item_config;
use crate::render::{RenderItemSnapshot, Renderer};
use crate::rift::{RiftBackendKind, RiftSnapshot, select_backend};

pub struct Daemon<R: Renderer> {
    config_path: PathBuf,
    state: Arc<Mutex<DaemonState<R>>>,
}

struct DaemonState<R: Renderer> {
    config: Config,
    backend: RiftBackendKind,
    renderer: R,
    item_states: HashMap<String, RenderItemSnapshot>,
    refresh_deadlines: HashMap<String, Instant>,
}

impl<R: Renderer> Daemon<R> {
    pub fn new(config_path: PathBuf, config: Config, renderer: R) -> Result<Self, BarrsError> {
        let backend = select_backend();
        let state = DaemonState {
            refresh_deadlines: build_refresh_deadlines(&config, Instant::now()),
            config,
            backend: backend.kind(),
            renderer,
            item_states: HashMap::new(),
        };
        Ok(Self {
            config_path,
            state: Arc::new(Mutex::new(state)),
        })
    }

    pub async fn run(mut self) -> Result<(), BarrsError> {
        self.refresh_all_items().await?;

        let socket_path = {
            let state = self.state.lock().await;
            state.config.socket_path()
        };
        cleanup_socket(&socket_path)?;
        let listener = UnixListener::bind(&socket_path)?;
        let mut ticker = time::interval(Duration::from_millis(250));

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    let (stream, _) = accept_result?;
                    let should_stop = self.handle_connection(stream).await?;
                    if should_stop {
                        break;
                    }
                }
                _ = ticker.tick() => {
                    self.process_renderer_events().await?;
                    self.refresh_due_items().await?;
                }
            }
        }

        cleanup_socket(&socket_path)?;
        Ok(())
    }

    async fn handle_connection(&mut self, stream: UnixStream) -> Result<bool, BarrsError> {
        let mut line = String::new();
        let mut reader = BufReader::new(stream);
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            return Ok(false);
        }
        let request: Request = serde_json::from_str(line.trim())?;
        let response = self.handle_request(request).await?;
        let stop = matches!(response, Response::Ok { ref message } if message == "stopping");
        let mut stream = reader.into_inner();
        let response_json = serde_json::to_string(&response)?;
        stream.write_all(response_json.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;
        Ok(stop)
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
            Request::ValidateConfig { path } => {
                load_config(&path)?;
                Ok(Response::Ok {
                    message: format!("validated {}", path.display()),
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
        let config = load_config(&self.config_path)?;
        let mut state = self.state.lock().await;
        state.config = config;
        state.item_states.clear();
        state.refresh_deadlines = build_refresh_deadlines(&state.config, Instant::now());
        let config_clone = state.config.clone();
        state.renderer.initialize(&config_clone)?;
        drop(state);
        self.refresh_all_items().await
    }

    async fn refresh_all_items(&mut self) -> Result<(), BarrsError> {
        let config = {
            let mut state = self.state.lock().await;
            let config = state.config.clone();
            state.renderer.initialize(&config)?;
            config
        };
        let rift_snapshot = select_backend().snapshot().ok();
        let mut next_states = HashMap::new();

        for (order, item) in config.items.iter().enumerate() {
            let snapshot = snapshot_for_item(item, order, rift_snapshot.as_ref())?;
            next_states.insert(item.id.clone(), snapshot);
        }

        let mut state = self.state.lock().await;
        for (item_id, snapshot) in next_states {
            state.renderer.render_item(&snapshot)?;
            state.item_states.insert(item_id, snapshot);
        }
        Ok(())
    }

    async fn refresh_due_items(&mut self) -> Result<(), BarrsError> {
        let due_items = {
            let state = self.state.lock().await;
            let now = Instant::now();
            state
                .refresh_deadlines
                .iter()
                .filter_map(|(item_id, deadline)| {
                    if *deadline <= now {
                        Some(item_id.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };

        if due_items.is_empty() {
            return Ok(());
        }

        self.refresh_selected_items(&due_items).await?;

        let config = {
            let state = self.state.lock().await;
            state.config.clone()
        };
        let now = Instant::now();
        let mut state = self.state.lock().await;
        for item_id in due_items {
            if let Some(item) = config.items.iter().find(|item| item.id == item_id) {
                if let Some(refresh_interval) = item_refresh_interval(item) {
                    state.refresh_deadlines.insert(item_id, now + refresh_interval);
                }
            }
        }

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

    async fn refresh_selected_items(&mut self, item_ids: &[String]) -> Result<(), BarrsError> {
        let config = {
            let state = self.state.lock().await;
            state.config.clone()
        };
        let rift_snapshot = select_backend().snapshot().ok();
        let mut next_states = HashMap::new();

        for (order, item) in config
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item_ids.iter().any(|item_id| item_id == &item.id))
        {
            let snapshot = snapshot_for_item(item, order, rift_snapshot.as_ref())?;
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
        let config = {
            let state = self.state.lock().await;
            state.config.clone()
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

        invoke_lua_handler(&self.config_path, &item, &payload)?;

        if let Some(mut plugin) = from_item_config(&item, select_backend().snapshot().ok().as_ref())
        {
            plugin.handle_event(&payload)?;
            let snapshot = RenderItemSnapshot::from_item_config(&item, order, plugin.snapshot()?);
            let mut state = self.state.lock().await;
            state.renderer.handle_event(&payload)?;
            state.renderer.render_item(&snapshot)?;
            state.item_states.insert(item.id.clone(), snapshot);
        }

        Ok(())
    }
}

fn cleanup_socket(path: &Path) -> Result<(), BarrsError> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn build_refresh_deadlines(config: &Config, now: Instant) -> HashMap<String, Instant> {
    config
        .items
        .iter()
        .filter_map(|item| {
            item_refresh_interval(item).map(|refresh_interval| (item.id.clone(), now + refresh_interval))
        })
        .collect()
}

fn item_refresh_interval(item: &ItemConfig) -> Option<Duration> {
    item.refresh_secs
        .map(|refresh_secs| Duration::from_secs(refresh_secs.max(1)))
        .or(match item.plugin.as_ref().map(|plugin| plugin.kind) {
        Some(PluginKind::RiftWorkspaces | PluginKind::RiftLayout) => {
            Some(Duration::from_millis(250))
        }
        _ => None,
    })
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
    config_path: &Path,
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

    let source = fs::read_to_string(config_path)?;
    let lua = Lua::new();
    lua.load(&source)
        .set_name(config_path.to_string_lossy())
        .exec()?;
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
        atomic::{AtomicUsize, Ordering},
    };

    use tempfile::tempdir;
    use tokio::task::JoinHandle;

    use crate::config::load_config;
    use crate::ipc::{Request, Response, default_socket_path, send_request};
    use crate::render::{NoopRenderer, Renderer};

    use super::Daemon;

    struct CountingRenderer {
        renders: Arc<AtomicUsize>,
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
      refresh_secs = 1
    }}
  }}
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
    {{
      id = "workspaces",
      plugin = {{ kind = "rift_workspaces" }}
    }}
  }}
}}
"#,
                socket_path.display()
            ),
        )
        .expect("write config");
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

    #[test]
    fn default_socket_path_is_stable() {
        assert_eq!(
            default_socket_path(),
            std::path::PathBuf::from("/tmp/barrs.sock")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_refreshes_scheduled_items() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_refreshing_config(&config_path, &socket_path);

        let renders = Arc::new(AtomicUsize::new(0));
        let renderer = CountingRenderer {
            renders: Arc::clone(&renders),
        };

        let config = load_config(&config_path).expect("config");
        let daemon = Daemon::new(config_path.clone(), config, renderer).expect("daemon");
        let task: JoinHandle<Result<(), crate::error::BarrsError>> = tokio::spawn(daemon.run());

        for _ in 0..20 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        tokio::time::sleep(std::time::Duration::from_millis(1250)).await;
        let _ = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop");
        task.await.expect("join").expect("daemon result");

        assert!(renders.load(Ordering::SeqCst) >= 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daemon_refreshes_rift_items_without_explicit_interval() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("barrs.sock");
        let config_path = dir.path().join("barrs.lua");
        write_rift_config(&config_path, &socket_path);

        let renders = Arc::new(AtomicUsize::new(0));
        let renderer = CountingRenderer {
            renders: Arc::clone(&renders),
        };

        let config = load_config(&config_path).expect("config");
        let daemon = Daemon::new(config_path.clone(), config, renderer).expect("daemon");
        let task: JoinHandle<Result<(), crate::error::BarrsError>> = tokio::spawn(daemon.run());

        for _ in 0..20 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        tokio::time::sleep(std::time::Duration::from_millis(1250)).await;
        let _ = send_request(&socket_path, &Request::Stop)
            .await
            .expect("stop");
        task.await.expect("join").expect("daemon result");

        assert!(renders.load(Ordering::SeqCst) >= 2);
    }
}
