use super::agent_identity::AgentParentError;
use super::App;
use crate::app::terminal_targets::TerminalTargetError;

pub(super) enum AgentArgsError {
    Target(TerminalTargetError),
    UnsupportedKind(String),
    InvalidArgument,
}

impl App {
    /// Replaces the args a resume starts the target's agent with, for an agent
    /// whose args changed after it started or that Herdr did not start.
    pub(super) fn set_agent_args(
        &mut self,
        target: &str,
        kind: &str,
        args: Vec<String>,
    ) -> Result<crate::api::schema::PaneInfo, AgentArgsError> {
        let (ws_idx, pane_id, terminal_id) =
            self.resolve_agent_identity_target(target)
                .map_err(|err| match err {
                    AgentParentError::Target(err) => AgentArgsError::Target(err),
                    _ => AgentArgsError::Target(TerminalTargetError::NotFound {
                        target: target.to_string(),
                    }),
                })?;
        let agent = crate::detect::parse_agent_label(kind)
            .ok_or_else(|| AgentArgsError::UnsupportedKind(kind.to_string()))?;
        if args.iter().any(|arg| arg.chars().any(char::is_control)) {
            return Err(AgentArgsError::InvalidArgument);
        }
        let launch = crate::agent_resume::AgentLaunch {
            agent: crate::detect::agent_label(agent).to_string(),
            args,
        };
        let terminal = self.state.terminals.get_mut(&terminal_id).ok_or_else(|| {
            AgentArgsError::Target(TerminalTargetError::NotFound {
                target: target.to_string(),
            })
        })?;
        if terminal.agent_launch.as_ref() != Some(&launch) {
            terminal.agent_launch = Some(launch);
            self.state.mark_session_dirty();
            self.schedule_session_save();
            self.emit_pane_updated(ws_idx, pane_id);
        }
        self.pane_info(ws_idx, pane_id).ok_or_else(|| {
            AgentArgsError::Target(TerminalTargetError::NotFound {
                target: target.to_string(),
            })
        })
    }

    pub(super) fn agent_args_error_body(
        &self,
        err: AgentArgsError,
    ) -> crate::api::schema::ErrorBody {
        match err {
            AgentArgsError::Target(err) => self.agent_target_error_body(err),
            AgentArgsError::UnsupportedKind(kind) => crate::api::schema::ErrorBody {
                code: "unsupported_agent_kind".into(),
                message: format!("unsupported interactive agent kind {kind}"),
            },
            AgentArgsError::InvalidArgument => crate::api::schema::ErrorBody {
                code: "invalid_agent_argument".into(),
                message: "agent arguments must not contain control characters".into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::schema::{
        AgentSetArgsParams, Method, PaneReportAgentSessionParams, PaneTarget, Request,
    };
    use crate::app::App;
    use crate::layout::PaneId;

    struct Fixture {
        app: App,
        root: PaneId,
    }

    fn fixture() -> Fixture {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        let workspace = crate::workspace::Workspace::test_new("launch");
        let root = workspace.tabs[0].root_pane;
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        Fixture { app, root }
    }

    impl Fixture {
        fn call(&mut self, method: Method) -> serde_json::Value {
            let response = self.app.handle_api_request(Request {
                id: "t".into(),
                method,
            });
            serde_json::from_str(&response).unwrap()
        }

        fn public(&self) -> String {
            self.app.public_pane_id(0, self.root).unwrap()
        }

        fn pane(&mut self) -> serde_json::Value {
            let pane_id = self.public();
            self.call(Method::PaneGet(PaneTarget { pane_id }))["result"]["pane"].clone()
        }

        fn report_session(&mut self, source: &str, path: &str) {
            let pane_id = self.public();
            self.call(Method::PaneReportAgentSession(
                PaneReportAgentSessionParams {
                    pane_id,
                    source: source.into(),
                    agent: "claude".into(),
                    seq: None,
                    agent_session_id: Some("abc".into()),
                    agent_session_path: Some(path.into()),
                    session_start_source: Some("startup".into()),
                    resume_argv: None,
                },
            ));
        }
    }

    #[tokio::test]
    async fn agent_start_records_its_args_for_the_resume() {
        let mut f = fixture();
        let terminal_id = f.app.state.terminal_id_for_pane(0, f.root).unwrap();
        let (runtime, _receiver) =
            crate::terminal::TerminalRuntime::test_with_channel_capacity(80, 24, 4);
        f.app.terminal_runtimes.insert(terminal_id, runtime);

        let response = f.call(Method::AgentStart(crate::api::schema::AgentStartParams {
            name: "worker".into(),
            kind: "claude".into(),
            pane_id: f.public(),
            args: vec!["--model".into(), "opus".into()],
            parent: None,
            timeout_ms: Some(4_000),
        }));

        assert_eq!(
            response["result"]["agent"]["launch_args"],
            serde_json::json!(["--model", "opus"])
        );
    }

    #[test]
    fn set_args_replaces_the_args_and_refuses_an_unknown_kind() {
        let mut f = fixture();
        let target = f.public();
        let set = |kind: &str, args: &[&str]| {
            Method::AgentSetArgs(AgentSetArgsParams {
                target: target.clone(),
                kind: kind.into(),
                args: args.iter().map(|arg| arg.to_string()).collect(),
            })
        };

        let refused = f.call(set("nonsense", &[]));
        assert_eq!(refused["error"]["code"], "unsupported_agent_kind");

        let response = f.call(set("codex", &["-c", "model=o4"]));
        assert_eq!(
            response["result"]["pane"]["launch_args"],
            serde_json::json!(["-c", "model=o4"])
        );
        assert!(f.app.state.session_dirty);
    }

    #[test]
    fn an_official_session_report_records_the_profile_directory() {
        let mut f = fixture();
        let home = std::env::temp_dir().join("herdr-launch-profile");
        let transcript = home.join("projects/-repo/abc.jsonl");

        f.report_session("custom:claude", &transcript.display().to_string());
        assert!(f.pane()["agent_home"].is_null());

        f.report_session("herdr:claude", &transcript.display().to_string());
        assert_eq!(f.pane()["agent_home"], home.display().to_string());
    }
}
