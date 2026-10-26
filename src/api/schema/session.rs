use serde::{Deserialize, Serialize};

use super::agents::AgentInfo;
use super::panes::{PaneInfo, PaneLayoutSnapshot};
use super::tabs::TabInfo;
use super::workspaces::WorkspaceInfo;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionSnapshot {
    pub version: String,
    pub protocol: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_id: Option<String>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub tabs: Vec<TabInfo>,
    pub panes: Vec<PaneInfo>,
    pub layouts: Vec<PaneLayoutSnapshot>,
    pub agents: Vec<AgentInfo>,
}

/// The server's one session pane: a terminal that belongs to no workspace or tab,
/// running the program configured as `ui.sidebar.pane`. Clients that support it
/// draw it in place of their native sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionPaneInfo {
    /// `off`, `running`, `restarting` or `failed`. Readers should treat unknown values as `off`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argv: Option<Vec<String>>,
    /// Whether the program holds keyboard focus. Focus is shared by every client.
    pub focused: bool,
    pub cols: u16,
    pub rows: u16,
    /// Expanded width: the one `session_pane.set_width` set, else `ui.sidebar_width`,
    /// clamped by the sidebar bounds.
    pub width: u16,
    pub collapsed_width: u16,
    pub restarts: u64,
}

/// Sets the session pane's expanded width for every client. The server clamps it
/// by the sidebar bounds and keeps it across restarts; `null` goes back to
/// `ui.sidebar_width`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct SessionPaneSetWidthParams {
    #[serde(default)]
    pub width: Option<u16>,
}
