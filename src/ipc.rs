use std::env;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::{self, Duration};

use crate::cli::TriggerEvent;
use crate::error::BarrsError;
use crate::rift::RiftBackendKind;

pub fn default_socket_path() -> PathBuf {
    env::temp_dir().join("barrs.sock")
}

/// Maximum JSON request payload size, excluding its newline frame delimiter.
pub const MAX_REQUEST_FRAME_BYTES: usize = 64 * 1024;
pub const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestFrameError {
    TimedOut,
    EndOfFile,
    TooLarge,
    InvalidUtf8,
    InvalidJson,
    Io,
}

impl RequestFrameError {
    pub fn response(self) -> Response {
        let message = match self {
            Self::TimedOut => "request read timed out",
            Self::EndOfFile => "request ended before a complete frame",
            Self::TooLarge => "request frame exceeds the 65536-byte limit",
            Self::InvalidUtf8 => "request is not valid UTF-8",
            Self::InvalidJson => "invalid request JSON",
            Self::Io => "failed to read request",
        };
        Response::Error {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    pub item_id: String,
    pub event: EventKind,
    pub timestamp_ms: u64,
    pub mouse: MouseState,
    pub modifiers: Modifiers,
}

impl EventPayload {
    pub fn from_trigger(item_id: String, event: TriggerEvent) -> Self {
        Self {
            item_id,
            event: EventKind::from(event),
            timestamp_ms: current_timestamp_ms(),
            mouse: MouseState::default(),
            modifiers: Modifiers::default(),
        }
    }
}

pub fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
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

pub async fn read_request_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Request, RequestFrameError> {
    read_request_frame_with_limit(reader, MAX_REQUEST_FRAME_BYTES, REQUEST_READ_TIMEOUT).await
}

async fn read_request_frame_with_limit<R: AsyncRead + Unpin>(
    reader: &mut R,
    max_frame_bytes: usize,
    timeout: Duration,
) -> Result<Request, RequestFrameError> {
    time::timeout(timeout, read_request_frame_inner(reader, max_frame_bytes))
        .await
        .map_err(|_| RequestFrameError::TimedOut)?
}

async fn read_request_frame_inner<R: AsyncRead + Unpin>(
    reader: &mut R,
    max_frame_bytes: usize,
) -> Result<Request, RequestFrameError> {
    let mut frame = Vec::with_capacity(max_frame_bytes.saturating_add(1));
    let mut buffer = [0_u8; 1024];

    loop {
        let remaining = max_frame_bytes
            .saturating_add(1)
            .saturating_sub(frame.len());
        if remaining == 0 {
            return Err(RequestFrameError::TooLarge);
        }
        let read_capacity = remaining.min(buffer.len());
        let bytes = reader
            .read(&mut buffer[..read_capacity])
            .await
            .map_err(|_| RequestFrameError::Io)?;
        if bytes == 0 {
            return Err(RequestFrameError::EndOfFile);
        }
        frame.extend_from_slice(&buffer[..bytes]);

        if let Some(newline) = frame.iter().position(|byte| *byte == b'\n') {
            if newline > max_frame_bytes {
                return Err(RequestFrameError::TooLarge);
            }
            let request = std::str::from_utf8(&frame[..newline])
                .map_err(|_| RequestFrameError::InvalidUtf8)?;
            return serde_json::from_str(request).map_err(|_| RequestFrameError::InvalidJson);
        }
        if frame.len() > max_frame_bytes {
            return Err(RequestFrameError::TooLarge);
        }
    }
}

pub async fn write_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    response: &Response,
) -> Result<(), BarrsError> {
    let response_json = serde_json::to_string(response)?;
    writer.write_all(response_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
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
    use tokio::io::AsyncWriteExt;

    use super::{
        EventKind, EventPayload, MAX_REQUEST_FRAME_BYTES, Request, RequestFrameError,
        read_request_frame_with_limit,
    };
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

    #[tokio::test]
    async fn request_frame_accepts_the_exact_limit() {
        let (mut writer, mut reader) = tokio::io::duplex(MAX_REQUEST_FRAME_BYTES + 1);
        let request = request_with_payload_size(MAX_REQUEST_FRAME_BYTES);
        writer.write_all(&request).await.expect("write request");
        writer.write_all(b"\n").await.expect("write delimiter");

        assert!(
            read_request_frame_with_limit(
                &mut reader,
                MAX_REQUEST_FRAME_BYTES,
                std::time::Duration::from_secs(1)
            )
            .await
            .is_ok()
        );
    }

    #[tokio::test]
    async fn request_frame_rejects_one_byte_over_the_limit() {
        let (mut writer, mut reader) = tokio::io::duplex(MAX_REQUEST_FRAME_BYTES + 2);
        writer
            .write_all(&request_with_payload_size(MAX_REQUEST_FRAME_BYTES + 1))
            .await
            .expect("write request");

        assert!(matches!(
            read_request_frame_with_limit(
                &mut reader,
                MAX_REQUEST_FRAME_BYTES,
                std::time::Duration::from_secs(1)
            )
            .await,
            Err(RequestFrameError::TooLarge)
        ));
    }

    #[tokio::test]
    async fn partial_frame_times_out() {
        let (_writer, mut reader) = tokio::io::duplex(8);

        assert!(matches!(
            read_request_frame_with_limit(
                &mut reader,
                MAX_REQUEST_FRAME_BYTES,
                std::time::Duration::from_millis(1)
            )
            .await,
            Err(RequestFrameError::TimedOut)
        ));
    }

    fn request_with_payload_size(size: usize) -> Vec<u8> {
        let mut payload = EventPayload::from_trigger("x".into(), TriggerEvent::Click);
        let base_size = serde_json::to_vec(&Request::TriggerItem {
            payload: payload.clone(),
        })
        .expect("serialize request")
        .len();
        payload.item_id = "x".repeat(size.saturating_sub(base_size).saturating_add(1));
        let request = serde_json::to_vec(&Request::TriggerItem { payload }).expect("serialize");
        assert_eq!(request.len(), size);
        request
    }
}
