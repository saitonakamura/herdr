use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::common::{AgentStatus, ReadFormat, ReadSource};
use crate::cli::agent::bindings;
use crate::cli::projection::{cli_params, Field};

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct AgentReadParams => AgentReadProjection {
        pub target: String => Field::new(String::new()).input(bindings::target()),
        pub source: ReadSource => Field::new(ReadSource::Recent).input(bindings::source()),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub lines: Option<u32> => Field::new(None).input(bindings::lines()),
        #[serde(default)]
        pub format: ReadFormat => Field::new(ReadFormat::Text)
            .input(bindings::format()).input(bindings::ansi()),
        #[serde(default = "super::common::default_true")]
        pub strip_ansi: bool => Field::new(true),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct AgentSendKeysParams => AgentSendKeysProjection {
        pub target: String => Field::new(String::new()).input(bindings::target()),
        pub keys: Vec<String> => Field::new(Vec::new()).input(bindings::keys()),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct AgentWaitParams => AgentWaitProjection {
        pub target: String => Field::new(String::new()).input(bindings::target()),
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub until: Vec<AgentStatus> => Field::new(Vec::new()).input(bindings::until()
            .help("State to match; repeat for more than one state")),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub timeout_ms: Option<u64> => Field::new(None).input(bindings::timeout()
            .help("Fail after this many milliseconds")),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct AgentPromptWaitOptions => AgentPromptWaitProjection {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub until: Vec<AgentStatus> => Field::new(Vec::new()).input(bindings::until()
            .help("State to match after --wait; repeat for more than one state")),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub timeout_ms: Option<u64> => Field::new(None).input(bindings::timeout()
            .help("Fail after this many milliseconds")),
        #[serde(skip)]
        #[schemars(skip)]
        pub(crate) submission_deadline: Option<std::time::Instant> => Field::new(None::<std::time::Instant>),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct AgentRenameParams => AgentRenameProjection {
        pub target: String => Field::new(String::new()).input(bindings::target()),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub name: Option<String> => Field::new(None)
            .input(bindings::rename_name()).input(bindings::clear_name()),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct AgentSetParentParams => AgentSetParentProjection {
        /// Agent id, agent name, or pane id. A pane without an agent gets an agent id too.
        pub target: String => Field::new(String::new()).input(bindings::target()),
        /// Parent agent id; omit to make the target top-level.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub parent: Option<String> => Field::new(None)
            .input(bindings::parent_position()).input(bindings::clear_parent()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentViewSetParams {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<AgentViewFilter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort: Vec<AgentViewSort>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct AgentViewClearParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AgentViewFilter {
    All {
        filters: Vec<AgentViewFilter>,
    },
    Any {
        filters: Vec<AgentViewFilter>,
    },
    Not {
        filter: Box<AgentViewFilter>,
    },
    Eq {
        field: AgentViewField,
        value: AgentViewValue,
    },
    In {
        field: AgentViewField,
        values: Vec<AgentViewValue>,
    },
    Exists {
        field: AgentViewField,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum AgentViewField {
    Builtin(AgentViewBuiltinField),
    Token { token: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentViewBuiltinField {
    Status,
    WorkspaceId,
    TabId,
    PaneId,
    Agent,
    Seen,
    StateChangeSeq,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum AgentViewValue {
    String(String),
    Bool(bool),
    Number(u64),
    Context { context: AgentViewContext },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentViewContext {
    CurrentWorkspaceId,
    CurrentTabId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentViewSort {
    pub field: AgentViewSortField,
    #[serde(default)]
    pub order: AgentViewSortOrder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum AgentViewSortField {
    Builtin(AgentViewBuiltinSortField),
    Token { token: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentViewBuiltinSortField {
    WorkspaceOrder,
    TabOrder,
    PaneOrder,
    Attention,
    Status,
    Agent,
    Seen,
    StateChangeSeq,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentViewSortOrder {
    #[default]
    Asc,
    Desc,
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct AgentStartParams => AgentStartProjection {
        pub name: String => Field::new(String::new()).input(bindings::text_position("name", "NAME", 0)),
        pub kind: String => Field::new(String::new()).input(bindings::kind()),
        pub pane_id: String => Field::new(String::new()).input(bindings::pane()),
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub args: Vec<String> => Field::new(Vec::new()).input(bindings::agent_args()),
        /// Agent id of the agent that starts this one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub parent: Option<String> => Field::new(None).input(bindings::parent_flag()),
        /// Startup timeout in milliseconds. Values must be greater than 3000 and at most 300000.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub timeout_ms: Option<u64> => Field::new(None).input(bindings::timeout()
            .help("Wait for interactive readiness (default: 30000; max: 300000)")),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct AgentPromptParams => AgentPromptProjection {
        pub target: String => Field::new(String::new()).input(bindings::target()),
        pub text: String => Field::new(String::new()).input(bindings::text_position("text", "TEXT", 1).verbatim()),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub wait: Option<AgentPromptWaitOptions> => Field::new(None).input(bindings::wait_presence()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentInfo {
    pub terminal_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_title_stripped: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_agent: Option<String>,
    pub agent_status: AgentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<super::common::AgentBlockedReason>,
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub screen_detection_skipped: bool,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub state_labels: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    #[schemars(schema_with = "super::common::metadata_token_values_schema")]
    pub tokens: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session: Option<AgentSessionInfo>,
    /// Stable agent identity; survives session changes, pane moves and server restarts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// The agent that started this one; absent for a top-level agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_agent_id: Option<String>,
    /// What the agent last said about its brief.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_status: Option<TaskStatus>,
    /// The Git checkout containing `cwd`; absent outside one or before its first refresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<super::panes::PaneRepoInfo>,
    pub workspace_id: String,
    pub tab_id: String,
    pub pane_id: String,
    pub focused: bool,
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub launch_pending: bool,
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub interactive_ready: bool,
    #[serde(default)]
    pub state_change_seq: u64,
    /// The current idle transition completed work, independently of who has viewed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground_cwd: Option<String>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentSessionInfo {
    pub source: String,
    pub agent: String,
    pub kind: crate::agent_resume::AgentSessionRefKind,
    pub value: String,
}

/// What an agent says about its brief, stored next to the state herdr detects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Starting,
    Running,
    NeedsParent,
    NeedsUser,
    Complete,
    Failed,
    Closed,
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct AgentSetTaskStatusParams => AgentSetTaskStatusProjection {
        /// Agent id, agent name, or pane id. A pane without an agent gets an agent id too.
        pub target: String => Field::new(String::new()).input(bindings::target()),
        pub status: TaskStatus => Field::new(TaskStatus::Running).input(bindings::task_status()),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct AgentCloseParams => AgentCloseProjection {
        /// Agent ids, agent names, or pane ids; each closes with every descendant.
        pub targets: Vec<String> => Field::new(Vec::new()).input(bindings::close_targets()),
        /// Close even while part of the subtree is working or blocked.
        #[serde(default, skip_serializing_if = "super::is_false")]
        pub force: bool => Field::new(false).input(crate::cli::projection::boolean("force", true)),
        /// Report what would close without closing anything.
        #[serde(default, skip_serializing_if = "super::is_false")]
        pub dry_run: bool => Field::new(false).input(crate::cli::projection::boolean("dry-run", true)),
        /// The caller's own pane: never counted as busy, and closed last.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub caller_pane_id: Option<String> => Field::new(None::<String>),
    }
}

/// What closing one agent of a subtree took with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentCloseScope {
    Pane,
    Tab,
    Workspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClosedAgentInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub pane_id: String,
    /// The pane, or the tab or workspace it was the last pane of.
    pub scope: AgentCloseScope,
    /// Why this agent's pane could not be closed; the rest of the subtree still closes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
