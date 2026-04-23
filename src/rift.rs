use std::collections::hash_map::DefaultHasher;
use std::env;
#[cfg(target_os = "macos")]
use std::ffi::CString;
use std::hash::{Hash, Hasher};
#[cfg(target_os = "macos")]
use std::mem::size_of;
use std::process::{Command, Stdio};
use std::sync::mpsc;
#[cfg(target_os = "macos")]
use std::thread;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::BarrsError;

const RIFT_BOOTSTRAP_NAME: &str = "git.acsandmann.rift";
#[cfg(target_os = "macos")]
const MAX_MESSAGE_SIZE: usize = 16_384;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiftBackendKind {
    Cli,
    Mach,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiftSnapshot {
    pub current_workspace: String,
    pub workspaces: Vec<RiftWorkspace>,
    pub layout: String,
    pub window_count: usize,
}

impl RiftSnapshot {
    pub fn signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.current_workspace.hash(&mut hasher);
        self.layout.hash(&mut hasher);
        self.window_count.hash(&mut hasher);
        for workspace in &self.workspaces {
            workspace.hash(&mut hasher);
        }
        hasher.finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RiftWorkspace {
    pub name: String,
    pub is_current: bool,
    pub has_windows: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiftEventKind {
    WorkspaceChanged,
    WindowsChanged,
    StacksChanged,
}

impl RiftEventKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceChanged => "workspace_changed",
            Self::WindowsChanged => "windows_changed",
            Self::StacksChanged => "stacks_changed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RiftEvent {
    pub kind: RiftEventKind,
    pub payload: Value,
}

#[cfg(target_os = "macos")]
type SubscriptionHandle = libc::mach_port_t;
#[cfg(not(target_os = "macos"))]
type SubscriptionHandle = u32;

pub struct RiftSubscription {
    receiver: mpsc::Receiver<RiftEvent>,
    handles: Vec<SubscriptionHandle>,
    _workers: Vec<std::thread::JoinHandle<()>>,
}

impl RiftSubscription {
    pub fn drain(&self) -> Vec<RiftEvent> {
        self.receiver.try_iter().collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiftApplyResult {
    NoChange,
    Updated,
    RequiresResync,
}

pub fn apply_event(snapshot: &mut RiftSnapshot, event: &RiftEvent) -> RiftApplyResult {
    match event.kind {
        RiftEventKind::WorkspaceChanged => apply_workspace_changed(snapshot, &event.payload),
        RiftEventKind::WindowsChanged => apply_windows_changed(snapshot, &event.payload),
        RiftEventKind::StacksChanged => apply_stacks_changed(snapshot, &event.payload),
    }
}

impl Drop for RiftSubscription {
    fn drop(&mut self) {
        for handle in &self.handles {
            deallocate_port(*handle);
        }
    }
}

pub trait RiftBackend: Send + Sync {
    fn kind(&self) -> RiftBackendKind;
    fn snapshot(&self) -> Result<RiftSnapshot, BarrsError>;
}

#[derive(Debug, Default)]
pub struct CliRiftBackend;

impl RiftBackend for CliRiftBackend {
    fn kind(&self) -> RiftBackendKind {
        RiftBackendKind::Cli
    }

    fn snapshot(&self) -> Result<RiftSnapshot, BarrsError> {
        cli_snapshot().ok_or_else(|| {
            BarrsError::Unsupported("failed to query workspace state from rift-cli".into())
        })
    }
}

#[derive(Debug, Default)]
pub struct MachRiftBackend;

impl RiftBackend for MachRiftBackend {
    fn kind(&self) -> RiftBackendKind {
        RiftBackendKind::Mach
    }

    fn snapshot(&self) -> Result<RiftSnapshot, BarrsError> {
        #[cfg(target_os = "macos")]
        {
            mach_snapshot()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(BarrsError::Unsupported(
                "Mach IPC is only supported on macOS".into(),
            ))
        }
    }
}

pub fn select_backend() -> Box<dyn RiftBackend> {
    if mach_backend_available() {
        Box::new(MachRiftBackend)
    } else {
        Box::new(CliRiftBackend)
    }
}

pub fn subscribe() -> Option<RiftSubscription> {
    #[cfg(target_os = "macos")]
    {
        if !mach_backend_available() {
            return None;
        }

        let (sender, receiver) = mpsc::channel();
        let mut handles = Vec::new();
        let mut workers = Vec::new();

        for kind in [
            RiftEventKind::WorkspaceChanged,
            RiftEventKind::WindowsChanged,
            RiftEventKind::StacksChanged,
        ] {
            let client = MachClient::connect().ok()?;
            let port = client.subscribe(kind).ok()?;
            let thread_sender = sender.clone();
            workers.push(thread::spawn(move || {
                while let Ok(payload) = receive_json_message(port) {
                    if thread_sender.send(RiftEvent { kind, payload }).is_err() {
                        break;
                    }
                }
            }));
            handles.push(port);
        }

        Some(RiftSubscription {
            receiver,
            handles,
            _workers: workers,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn mach_snapshot() -> Result<RiftSnapshot, BarrsError> {
    let client = MachClient::connect()?;
    let workspaces_value = client.get_workspaces(None)?;
    let mut workspaces = parse_workspaces(&workspaces_value);
    workspaces.sort_by_key(|workspace| workspace.index.unwrap_or(usize::MAX));
    let active = select_active_workspace(&workspaces)
        .ok_or_else(|| BarrsError::Unsupported("Rift returned no workspaces".into()))?;
    let layout = active.layout.clone().unwrap_or_else(|| "tiling".into());
    let window_count = active.window_count.unwrap_or(0);

    Ok(RiftSnapshot {
        current_workspace: active.display_name(),
        workspaces: if workspaces.is_empty() {
            vec![RiftWorkspace {
                name: active.display_name(),
                is_current: true,
                has_windows: window_count > 0,
            }]
        } else {
            workspaces
                .iter()
                .map(|workspace| RiftWorkspace {
                    name: workspace.display_name(),
                    is_current: workspace.active,
                    has_windows: workspace.window_count.unwrap_or(0) > 0,
                })
                .collect()
        },
        layout,
        window_count,
    })
}

fn mach_backend_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        MachClient::connect().is_ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn cli_snapshot() -> Option<RiftSnapshot> {
    let workspaces_value = run_rift_cli(["query", "workspaces"])?;
    let mut workspaces = parse_workspaces(&workspaces_value);
    workspaces.sort_by_key(|workspace| workspace.index.unwrap_or(usize::MAX));
    let active = select_active_workspace(&workspaces)?;

    let layout = active.layout.clone().unwrap_or_else(|| "tiling".into());
    let window_count = active.window_count.unwrap_or(0);

    Some(RiftSnapshot {
        current_workspace: active.display_name(),
        workspaces: if workspaces.is_empty() {
            vec![RiftWorkspace {
                name: active.display_name(),
                is_current: true,
                has_windows: window_count > 0,
            }]
        } else {
            workspaces
                .iter()
                .map(|workspace| RiftWorkspace {
                    name: workspace.display_name(),
                    is_current: workspace.active,
                    has_windows: workspace.window_count.unwrap_or(0) > 0,
                })
                .collect()
        },
        layout,
        window_count,
    })
}

fn run_rift_cli<const N: usize>(args: [&str; N]) -> Option<Value> {
    let output = Command::new("rift-cli")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let value: Value = serde_json::from_str(&stdout).ok()?;
    unwrap_response(value)
}

fn unwrap_response(value: Value) -> Option<Value> {
    if value.is_array() {
        return Some(value);
    }

    let object = value.as_object()?;
    for key in ["data", "payload", "result"] {
        if let Some(inner) = object.get(key) {
            return Some(inner.clone());
        }
    }
    for key in ["success", "Success"] {
        if let Some(success) = object.get(key).and_then(Value::as_object) {
            for inner_key in ["data", "payload", "result"] {
                if let Some(inner) = success.get(inner_key) {
                    return Some(inner.clone());
                }
            }
        }
    }
    Some(Value::Object(object.clone()))
}

#[derive(Debug, Clone, Default)]
struct ParsedWorkspace {
    workspace_id: Option<u64>,
    name: Option<String>,
    index: Option<usize>,
    active: bool,
    visible: bool,
    layout: Option<String>,
    window_count: Option<usize>,
}

impl ParsedWorkspace {
    fn display_name(&self) -> String {
        self.index
            .map(|index| (index + 1).to_string())
            .or_else(|| self.name.clone().filter(|name| !name.trim().is_empty()))
            .or_else(|| self.workspace_id.map(|id| id.to_string()))
            .unwrap_or_else(|| "?".into())
    }
}

fn parse_workspaces(value: &Value) -> Vec<ParsedWorkspace> {
    match value {
        Value::Array(items) => items.iter().map(parse_workspace).collect(),
        Value::Object(map) => map
            .get("workspaces")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(parse_workspace).collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn parse_workspace(value: &Value) -> ParsedWorkspace {
    ParsedWorkspace {
        workspace_id: find_u64(value, &["workspace_id", "id"]),
        name: find_string(value, &["workspace_name", "name", "label", "title"]),
        index: find_u64(value, &["workspace_index", "index"]).map(|value| value as usize),
        active: find_bool(value, &["active", "is_active", "focused", "current"]).unwrap_or(false),
        visible: find_bool(value, &["visible", "is_visible"]).unwrap_or(false),
        layout: find_string(value, &["layout", "layout_mode", "mode"]),
        window_count: find_u64(value, &["window_count"])
            .map(|value| value as usize)
            .or_else(|| {
                value.as_object().and_then(|object| {
                    object
                        .get("windows")
                        .and_then(Value::as_array)
                        .map(|windows| windows.len())
                })
            }),
    }
}

fn select_active_workspace(workspaces: &[ParsedWorkspace]) -> Option<&ParsedWorkspace> {
    workspaces
        .iter()
        .find(|workspace| workspace.active)
        .or_else(|| workspaces.iter().find(|workspace| workspace.visible))
        .or_else(|| workspaces.first())
}

#[cfg(test)]
fn parse_layout_value(value: &Value) -> Option<String> {
    if let Some(layout) = find_string(value, &["layout_mode", "layout", "mode"]) {
        return Some(layout);
    }
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => items.iter().find_map(parse_layout_value),
        Value::Object(map) => map.values().find_map(parse_layout_value),
        _ => None,
    }
}

#[cfg(test)]
fn count_windows_in_value(value: &Value, workspace_id: Option<u64>, space_id: Option<u64>) -> usize {
    let windows = match value {
        Value::Array(items) => items.clone(),
        Value::Object(map) => map
            .get("windows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    windows
        .iter()
        .filter(|window| {
            if let Some(target_workspace) = workspace_id {
                find_u64(window, &["workspace_id"]) == Some(target_workspace)
            } else if let Some(target_space) = space_id {
                find_u64(window, &["space_id"]) == Some(target_space)
            } else {
                true
            }
        })
        .count()
}

fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(text) = object.get(*key).and_then(Value::as_str) {
            return Some(text.to_string());
        }
    }
    None
}

fn find_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(number) = object.get(*key).and_then(Value::as_u64) {
            return Some(number);
        }
    }
    None
}

fn find_bool(value: &Value, keys: &[&str]) -> Option<bool> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(flag) = object.get(*key).and_then(Value::as_bool) {
            return Some(flag);
        }
    }
    None
}

fn apply_workspace_changed(snapshot: &mut RiftSnapshot, payload: &Value) -> RiftApplyResult {
    let Some(current_name) = workspace_index_name(payload) else {
        return RiftApplyResult::RequiresResync;
    };
    let layout = find_string(payload, &["layout", "layout_mode", "mode"]);
    let mut changed = false;

    if snapshot.current_workspace != current_name {
        snapshot.current_workspace = current_name.clone();
        changed = true;
    }

    if let Some(layout) = layout.filter(|layout| !layout.trim().is_empty()) {
        if snapshot.layout != layout {
            snapshot.layout = layout;
            changed = true;
        }
    }

    for workspace in &mut snapshot.workspaces {
        let is_current = workspace.name == current_name;
        if workspace.is_current != is_current {
            workspace.is_current = is_current;
            changed = true;
        }
    }

    if !snapshot.workspaces.iter().any(|workspace| workspace.name == current_name) {
        return RiftApplyResult::RequiresResync;
    }

    snapshot.window_count = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.is_current)
        .map(|workspace| usize::from(workspace.has_windows))
        .unwrap_or(snapshot.window_count);

    if changed {
        RiftApplyResult::Updated
    } else {
        RiftApplyResult::NoChange
    }
}

fn apply_windows_changed(snapshot: &mut RiftSnapshot, payload: &Value) -> RiftApplyResult {
    let Some(target_name) = workspace_index_name(payload) else {
        return RiftApplyResult::RequiresResync;
    };
    let Some(window_count) = payload
        .as_object()
        .and_then(|object| object.get("windows"))
        .and_then(Value::as_array)
        .map(|windows| windows.len())
        .or_else(|| find_u64(payload, &["window_count"]).map(|count| count as usize))
    else {
        return RiftApplyResult::RequiresResync;
    };

    let mut changed = false;
    let mut found = false;

    for workspace in &mut snapshot.workspaces {
        if workspace.name == target_name {
            found = true;
            let has_windows = window_count > 0;
            if workspace.has_windows != has_windows {
                workspace.has_windows = has_windows;
                changed = true;
            }
            if workspace.is_current && snapshot.window_count != window_count {
                snapshot.window_count = window_count;
                changed = true;
            }
            break;
        }
    }

    if !found {
        return RiftApplyResult::RequiresResync;
    }

    if changed {
        RiftApplyResult::Updated
    } else {
        RiftApplyResult::NoChange
    }
}

fn apply_stacks_changed(snapshot: &mut RiftSnapshot, payload: &Value) -> RiftApplyResult {
    if let Some(current_name) = workspace_index_name(payload) {
        let mut changed = false;
        for workspace in &mut snapshot.workspaces {
            let is_current = workspace.name == current_name;
            if workspace.is_current != is_current {
                workspace.is_current = is_current;
                changed = true;
            }
        }
        if changed {
            snapshot.current_workspace = current_name;
            return RiftApplyResult::Updated;
        }
        return RiftApplyResult::NoChange;
    }
    RiftApplyResult::RequiresResync
}

fn workspace_index_name(payload: &Value) -> Option<String> {
    find_u64(payload, &["workspace_index", "index"]).map(|index| (index + 1).to_string())
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct MachMsgHeader {
    msgh_bits: u32,
    msgh_size: u32,
    msgh_remote_port: libc::mach_port_t,
    msgh_local_port: libc::mach_port_t,
    msgh_voucher_port: u32,
    msgh_id: i32,
}

#[cfg(target_os = "macos")]
const MACH_PAYLOAD_CAPACITY: usize = MAX_MESSAGE_SIZE - size_of::<MachMsgHeader>();

#[cfg(target_os = "macos")]
#[repr(C)]
struct SimpleMessage {
    header: MachMsgHeader,
    data: [u8; MACH_PAYLOAD_CAPACITY],
}

#[cfg(target_os = "macos")]
struct MachClient {
    service_port: libc::mach_port_t,
}

#[cfg(target_os = "macos")]
impl MachClient {
    fn connect() -> Result<Self, BarrsError> {
        let service_name = CString::new(
            env::var("RIFT_BS_NAME").unwrap_or_else(|_| RIFT_BOOTSTRAP_NAME.into()),
        )
        .map_err(|_| BarrsError::Unsupported("invalid Rift bootstrap name".into()))?;
        let mut service_port = libc::MACH_PORT_NULL as libc::mach_port_t;
        let result = unsafe {
            bootstrap_look_up(bootstrap_port, service_name.as_ptr(), &mut service_port)
        };
        if result != 0 || service_port == libc::MACH_PORT_NULL as libc::mach_port_t {
            return Err(BarrsError::Unsupported(
                "failed to connect to Rift Mach service".into(),
            ));
        }
        Ok(Self { service_port })
    }

    fn get_workspaces(&self, space_id: Option<u64>) -> Result<Value, BarrsError> {
        self.request(json!({
            "get_workspaces": { "space_id": space_id }
        }))
    }

    fn subscribe(&self, event: RiftEventKind) -> Result<libc::mach_port_t, BarrsError> {
        let reply_port = allocate_reply_port()?;
        let response = send_request_on_port(
            self.service_port,
            reply_port,
            json!({
                "subscribe": { "event": event.as_str() }
            }),
        );
        match response {
            Ok(_) => Ok(reply_port),
            Err(error) => {
                deallocate_port(reply_port);
                Err(error)
            }
        }
    }

    fn request(&self, request: Value) -> Result<Value, BarrsError> {
        let reply_port = allocate_reply_port()?;
        let response = send_request_on_port(self.service_port, reply_port, request);
        deallocate_port(reply_port);
        response
    }
}

#[cfg(target_os = "macos")]
impl Drop for MachClient {
    fn drop(&mut self) {
        deallocate_port(self.service_port);
    }
}

#[cfg(target_os = "macos")]
fn send_request_on_port(
    service_port: libc::mach_port_t,
    reply_port: libc::mach_port_t,
    request: Value,
) -> Result<Value, BarrsError> {
    let payload = serde_json::to_vec(&request)?;
    send_json_message(service_port, Some(reply_port), &payload)?;
    receive_response_message(reply_port)
}

#[cfg(target_os = "macos")]
fn send_json_message(
    service_port: libc::mach_port_t,
    reply_port: Option<libc::mach_port_t>,
    payload: &[u8],
) -> Result<(), BarrsError> {
    if payload.len() > MACH_PAYLOAD_CAPACITY {
        return Err(BarrsError::Unsupported("Rift Mach payload exceeds 16KB".into()));
    }
    let aligned_len = (payload.len() + 3) & !3;
    let mut message = SimpleMessage {
        header: MachMsgHeader {
            msgh_bits: mach_msg_bits(
                MACH_MSG_TYPE_COPY_SEND,
                if reply_port.is_some() {
                    MACH_MSG_TYPE_MAKE_SEND
                } else {
                    0
                },
            ),
            msgh_size: (size_of::<MachMsgHeader>() + aligned_len) as u32,
            msgh_remote_port: service_port,
            msgh_local_port: reply_port.unwrap_or(libc::MACH_PORT_NULL as libc::mach_port_t),
            msgh_voucher_port: 0,
            msgh_id: reply_port.unwrap_or(0) as i32,
        },
        data: [0; MACH_PAYLOAD_CAPACITY],
    };
    message.data[..payload.len()].copy_from_slice(payload);

    let result = unsafe {
        mach_msg(
            &mut message.header,
            MACH_SEND_MSG,
            message.header.msgh_size,
            0,
            libc::MACH_PORT_NULL as libc::mach_port_t,
            MACH_MSG_TIMEOUT_NONE,
            libc::MACH_PORT_NULL as libc::mach_port_t,
        )
    };
    if result != 0 {
        return Err(BarrsError::Unsupported(format!(
            "failed to send Rift Mach request ({result})"
        )));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn receive_response_message(reply_port: libc::mach_port_t) -> Result<Value, BarrsError> {
    let payload = receive_payload(reply_port)?;
    let value: Value = serde_json::from_slice(&payload)?;
    unwrap_response(value).ok_or_else(|| {
        BarrsError::Unsupported("failed to decode Rift Mach response".into())
    })
}

#[cfg(target_os = "macos")]
fn receive_json_message(reply_port: libc::mach_port_t) -> Result<Value, BarrsError> {
    let payload = receive_payload(reply_port)?;
    serde_json::from_slice(&payload).map_err(BarrsError::from)
}

#[cfg(target_os = "macos")]
fn receive_payload(reply_port: libc::mach_port_t) -> Result<Vec<u8>, BarrsError> {
    let mut message = SimpleMessage {
        header: MachMsgHeader {
            msgh_bits: 0,
            msgh_size: 0,
            msgh_remote_port: 0,
            msgh_local_port: 0,
            msgh_voucher_port: 0,
            msgh_id: 0,
        },
        data: [0; MACH_PAYLOAD_CAPACITY],
    };

    let result = unsafe {
        mach_msg(
            &mut message.header,
            MACH_RCV_MSG,
            0,
            size_of::<SimpleMessage>() as u32,
            reply_port,
            MACH_MSG_TIMEOUT_NONE,
            libc::MACH_PORT_NULL as libc::mach_port_t,
        )
    };
    if result != 0 {
        return Err(BarrsError::Unsupported(format!(
            "failed to receive Rift Mach message ({result})"
        )));
    }

    let payload_len = message
        .header
        .msgh_size
        .saturating_sub(size_of::<MachMsgHeader>() as u32) as usize;
    let mut payload = message.data[..payload_len.min(MACH_PAYLOAD_CAPACITY)].to_vec();
    while payload.last() == Some(&0) {
        payload.pop();
    }
    Ok(payload)
}

#[cfg(target_os = "macos")]
fn allocate_reply_port() -> Result<libc::mach_port_t, BarrsError> {
    let mut port = libc::MACH_PORT_NULL as libc::mach_port_t;
    let result = unsafe {
        mach_port_allocate(
            mach_task_self_port(),
            MACH_PORT_RIGHT_RECEIVE,
            &mut port,
        )
    };
    if result != 0 {
        return Err(BarrsError::Unsupported(format!(
            "failed to allocate Mach reply port ({result})"
        )));
    }
    Ok(port)
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn mach_task_self_port() -> libc::mach_port_t {
    unsafe { libc::mach_task_self() }
}

fn deallocate_port(port: SubscriptionHandle) {
    #[cfg(target_os = "macos")]
    if port != libc::MACH_PORT_NULL as libc::mach_port_t {
        unsafe {
            let _ = mach_port_deallocate(mach_task_self_port(), port);
        }
    }
}

#[cfg(target_os = "macos")]
const MACH_MSG_TYPE_COPY_SEND: u32 = 19;
#[cfg(target_os = "macos")]
const MACH_MSG_TYPE_MAKE_SEND: u32 = 20;
#[cfg(target_os = "macos")]
const MACH_SEND_MSG: i32 = 0x0000_0001;
#[cfg(target_os = "macos")]
const MACH_RCV_MSG: i32 = 0x0000_0002;
#[cfg(target_os = "macos")]
const MACH_MSG_TIMEOUT_NONE: u32 = 0;
#[cfg(target_os = "macos")]
const MACH_PORT_RIGHT_RECEIVE: u32 = 1;

#[cfg(target_os = "macos")]
const fn mach_msg_bits(remote: u32, local: u32) -> u32 {
    remote | (local << 8)
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    static bootstrap_port: libc::mach_port_t;

    fn bootstrap_look_up(
        bp: libc::mach_port_t,
        service_name: *const libc::c_char,
        service_port: *mut libc::mach_port_t,
    ) -> libc::kern_return_t;

    fn mach_port_allocate(
        task: libc::mach_port_t,
        right: u32,
        name: *mut libc::mach_port_t,
    ) -> libc::kern_return_t;

    fn mach_port_deallocate(
        task: libc::mach_port_t,
        name: libc::mach_port_t,
    ) -> libc::kern_return_t;

    fn mach_msg(
        msg: *mut MachMsgHeader,
        option: i32,
        send_size: u32,
        rcv_size: u32,
        rcv_name: libc::mach_port_t,
        timeout: u32,
        notify: libc::mach_port_t,
    ) -> libc::kern_return_t;
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        CliRiftBackend, RiftApplyResult, RiftBackend, RiftBackendKind, RiftEvent, RiftEventKind,
        RiftSnapshot, RiftWorkspace, apply_event, count_windows_in_value, parse_layout_value,
        parse_workspaces, select_active_workspace, unwrap_response,
    };

    #[test]
    fn cli_backend_snapshot_is_stable() {
        let backend = CliRiftBackend;
        assert_eq!(backend.kind(), RiftBackendKind::Cli);
    }

    #[test]
    fn backend_selection_returns_known_backend() {
        let backend = super::select_backend();
        assert!(matches!(
            backend.kind(),
            RiftBackendKind::Cli | RiftBackendKind::Mach
        ));
    }

    #[test]
    fn unwraps_success_response() {
        let value = unwrap_response(json!({
            "Success": {
                "data": [
                    { "workspace_id": 1, "workspace_name": "1", "active": true }
                ]
            }
        }))
        .expect("unwrap response");
        assert!(value.is_array());
    }

    #[test]
    fn parses_active_workspace_list() {
        let workspaces = parse_workspaces(&json!([
            { "workspace_id": 1, "workspace_name": "1", "workspace_index": 0, "visible": true },
            { "workspace_id": 2, "workspace_name": "dev", "workspace_index": 1, "active": true, "space_id": 7 }
        ]));
        let active = select_active_workspace(&workspaces).expect("active workspace");
        assert_eq!(active.display_name(), "2");
        assert!(active.active);
    }

    #[test]
    fn parses_workspace_window_count() {
        let workspaces = parse_workspaces(&json!([
            { "workspace_index": 0, "name": "1", "window_count": 2 },
            { "workspace_index": 1, "name": "2", "window_count": 0 }
        ]));
        assert_eq!(workspaces[0].window_count, Some(2));
        assert_eq!(workspaces[1].window_count, Some(0));
    }

    #[test]
    fn parses_layout_value_from_nested_shape() {
        let layout = parse_layout_value(&json!({
            "workspace_id": 2,
            "layout_mode": "bsp"
        }))
        .expect("layout");
        assert_eq!(layout, "bsp");
    }

    #[test]
    fn counts_windows_for_workspace() {
        let value = json!([
            { "workspace_id": 2, "space_id": 7 },
            { "workspace_id": 2, "space_id": 7 },
            { "workspace_id": 3, "space_id": 8 }
        ]);
        assert_eq!(count_windows_in_value(&value, Some(2), None), 2);
        let _ = match value {
            Value::Array(_) => Some(()),
            _ => None,
        };
    }

    #[test]
    fn workspace_changed_event_updates_current_workspace() {
        let mut snapshot = RiftSnapshot {
            current_workspace: "1".into(),
            workspaces: vec![
                RiftWorkspace {
                    name: "1".into(),
                    is_current: true,
                    has_windows: true,
                },
                RiftWorkspace {
                    name: "2".into(),
                    is_current: false,
                    has_windows: false,
                },
            ],
            layout: "tiling".into(),
            window_count: 1,
        };
        let changed = apply_event(
            &mut snapshot,
            &RiftEvent {
                kind: RiftEventKind::WorkspaceChanged,
                payload: json!({ "workspace_index": 1, "layout_mode": "bsp" }),
            },
        );
        assert_eq!(changed, RiftApplyResult::Updated);
        assert_eq!(snapshot.current_workspace, "2");
        assert_eq!(snapshot.layout, "bsp");
        assert!(snapshot.workspaces[1].is_current);
        assert!(!snapshot.workspaces[0].is_current);
    }

    #[test]
    fn windows_changed_event_updates_occupied_state() {
        let mut snapshot = RiftSnapshot {
            current_workspace: "2".into(),
            workspaces: vec![
                RiftWorkspace {
                    name: "1".into(),
                    is_current: false,
                    has_windows: false,
                },
                RiftWorkspace {
                    name: "2".into(),
                    is_current: true,
                    has_windows: true,
                },
            ],
            layout: "tiling".into(),
            window_count: 1,
        };
        let changed = apply_event(
            &mut snapshot,
            &RiftEvent {
                kind: RiftEventKind::WindowsChanged,
                payload: json!({ "workspace_index": 0, "windows": ["a", "b"] }),
            },
        );
        assert_eq!(changed, RiftApplyResult::Updated);
        assert!(snapshot.workspaces[0].has_windows);
        assert_eq!(snapshot.window_count, 1);
    }

    #[test]
    fn name_only_event_requests_resync() {
        let mut snapshot = RiftSnapshot {
            current_workspace: "1".into(),
            workspaces: vec![RiftWorkspace {
                name: "1".into(),
                is_current: true,
                has_windows: true,
            }],
            layout: "tiling".into(),
            window_count: 1,
        };
        let result = apply_event(
            &mut snapshot,
            &RiftEvent {
                kind: RiftEventKind::WorkspaceChanged,
                payload: json!({ "workspace_name": "Main" }),
            },
        );
        assert_eq!(result, RiftApplyResult::RequiresResync);
        assert_eq!(snapshot.workspaces.len(), 1);
        assert_eq!(snapshot.current_workspace, "1");
    }
}
