//! The slice of herdr's socket API the sidebar reads. Field names follow herdr's
//! schema types (`src/api/schema/`); `schema_tests` checks them against the
//! generated `docs/next/api/herdr-api.schema.json`, so a rename fails `just check`.

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct SessionSnapshot {
    #[serde(default)]
    pub focused_workspace_id: Option<String>,
    #[serde(default)]
    pub focused_tab_id: Option<String>,
    #[serde(default)]
    pub focused_pane_id: Option<String>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub tabs: Vec<TabInfo>,
    pub panes: Vec<PaneInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct WorkspaceInfo {
    pub workspace_id: String,
    pub label: String,
    pub focused: bool,
    #[serde(default)]
    pub tokens: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct TabInfo {
    pub tab_id: String,
    pub workspace_id: String,
    pub label: String,
    pub focused: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct PaneInfo {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub focused: bool,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub foreground_cwd: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub terminal_title_stripped: Option<String>,
    pub agent_status: AgentStatus,
    #[serde(default)]
    pub blocked_reason: Option<BlockedReason>,
    #[serde(default)]
    pub tokens: HashMap<String, String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub parent_agent_id: Option<String>,
    #[serde(default)]
    pub task_status: Option<TaskStatus>,
    #[serde(default)]
    pub annotations: BTreeMap<String, String>,
    #[serde(default)]
    pub repo: Option<PaneRepoInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct PaneRepoInfo {
    pub root: String,
    pub repo_name: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub ahead: Option<usize>,
    #[serde(default)]
    pub behind: Option<usize>,
    #[serde(default)]
    pub dirty: Option<usize>,
    #[serde(default)]
    pub is_linked_worktree: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct MessageInfo {
    pub message_id: u64,
    pub kind: MessageKind,
    #[serde(default)]
    pub from_agent_id: Option<String>,
    pub to_agent_id: String,
}

// Readers tolerate values a newer server adds: they decode as `Unknown`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Working,
    Awaiting,
    Blocked,
    Done,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum BlockedReason {
    Question,
    Permission,
    Form,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Starting,
    Running,
    NeedsParent,
    NeedsUser,
    Complete,
    Failed,
    Closed,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Report,
    Reply,
    Tell,
    #[serde(other)]
    Unknown,
}

/// The lifecycle events the sidebar redraws on. It treats each as a reason to
/// re-read, never as state to apply, as herdr's socket API page advises.
pub const SUBSCRIPTIONS: &[&str] = &[
    "workspace.created",
    "workspace.updated",
    "workspace.metadata_updated",
    "workspace.renamed",
    "workspace.moved",
    "workspace.reordered",
    "workspace.closed",
    "workspace.focused",
    "worktree.created",
    "worktree.opened",
    "worktree.removed",
    "tab.created",
    "tab.closed",
    "tab.focused",
    "tab.renamed",
    "tab.moved",
    "pane.created",
    "pane.closed",
    "pane.updated",
    "pane.focused",
    "pane.moved",
    "pane.exited",
    "pane.agent_detected",
    "message.queued",
    "message.delivered",
];

/// herdr has no session-wide state event: `pane.agent_status_changed` takes one
/// pane per entry, so the sidebar keeps a second subscription over its agent panes.
pub fn status_subscriptions(pane_ids: &[String]) -> Vec<serde_json::Value> {
    pane_ids
        .iter()
        .map(|pane| serde_json::json!({ "type": "pane.agent_status_changed", "pane_id": pane }))
        .collect()
}

pub fn lifecycle_subscriptions() -> Vec<serde_json::Value> {
    SUBSCRIPTIONS
        .iter()
        .map(|kind| serde_json::json!({ "type": kind }))
        .collect()
}

/// An event line as the sidebar needs it: its name, and the pane it names, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventLine {
    pub event: String,
    pub pane_id: Option<String>,
}

pub fn parse_event_line(value: &serde_json::Value) -> Option<EventLine> {
    let event = value.get("event")?.as_str()?.to_string();
    let data = value.get("data");
    let pane_id = data
        .and_then(|data| {
            data.get("pane_id")
                .or_else(|| data.get("pane")?.get("pane_id"))
        })
        .and_then(|id| id.as_str())
        .map(str::to_string);
    Some(EventLine { event, pane_id })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// Splits a response line into its result or its error.
pub fn parse_response(line: &str) -> Result<serde_json::Value, ApiError> {
    let value: serde_json::Value = serde_json::from_str(line).map_err(|err| ApiError {
        code: "bad_response".into(),
        message: err.to_string(),
    })?;
    if let Some(error) = value.get("error") {
        return Err(ApiError {
            code: error["code"].as_str().unwrap_or("error").to_string(),
            message: error["message"].as_str().unwrap_or_default().to_string(),
        });
    }
    value.get("result").cloned().ok_or_else(|| ApiError {
        code: "bad_response".into(),
        message: "response has neither result nor error".into(),
    })
}

pub fn snapshot_from_result(result: &serde_json::Value) -> Result<SessionSnapshot, ApiError> {
    serde_json::from_value(result.get("snapshot").cloned().unwrap_or_default()).map_err(|err| {
        ApiError {
            code: "bad_snapshot".into(),
            message: err.to_string(),
        }
    })
}

pub fn messages_from_result(result: &serde_json::Value) -> Vec<MessageInfo> {
    result
        .get("messages")
        .and_then(|messages| messages.as_array())
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| serde_json::from_value(message.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(unix)]
pub use unix::Client;

#[cfg(unix)]
mod unix {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use super::{parse_response, ApiError};

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    #[derive(Debug, Clone)]
    pub struct Client {
        pub socket: PathBuf,
    }

    fn io_error(err: std::io::Error) -> ApiError {
        ApiError {
            code: "socket".into(),
            message: err.to_string(),
        }
    }

    fn request_line(method: &str, params: serde_json::Value) -> (String, String) {
        let id = format!("sidebar_{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
        let line = serde_json::json!({ "id": id, "method": method, "params": params });
        (id, format!("{line}\n"))
    }

    impl Client {
        /// One request on its own connection.
        pub fn call(
            &self,
            method: &str,
            params: serde_json::Value,
        ) -> Result<serde_json::Value, ApiError> {
            let mut stream = UnixStream::connect(&self.socket).map_err(io_error)?;
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .map_err(io_error)?;
            let (_, line) = request_line(method, params);
            stream.write_all(line.as_bytes()).map_err(io_error)?;
            let mut reader = BufReader::new(stream);
            let mut response = String::new();
            reader.read_line(&mut response).map_err(io_error)?;
            parse_response(&response)
        }

        /// Opens `events.subscribe` and calls `on_line` with each pushed event until
        /// the connection ends. The error is why it ended (`events_lost` included).
        /// `on_connect` gets a handle that can shut the connection down from elsewhere.
        pub fn subscribe(
            &self,
            subscriptions: Vec<serde_json::Value>,
            on_connect: impl FnOnce(&UnixStream),
            mut on_started: impl FnMut(),
            mut on_line: impl FnMut(serde_json::Value),
        ) -> ApiError {
            let mut stream = match UnixStream::connect(&self.socket) {
                Ok(stream) => stream,
                Err(err) => return io_error(err),
            };
            on_connect(&stream);
            let (id, line) = request_line(
                "events.subscribe",
                serde_json::json!({ "subscriptions": subscriptions }),
            );
            if let Err(err) = stream.write_all(line.as_bytes()) {
                return io_error(err);
            }
            let mut reader = BufReader::new(stream);
            let mut started = false;
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        return ApiError {
                            code: "closed".into(),
                            message: "subscription closed".into(),
                        }
                    }
                    Ok(_) => {}
                    Err(err) => return io_error(err),
                }
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                    continue;
                };
                if value.get("id").and_then(|v| v.as_str()) == Some(id.as_str()) {
                    if let Err(err) = parse_response(&line) {
                        return err;
                    }
                    if !started {
                        started = true;
                        on_started();
                    }
                    continue;
                }
                on_line(value);
            }
        }
    }
}
