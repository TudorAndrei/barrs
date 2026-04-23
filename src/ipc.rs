use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::cli::TriggerEvent;
use crate::error::BarrsError;
use crate::rift::RiftBackendKind;

pub const DEFAULT_SOCKET_PATH: &str = "/tmp/barrs.sock";

pub fn default_socket_path() -> PathBuf {
    PathBuf::from(DEFAULT_SOCKET_PATH)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    pub item_id: String,
    pub event: EventKind,
    pub timestamp_ms: u128,
    pub mouse: MouseState,
    pub modifiers: Modifiers,
}

impl EventPayload {
    pub fn from_trigger(item_id: String, event: TriggerEvent) -> Self {
        Self {
            item_id,
            event: EventKind::from(event),
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            mouse: MouseState::default(),
            modifiers: Modifiers::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Click,
    RightClick,
    Scroll,
    HoverEnter,
    HoverLeave,
    HoverUpdate,
}

impl From<TriggerEvent> for EventKind {
    fn from(value: TriggerEvent) -> Self {
        match value {
            TriggerEvent::Click => Self::Click,
            TriggerEvent::RightClick => Self::RightClick,
            TriggerEvent::Scroll => Self::Scroll,
            TriggerEvent::HoverEnter => Self::HoverEnter,
            TriggerEvent::HoverLeave => Self::HoverLeave,
            TriggerEvent::HoverUpdate => Self::HoverUpdate,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub button: Option<String>,
    pub scroll_delta: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub option: bool,
    pub command: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Ping,
    Stop,
    Reload,
    Status,
    DumpState,
    RiftBackend,
    ValidateConfig { path: PathBuf },
    TriggerItem { payload: EventPayload },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Pong,
    Ok {
        message: String,
    },
    Status {
        running: bool,
        items: usize,
        backend: RiftBackendKind,
        config_path: PathBuf,
    },
    State(serde_json::Value),
    RiftBackend {
        backend: RiftBackendKind,
    },
    Error {
        message: String,
    },
}

pub async fn send_request(socket_path: &Path, request: &Request) -> Result<Response, BarrsError> {
    let mut stream = UnixStream::connect(socket_path)
        .await
        .map_err(|_| BarrsError::DaemonUnavailable)?;
    let request_json = serde_json::to_string(request)?;
    stream.write_all(request_json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    let count = reader.read_line(&mut line).await?;
    if count == 0 {
        return Err(BarrsError::DaemonUnavailable);
    }
    Ok(serde_json::from_str(line.trim())?)
}

#[cfg(test)]
mod tests {
    use super::{EventKind, EventPayload, Request};
    use crate::cli::TriggerEvent;

    #[test]
    fn serializes_trigger_request() {
        let request = Request::TriggerItem {
            payload: EventPayload::from_trigger("time".into(), TriggerEvent::Click),
        };
        let json = serde_json::to_string(&request).expect("serialize request");
        assert!(json.contains("\"trigger_item\""));
        assert!(json.contains("\"click\""));
    }

    #[test]
    fn maps_trigger_event() {
        let payload = EventPayload::from_trigger("cpu".into(), TriggerEvent::HoverLeave);
        assert_eq!(payload.event, EventKind::HoverLeave);
    }
}
