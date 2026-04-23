use std::io;

#[derive(Debug, thiserror::Error)]
pub enum BarrsError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("lua error: {0}")]
    Lua(String),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("daemon is not running")]
    DaemonUnavailable,
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("unsupported operation: {0}")]
    Unsupported(String),
}

impl From<mlua::Error> for BarrsError {
    fn from(value: mlua::Error) -> Self {
        Self::Lua(value.to_string())
    }
}
