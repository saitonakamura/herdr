use bytes::Bytes;

use super::App;

pub(super) enum RestoreCommandError {
    PaneNotFound,
    Invalid(&'static str),
    Send { code: &'static str, message: String },
}

impl App {
    /// Sets or clears the shell command a restart types into the pane's fresh
    /// shell, and with `run` also types it now.
    pub(super) fn set_pane_restore_command(
        &mut self,
        pane_id: &str,
        command: Option<String>,
        run: bool,
    ) -> Result<crate::api::schema::PaneInfo, RestoreCommandError> {
        let (ws_idx, pane_id) = self
            .parse_pane_id(pane_id)
            .ok_or(RestoreCommandError::PaneNotFound)?;
        let terminal_id = self
            .state
            .terminal_id_for_pane(ws_idx, pane_id)
            .ok_or(RestoreCommandError::PaneNotFound)?;
        let command = command.filter(|command| !command.trim().is_empty());
        // A control character would submit part of the command early when it is typed.
        if command
            .as_deref()
            .is_some_and(|command| command.chars().any(char::is_control))
        {
            return Err(RestoreCommandError::Invalid(
                "restore command must be a single line without control characters",
            ));
        }
        if run {
            let command = command
                .as_deref()
                .ok_or(RestoreCommandError::Invalid("run needs a command"))?;
            let runtime = self
                .lookup_runtime_sender(ws_idx, pane_id)
                .ok_or(RestoreCommandError::PaneNotFound)?;
            let bytes =
                super::api_helpers::encode_api_input(runtime, command, &[String::from("Enter")])
                    .map_err(|key| RestoreCommandError::Send {
                        code: "invalid_key",
                        message: format!("unsupported key {key}"),
                    })?;
            let send_failed = |err: String| RestoreCommandError::Send {
                code: "pane_send_failed",
                message: err,
            };
            runtime
                .validate_input_submission(&bytes)
                .map_err(|err| send_failed(err.to_string()))?;
            runtime
                .try_send_bytes(Bytes::from(bytes))
                .map_err(|err| send_failed(err.to_string()))?;
        }
        let terminal = self
            .state
            .terminals
            .get_mut(&terminal_id)
            .ok_or(RestoreCommandError::PaneNotFound)?;
        if terminal.restore_command != command {
            terminal.restore_command = command;
            self.state.mark_session_dirty();
            self.schedule_session_save();
            self.emit_pane_updated(ws_idx, pane_id);
        }
        self.pane_info(ws_idx, pane_id)
            .ok_or(RestoreCommandError::PaneNotFound)
    }
}

#[cfg(test)]
mod tests {
    use crate::api::schema::{Method, PaneSetRestoreCommandParams, Request};
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
        let workspace = crate::workspace::Workspace::test_new("restore");
        let root = workspace.tabs[0].root_pane;
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        Fixture { app, root }
    }

    impl Fixture {
        fn set(&mut self, command: Option<&str>, run: bool) -> serde_json::Value {
            let pane_id = self.app.public_pane_id(0, self.root).unwrap();
            let response = self.app.handle_api_request(Request {
                id: "t".into(),
                method: Method::PaneSetRestoreCommand(PaneSetRestoreCommandParams {
                    pane_id,
                    command: command.map(str::to_string),
                    run,
                }),
            });
            serde_json::from_str(&response).unwrap()
        }

        fn stored(&self) -> Option<String> {
            let terminal_id = self.app.state.terminal_id_for_pane(0, self.root).unwrap();
            self.app.state.terminals[&terminal_id]
                .restore_command
                .clone()
        }
    }

    #[test]
    fn sets_and_clears_the_restore_command() {
        let mut f = fixture();

        let response = f.set(Some("npm run dev"), false);
        assert_eq!(response["result"]["pane"]["restore_command"], "npm run dev");
        assert_eq!(f.stored().as_deref(), Some("npm run dev"));
        assert!(f.app.state.session_dirty);

        let response = f.set(None, false);
        assert!(response["result"]["pane"]["restore_command"].is_null());
        assert_eq!(f.stored(), None);
    }

    #[test]
    fn refuses_a_multi_line_command_and_run_without_one() {
        let mut f = fixture();

        let refused = f.set(Some("make\nrm -rf build"), false);
        assert_eq!(refused["error"]["code"], "invalid_restore_command");
        let refused = f.set(Some("   "), true);
        assert_eq!(refused["error"]["code"], "invalid_restore_command");
        assert_eq!(f.stored(), None);
    }

    #[tokio::test]
    async fn run_types_the_command_into_the_pane() {
        let mut f = fixture();
        let terminal_id = f.app.state.terminal_id_for_pane(0, f.root).unwrap();
        let (runtime, mut receiver) =
            crate::terminal::TerminalRuntime::test_with_channel_capacity(80, 24, 4);
        f.app.terminal_runtimes.insert(terminal_id, runtime);

        let response = f.set(Some("tail -f log"), true);

        assert_eq!(response["result"]["pane"]["restore_command"], "tail -f log");
        let typed = receiver.try_recv().expect("command should be typed");
        assert!(String::from_utf8_lossy(&typed).starts_with("tail -f log"));
    }
}
