use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct PingParams {}

crate::cli::projection::cli_params! {
    #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct ServerLiveHandoffParams => ServerLiveHandoffProjection {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub import_exe: Option<String> => crate::cli::projection::Field::new(None).input(
            crate::cli::projection::Input::value("import-exe", "path", |slot, value| {
                *slot = Some(value.to_owned());
                Ok(())
            }).path()
        ),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub expected_protocol: Option<u32> => crate::cli::projection::Field::new(None).input(
            crate::cli::projection::Input::value("expected-protocol", "n", |slot, value| {
                *slot = Some(value.parse::<u32>().map_err(|_| {
                    crate::cli::projection::ParseError::Usage(format!("invalid expected protocol: {value}"))
                })?);
                Ok(())
            })
        ),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub expected_version: Option<String> => crate::cli::projection::Field::new(None).input(
            crate::cli::projection::Input::value("expected-version", "version", |slot, value| {
                *slot = Some(value.to_owned());
                Ok(())
            })
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ServerSshAgentRegisterParams {
    /// Absolute remote-host agent socket. Registration lasts until this API connection closes.
    pub socket_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ServerCapabilities {
    pub live_handoff: bool,
    #[serde(default)]
    pub detached_server_daemon: bool,
    /// Stable client-owned endpoint generation supported by this server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_protocol_generation: Option<u32>,
    /// Whether this server supports explicit client-shell surface interest.
    #[serde(default)]
    pub surface_interest: bool,
    /// Whether this server supports endpoint health probes.
    #[serde(default)]
    pub health_check: bool,
    /// Supports connection-scoped `server.ssh_agent.register` on the local JSON API.
    #[serde(default)]
    pub ssh_agent_registration: bool,
    /// Runs `[worktrees] post_create` in each new checkout before opening it.
    #[serde(default)]
    pub worktree_post_create: bool,
    /// Opens worktrees as tabs in a workspace and removes them by tab.
    #[serde(default)]
    pub worktree_tabs: bool,
    /// Resumes a restored agent with its `agent start` args, its pane's `--env`
    /// and its session's profile directory.
    #[serde(default)]
    pub agent_launch_resume: bool,
    /// Keeps a task status per agent (`agent.set_task_status`) and closes subtrees (`agent.close`).
    #[serde(default)]
    pub agent_task_status: bool,
    /// Keeps annotations per agent (`agent.annotate`).
    #[serde(default)]
    pub agent_annotations: bool,
}
