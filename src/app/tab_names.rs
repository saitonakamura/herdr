use super::App;
use crate::api::schema::{EventData, EventEnvelope, EventKind};
use crate::layout::PaneId;

impl App {
    /// Re-reads the followed name of the tab holding `pane_id`. A tab with no
    /// agent, several, or an agent whose title says nothing keeps its name.
    pub(crate) fn refresh_followed_tab_name(&mut self, ws_idx: usize, pane_id: PaneId) {
        let Some(ws) = self.state.workspaces.get(ws_idx) else {
            return;
        };
        let Some(tab_idx) = ws.find_tab_index_for_pane(pane_id) else {
            return;
        };
        let tab = &ws.tabs[tab_idx];
        let mut agents = tab.panes.iter().filter(|(_, pane)| {
            self.state
                .terminals
                .get(&pane.attached_terminal_id)
                .is_some_and(|terminal| terminal.effective_agent_label().is_some())
        });
        let (Some((agent_pane, pane)), None) = (agents.next(), agents.next()) else {
            return;
        };
        let Some(title) = self
            .state
            .terminals
            .get(&pane.attached_terminal_id)
            .and_then(|terminal| terminal.terminal_title_stripped())
        else {
            return;
        };
        let cwd = tab.cwd_for_pane(*agent_pane, &self.state.terminals, &self.terminal_runtimes);
        let foreground_cwd = tab.foreground_cwd_for_pane(*agent_pane, &self.terminal_runtimes);
        let Some(name) = crate::workspace::followed_tab_name(
            Some(&title),
            cwd.as_deref(),
            foreground_cwd.as_deref(),
            self.state.tab_name_max_length,
        ) else {
            return;
        };
        if tab.followed_name.as_deref() == Some(name.as_str()) {
            return;
        }
        let pinned = tab.custom_name.is_some();
        self.state.workspaces[ws_idx].tabs[tab_idx].followed_name = Some(name.clone());
        self.schedule_session_save();
        if pinned {
            return;
        }
        let Some(tab_id) = self.public_tab_id(ws_idx, tab_idx) else {
            return;
        };
        crate::logging::tab_renamed(&self.state.workspaces[ws_idx].id, &tab_id);
        self.emit_event(EventEnvelope {
            event: EventKind::TabRenamed,
            data: EventData::TabRenamed {
                tab_id,
                workspace_id: self.public_workspace_id(ws_idx),
                label: name,
            },
        });
        self.render_dirty.request_generic();
        self.render_notify.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::api::schema::{TabLabelSource, TabRenameParams};
    use crate::config::Config;
    use crate::detect::{Agent, AgentState};
    use crate::workspace::Workspace;

    fn app() -> (App, crate::api::EventHub) {
        let event_hub = crate::api::EventHub::default();
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            api_rx,
            event_hub.clone(),
        );
        app.state.workspaces = vec![Workspace::test_new("one")];
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        (app, event_hub)
    }

    fn make_agent(app: &mut App, pane_id: PaneId) {
        let terminal_id = app.state.workspaces[0]
            .terminal_id(pane_id)
            .unwrap()
            .clone();
        app.state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        app.state.terminals.get_mut(&terminal_id).unwrap().state = AgentState::Working;
        app.terminal_runtimes.insert(
            terminal_id,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(80, 24, b""),
        );
    }

    fn set_title(app: &mut App, pane_id: PaneId, title: &str) {
        let terminal_id = app.state.workspaces[0]
            .terminal_id(pane_id)
            .unwrap()
            .clone();
        app.terminal_runtimes
            .get(&terminal_id)
            .unwrap()
            .test_process_pty_bytes(format!("\x1b]0;{title}\x07").as_bytes());
        app.sync_terminal_titles(&HashSet::from([pane_id]));
    }

    fn rename(app: &mut App, label: &str) {
        let tab_id = app.public_tab_id(0, 0).unwrap();
        app.handle_tab_rename(
            "1".into(),
            TabRenameParams {
                tab_id,
                label: label.into(),
            },
        );
    }

    fn tab_renamed_labels(event_hub: &crate::api::EventHub) -> Vec<String> {
        event_hub
            .events_after(0)
            .into_iter()
            .filter_map(|(_, event)| match event.data {
                EventData::TabRenamed { label, .. } => Some(label),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn a_tab_with_one_agent_follows_its_title() {
        let (mut app, event_hub) = app();
        let pane_id = app.state.workspaces[0].tabs[0].root_pane;
        make_agent(&mut app, pane_id);

        set_title(&mut app, pane_id, "⠋ Loom tab names follow task titles");

        let tab = app.tab_info(0, 0).unwrap();
        assert_eq!(tab.label, "loom-tab-names-follow");
        assert_eq!(tab.label_source, Some(TabLabelSource::Following));
        assert_eq!(tab_renamed_labels(&event_hub), ["loom-tab-names-follow"]);

        // a spinner frame or a bare agent name changes nothing
        set_title(&mut app, pane_id, "⠙ Loom tab names follow task titles");
        set_title(&mut app, pane_id, "Claude Code");
        assert_eq!(app.tab_info(0, 0).unwrap().label, "loom-tab-names-follow");
        assert_eq!(tab_renamed_labels(&event_hub).len(), 1);
    }

    #[tokio::test]
    async fn a_hand_rename_pins_until_cleared() {
        let (mut app, event_hub) = app();
        let pane_id = app.state.workspaces[0].tabs[0].root_pane;
        make_agent(&mut app, pane_id);
        set_title(&mut app, pane_id, "first task");

        rename(&mut app, "My Worker");
        set_title(&mut app, pane_id, "second task");
        let tab = app.tab_info(0, 0).unwrap();
        assert_eq!(tab.label, "My Worker");
        assert_eq!(tab.label_source, Some(TabLabelSource::Pinned));

        // the pin kept the title it hid, and a bare number gives it back
        rename(&mut app, "7");
        let tab = app.tab_info(0, 0).unwrap();
        assert_eq!(tab.label, "second-task");
        assert_eq!(tab.label_source, Some(TabLabelSource::Following));
        assert_eq!(
            tab_renamed_labels(&event_hub),
            ["first-task", "My Worker", "second-task"]
        );

        rename(&mut app, "pinned again");
        rename(&mut app, "");
        assert_eq!(app.tab_info(0, 0).unwrap().label, "second-task");
    }

    #[tokio::test]
    async fn a_tab_with_two_agents_keeps_its_name() {
        let (mut app, _event_hub) = app();
        let first = app.state.workspaces[0].tabs[0].root_pane;
        let second = app.state.workspaces[0].test_split(ratatui::layout::Direction::Horizontal);
        app.state.ensure_test_terminals();
        make_agent(&mut app, first);
        set_title(&mut app, first, "only agent");
        make_agent(&mut app, second);

        set_title(&mut app, first, "now one of two");

        let tab = app.tab_info(0, 0).unwrap();
        assert_eq!(tab.label, "only-agent");
        assert_eq!(tab.label_source, Some(TabLabelSource::Following));
    }

    #[tokio::test]
    async fn a_tab_with_no_agent_stays_numbered() {
        let (mut app, _event_hub) = app();
        let pane_id = app.state.workspaces[0].tabs[0].root_pane;
        let terminal_id = app.state.workspaces[0]
            .terminal_id(pane_id)
            .unwrap()
            .clone();
        app.terminal_runtimes.insert(
            terminal_id,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(80, 24, b""),
        );

        set_title(&mut app, pane_id, "vim notes.md");

        let tab = app.tab_info(0, 0).unwrap();
        assert_eq!(tab.label, "1");
        assert_eq!(tab.label_source, Some(TabLabelSource::Number));
    }
}
