use serde::{Deserialize, Serialize};

use crate::cli::projection::{cli_params, Field};
use crate::cli::wait::bindings;

/// Who decides what happens when a wait fires, resolved when it fires: `self` is the waiter,
/// `parent` its parent agent (the user when it has none), and `user` flags the waiter's agent
/// info with `wait_needs_user`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WaitDecider {
    #[serde(rename = "self")]
    Waiter,
    Parent,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WaitState {
    /// Waiting for its source or its deadline.
    Live,
    /// Its deadline passed, or its source failed; the decider was told.
    Fired,
    /// Its source reached the state it waited for.
    Satisfied,
    Cancelled,
}

/// Which waits `wait.list` returns.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WaitListState {
    #[default]
    Live,
    Fired,
    Satisfied,
    Cancelled,
    All,
}

/// What a wait watches. `kind` is open: a server refuses one it does not list in `wait_sources`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WaitSourceInfo {
    pub kind: String,
    pub target: String,
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct WaitDeclareParams => WaitDeclareProjection {
        /// The agent that waits: agent id, agent name, or pane id. A pane without an agent gets an agent id.
        pub waiter: String => Field::new(String::new()).input(bindings::waiter_position()),
        /// What it waits for, 1 to 200 characters.
        pub summary: String => Field::new(String::new()).input(bindings::summary()),
        /// Unix time in milliseconds. Give this or `deadline_in_ms`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub deadline_at_ms: Option<u64> => Field::new(None::<u64>),
        /// Milliseconds from now. Give this or `deadline_at_ms`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub deadline_in_ms: Option<u64> => Field::new(None::<u64>),
        /// Required, unless `rearm` names a wait to inherit it from.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub then: Option<WaitDecider> => Field::new(None).input(bindings::then()),
        /// What it watches. `node` watches another agent, named by agent id, agent name or pane
        /// id: reaching `until` satisfies the wait, and any other terminal task status or a close fires it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub on: Option<WaitSourceInfo> => Field::new(None).input(bindings::on()),
        /// The source state that satisfies the wait, interpreted per source kind. A `node` wait takes
        /// a task status: `complete` (the default), `failed`, `closed`, `needs_parent` or `needs_user`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub until: Option<String> => Field::new(None).input(bindings::until()),
        /// A fired wait of the same waiter to re-arm: the new wait inherits its extensions and `then`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub rearm: Option<u64> => Field::new(None).input(bindings::rearm()),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct WaitListParams => WaitListProjection {
        /// Only waits of this agent id, agent name, or pane id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub waiter: Option<String> => Field::new(None).input(bindings::waiter_filter()),
        #[serde(default)]
        pub state: WaitListState => Field::new(WaitListState::Live).input(bindings::state()).input(bindings::all()),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct WaitTarget => WaitTargetProjection {
        pub wait_id: u64 => Field::new(0).input(bindings::wait_id()),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct WaitExtendParams => WaitExtendProjection {
        pub wait_id: u64 => Field::new(0).input(bindings::wait_id()),
        /// Unix time in milliseconds. Give this or `deadline_in_ms`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub deadline_at_ms: Option<u64> => Field::new(None::<u64>),
        /// Milliseconds from now. Give this or `deadline_at_ms`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub deadline_in_ms: Option<u64> => Field::new(None::<u64>),
        /// Who asks: agent id, agent name, or pane id. Omit, or name a pane without an agent, to ask as the user.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub by: Option<String> => Field::new(None).input(bindings::by()).input(bindings::user()),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct WaitCancelParams => WaitCancelProjection {
        pub wait_id: u64 => Field::new(0).input(bindings::wait_id()),
        /// Who asks: agent id, agent name, or pane id. Omit, or name a pane without an agent, to ask as the user.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub by: Option<String> => Field::new(None).input(bindings::by()).input(bindings::user()),
    }
}

/// What a fired wait did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WaitFiredInfo {
    pub reason: String,
    /// Agent ids messaged, and `user` when the user was flagged.
    pub to: Vec<String>,
    /// Its extensions were spent, so it went one level above its decider too.
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub escalated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WaitInfo {
    pub wait_id: u64,
    pub waiter_agent_id: String,
    pub state: WaitState,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<WaitSourceInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    /// Unix time in milliseconds.
    pub deadline_at_ms: u64,
    pub then: WaitDecider,
    /// Deadline extensions used, at most 2; a re-armed wait carries its predecessor's.
    pub extensions: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rearm_of: Option<u64>,
    pub created_at_ms: u64,
    /// When it fired, was satisfied or was cancelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fired: Option<WaitFiredInfo>,
    /// It fired to the user, who has not dealt with it yet.
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub needs_user: bool,
    /// The last time a reader reached the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ok_at_ms: Option<u64>,
    /// The state the last successful read found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_state: Option<String>,
    /// Reader errors in a row.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub errors: u32,
    /// What the last failed read said, while errors run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// A wait of this agent's fired to the user, who has not dealt with it yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WaitNeedsUserInfo {
    pub wait_id: u64,
    pub reason: String,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}
