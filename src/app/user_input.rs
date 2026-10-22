use super::App;
use crate::terminal::TerminalId;

impl App {
    /// Records that a client typed into this terminal. Only client input paths
    /// call this; API writes (`pane send-*`, `agent prompt`, messages) never do.
    pub(crate) fn note_user_input(&mut self, terminal_id: &TerminalId) {
        let Some(terminal) = self.state.terminals.get_mut(terminal_id) else {
            return;
        };
        terminal.last_user_input_at_ms = Some(now_ms());
        // Keystrokes must not keep pushing back a save that is already due.
        if self.session_save_deadline.is_none() {
            self.schedule_session_save();
        }
    }

    pub(crate) fn note_user_input_in_pane(
        &mut self,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
    ) {
        if let Some(terminal_id) = self.state.terminal_id_for_pane(ws_idx, pane_id) {
            self.note_user_input(&terminal_id);
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use crate::app::App;

    fn app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![crate::workspace::Workspace::test_new("input")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app
    }

    #[test]
    fn note_user_input_shows_on_pane_info() {
        let mut app = app();
        let pane = app.state.workspaces[0].tabs[0].root_pane;
        assert_eq!(app.pane_info(0, pane).unwrap().last_user_input_at_ms, None);

        app.note_user_input_in_pane(0, pane);

        let at = app.pane_info(0, pane).unwrap().last_user_input_at_ms;
        assert!(at.is_some_and(|at| at > 0), "{at:?}");
    }
}
