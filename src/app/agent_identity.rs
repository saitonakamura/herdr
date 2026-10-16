use std::collections::HashSet;

use super::{terminal_targets::TerminalTargetError, App};
use crate::terminal::{AgentId, TerminalId};

pub(super) enum AgentParentError {
    Target(TerminalTargetError),
    InvalidParent(String),
    ParentNotFound(String),
    Cycle(String),
}

impl App {
    /// Mints ids for agents that have none and drops parents whose agent is gone.
    pub(crate) fn sync_agent_identities(&mut self) {
        let needs_mint = self
            .state
            .terminals
            .values()
            .any(|terminal| terminal.agent_id.is_none() && terminal.is_agent_terminal());
        if !needs_mint && !self.state.agent_identities_dirty {
            return;
        }
        self.state.agent_identities_dirty = false;

        let mut terminal_ids: Vec<TerminalId> = self.state.terminals.keys().cloned().collect();
        // Deterministic order, so a duplicated restored id stays with the same terminal.
        terminal_ids.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
        let mut live = HashSet::new();
        let mut changed = Vec::new();
        for terminal_id in &terminal_ids {
            let Some(terminal) = self.state.terminals.get_mut(terminal_id) else {
                continue;
            };
            if let Some(agent_id) = terminal.agent_id.clone() {
                if !live.insert(agent_id) {
                    terminal.agent_id = None;
                    changed.push(terminal_id.clone());
                }
            }
        }
        for terminal_id in &terminal_ids {
            let Some(terminal) = self.state.terminals.get_mut(terminal_id) else {
                continue;
            };
            if terminal.agent_id.is_none() && terminal.is_agent_terminal() {
                let agent_id = mint_unique(&live);
                live.insert(agent_id.clone());
                terminal.agent_id = Some(agent_id);
                changed.push(terminal_id.clone());
            }
        }
        for terminal_id in &terminal_ids {
            let Some(terminal) = self.state.terminals.get_mut(terminal_id) else {
                continue;
            };
            let orphaned = terminal.parent_agent_id.as_ref().is_some_and(|parent| {
                !live.contains(parent) || terminal.agent_id.as_ref() == Some(parent)
            });
            if orphaned {
                terminal.parent_agent_id = None;
                changed.push(terminal_id.clone());
            }
        }
        if changed.is_empty() {
            return;
        }
        changed.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
        changed.dedup();
        self.state.mark_session_dirty();
        self.schedule_session_save();
        for terminal_id in changed {
            if let Some((ws_idx, pane_id)) = self.pane_for_terminal(&terminal_id) {
                self.emit_pane_updated(ws_idx, pane_id);
            }
        }
    }

    pub(super) fn set_agent_parent(
        &mut self,
        target: &str,
        parent: Option<String>,
    ) -> Result<crate::api::schema::PaneInfo, AgentParentError> {
        let (ws_idx, pane_id, terminal_id) = self.resolve_agent_identity_target(target)?;
        let parent = self.validate_agent_parent(parent, &terminal_id)?;
        let changed = self.assign_agent_identity(&terminal_id, Some(parent));
        if changed {
            self.state.mark_session_dirty();
            self.schedule_session_save();
            self.emit_pane_updated(ws_idx, pane_id);
        }
        self.pane_info(ws_idx, pane_id).ok_or_else(|| {
            AgentParentError::Target(TerminalTargetError::NotFound {
                target: target.to_string(),
            })
        })
    }

    /// Parses a requested parent and checks it names a live agent that is not
    /// the target itself or one of its descendants.
    pub(super) fn validate_agent_parent(
        &self,
        parent: Option<String>,
        terminal_id: &TerminalId,
    ) -> Result<Option<AgentId>, AgentParentError> {
        let Some(parent) = parent else {
            return Ok(None);
        };
        let parent_id = AgentId::parse(&parent)
            .ok_or_else(|| AgentParentError::InvalidParent(parent.clone()))?;
        if self.terminal_with_agent_id(&parent_id).is_none() {
            return Err(AgentParentError::ParentNotFound(parent));
        }
        let own_id = self
            .state
            .terminals
            .get(terminal_id)
            .and_then(|terminal| terminal.agent_id.clone());
        if let Some(own_id) = own_id {
            let mut cursor = Some(parent_id.clone());
            let mut steps = 0;
            while let Some(id) = cursor {
                if id == own_id {
                    return Err(AgentParentError::Cycle(parent));
                }
                steps += 1;
                if steps > self.state.terminals.len() {
                    break;
                }
                cursor = self
                    .terminal_with_agent_id(&id)
                    .and_then(|terminal_id| self.state.terminals.get(&terminal_id))
                    .and_then(|terminal| terminal.parent_agent_id.clone());
            }
        }
        Ok(Some(parent_id))
    }

    /// Gives the terminal an agent id if it has none and, when `parent` is
    /// `Some`, replaces its parent. Returns whether anything changed.
    pub(super) fn assign_agent_identity(
        &mut self,
        terminal_id: &TerminalId,
        parent: Option<Option<AgentId>>,
    ) -> bool {
        let needs_id = self
            .state
            .terminals
            .get(terminal_id)
            .is_some_and(|terminal| terminal.agent_id.is_none());
        let minted = needs_id.then(|| {
            let live = self
                .state
                .terminals
                .values()
                .filter_map(|terminal| terminal.agent_id.clone())
                .collect();
            mint_unique(&live)
        });
        let Some(terminal) = self.state.terminals.get_mut(terminal_id) else {
            return false;
        };
        let mut changed = false;
        if let Some(agent_id) = minted {
            terminal.agent_id = Some(agent_id);
            changed = true;
        }
        if let Some(parent) = parent {
            if terminal.parent_agent_id != parent {
                terminal.parent_agent_id = parent;
                changed = true;
            }
        }
        changed
    }

    pub(super) fn agent_parent_error_body(
        &self,
        err: AgentParentError,
    ) -> crate::api::schema::ErrorBody {
        match err {
            AgentParentError::Target(err) => self.agent_target_error_body(err),
            AgentParentError::InvalidParent(parent) => crate::api::schema::ErrorBody {
                code: "invalid_agent_parent".into(),
                message: format!("agent parent {parent} is not an agent id"),
            },
            AgentParentError::ParentNotFound(parent) => crate::api::schema::ErrorBody {
                code: "agent_parent_not_found".into(),
                message: format!("agent parent {parent} not found"),
            },
            AgentParentError::Cycle(parent) => crate::api::schema::ErrorBody {
                code: "agent_parent_cycle".into(),
                message: format!("agent {parent} is the target or one of its descendants"),
            },
        }
    }

    /// An agent id, a pane id (with or without an agent), or an agent name.
    fn resolve_agent_identity_target(
        &self,
        target: &str,
    ) -> Result<(usize, crate::layout::PaneId, TerminalId), AgentParentError> {
        let terminal_id = AgentId::parse(target)
            .and_then(|agent_id| self.terminal_with_agent_id(&agent_id))
            .or_else(|| {
                let (ws_idx, pane_id) = self.parse_current_public_pane_id(target)?;
                self.state.terminal_id_for_pane(ws_idx, pane_id)
            });
        let terminal_id = match terminal_id {
            Some(terminal_id) => terminal_id,
            None => {
                let resolved = self
                    .resolve_agent_target(target)
                    .map_err(AgentParentError::Target)?;
                self.state
                    .terminal_id_for_pane(resolved.ws_idx, resolved.pane_id)
                    .ok_or_else(|| {
                        AgentParentError::Target(TerminalTargetError::NotFound {
                            target: target.to_string(),
                        })
                    })?
            }
        };
        let (ws_idx, pane_id) = self.pane_for_terminal(&terminal_id).ok_or_else(|| {
            AgentParentError::Target(TerminalTargetError::NotFound {
                target: target.to_string(),
            })
        })?;
        Ok((ws_idx, pane_id, terminal_id))
    }

    pub(crate) fn terminal_with_agent_id(&self, agent_id: &AgentId) -> Option<TerminalId> {
        self.state
            .terminals
            .values()
            .find(|terminal| terminal.agent_id.as_ref() == Some(agent_id))
            .map(|terminal| terminal.id.clone())
    }

    pub(crate) fn pane_for_terminal(
        &self,
        terminal_id: &TerminalId,
    ) -> Option<(usize, crate::layout::PaneId)> {
        self.state
            .workspaces
            .iter()
            .enumerate()
            .find_map(|(ws_idx, ws)| {
                ws.tabs.iter().find_map(|tab| {
                    tab.panes
                        .iter()
                        .find(|(_, pane)| &pane.attached_terminal_id == terminal_id)
                        .map(|(pane_id, _)| (ws_idx, *pane_id))
                })
            })
    }
}

fn mint_unique(live: &HashSet<AgentId>) -> AgentId {
    loop {
        let agent_id = AgentId::alloc();
        if !live.contains(&agent_id) {
            return agent_id;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::schema::{
        AgentSetParentParams, AgentTarget, EmptyParams, EventData, EventKind, Method, PaneTarget,
        Request,
    };
    use crate::app::{App, AppEvent};
    use crate::detect::{Agent, AgentState};
    use crate::layout::PaneId;

    struct Fixture {
        app: App,
        events: crate::api::EventHub,
        root: PaneId,
        second: PaneId,
    }

    fn fixture() -> Fixture {
        let events = crate::api::EventHub::default();
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            api_rx,
            events.clone(),
        );
        let mut workspace = crate::workspace::Workspace::test_new("identity");
        let root = workspace.tabs[0].root_pane;
        let second = workspace.test_split(ratatui::layout::Direction::Horizontal);
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        Fixture {
            app,
            events,
            root,
            second,
        }
    }

    impl Fixture {
        fn detect(&mut self, pane: PaneId, agent: Agent) {
            let terminal_id = self.app.state.terminal_id_for_pane(0, pane).unwrap();
            self.app
                .state
                .terminals
                .get_mut(&terminal_id)
                .unwrap()
                .set_detected_state(Some(agent), AgentState::Idle);
        }

        fn public(&self, pane: PaneId) -> String {
            self.app.public_pane_id(0, pane).unwrap()
        }

        fn call(&mut self, method: Method) -> serde_json::Value {
            let response = self.app.handle_api_request(Request {
                id: "t".into(),
                method,
            });
            serde_json::from_str(&response).unwrap()
        }

        fn pane(&mut self, pane: PaneId) -> serde_json::Value {
            let pane_id = self.public(pane);
            self.call(Method::PaneGet(PaneTarget { pane_id }))["result"]["pane"].clone()
        }

        fn set_parent(&mut self, target: &str, parent: Option<&str>) -> serde_json::Value {
            self.call(Method::AgentSetParent(AgentSetParentParams {
                target: target.into(),
                parent: parent.map(str::to_string),
            }))
        }
    }

    #[test]
    fn a_detected_agent_gets_one_stable_id_when_the_api_first_refers_to_it() {
        let mut f = fixture();
        f.detect(f.root, Agent::Pi);

        let listed = f.call(Method::AgentList(EmptyParams::default()));
        let agent_id = listed["result"]["agents"][0]["agent_id"]
            .as_str()
            .expect("listed agent has an id")
            .to_string();

        assert!(crate::terminal::AgentId::parse(&agent_id).is_some());
        assert_eq!(f.pane(f.root)["agent_id"], agent_id.as_str());
        assert!(
            f.pane(f.second)["agent_id"].is_null(),
            "a plain shell gets no id"
        );
        let by_id = f.call(Method::AgentGet(AgentTarget {
            target: agent_id.clone(),
        }));
        assert_eq!(
            by_id["result"]["agent"]["pane_id"],
            f.public(f.root).as_str()
        );
    }

    #[test]
    fn set_parent_links_agents_and_rejects_bad_parents() {
        let mut f = fixture();
        f.detect(f.root, Agent::Claude);
        f.detect(f.second, Agent::Pi);
        let parent = f.pane(f.root)["agent_id"].as_str().unwrap().to_string();
        let child = f.pane(f.second)["agent_id"].as_str().unwrap().to_string();

        let linked = f.set_parent(&f.public(f.second), Some(&parent));
        assert_eq!(linked["result"]["pane"]["parent_agent_id"], parent.as_str());
        let agent = f.call(Method::AgentGet(AgentTarget {
            target: child.clone(),
        }));
        assert_eq!(agent["result"]["agent"]["parent_agent_id"], parent.as_str());

        for (target, bad_parent, code) in [
            (parent.as_str(), child.as_str(), "agent_parent_cycle"),
            (child.as_str(), child.as_str(), "agent_parent_cycle"),
            (child.as_str(), "reviewer", "invalid_agent_parent"),
            (child.as_str(), "99999999", "agent_parent_not_found"),
        ] {
            let response = f.set_parent(target, Some(bad_parent));
            assert_eq!(response["error"]["code"], code, "{target} -> {bad_parent}");
        }
        assert_eq!(f.pane(f.second)["parent_agent_id"], parent.as_str());

        let cleared = f.set_parent(&child, None);
        assert!(cleared["result"]["pane"]["parent_agent_id"].is_null());
        assert_eq!(cleared["result"]["pane"]["agent_id"], child.as_str());
    }

    #[test]
    fn set_parent_gives_a_pane_without_an_agent_an_id() {
        let mut f = fixture();
        let shell = f.public(f.second);

        let response = f.set_parent(&shell, None);

        let agent_id = response["result"]["pane"]["agent_id"].as_str().unwrap();
        assert!(crate::terminal::AgentId::parse(agent_id).is_some());
        assert_eq!(f.pane(f.second)["agent_id"], agent_id);
    }

    #[tokio::test]
    async fn agent_start_sets_the_parent_and_rejects_a_bad_one_before_typing() {
        let mut f = fixture();
        f.detect(f.root, Agent::Claude);
        let parent = f.pane(f.root)["agent_id"].as_str().unwrap().to_string();
        let terminal_id = f.app.state.terminal_id_for_pane(0, f.second).unwrap();
        let (runtime, mut receiver) =
            crate::terminal::TerminalRuntime::test_with_channel_capacity(80, 24, 4);
        f.app.terminal_runtimes.insert(terminal_id, runtime);
        let start = |parent: &str| {
            Method::AgentStart(crate::api::schema::AgentStartParams {
                name: "worker".into(),
                kind: "pi".into(),
                pane_id: f.public(f.second),
                args: Vec::new(),
                parent: Some(parent.into()),
                timeout_ms: Some(4_000),
            })
        };
        let missing = start("99999999");
        let started = start(&parent);

        let refused = f.call(missing);
        assert_eq!(refused["error"]["code"], "agent_parent_not_found");
        assert!(
            receiver.try_recv().is_err(),
            "nothing typed for a bad parent"
        );

        let response = f.call(started);
        let agent = &response["result"]["agent"];
        assert!(agent["agent_id"].as_str().is_some_and(|id| id != parent));
        assert_eq!(agent["parent_agent_id"], parent.as_str());
    }

    #[test]
    fn closing_the_parent_pane_makes_its_children_top_level() {
        let mut f = fixture();
        f.detect(f.root, Agent::Claude);
        f.detect(f.second, Agent::Pi);
        let parent = f.pane(f.second)["agent_id"].as_str().unwrap().to_string();
        let child = f.pane(f.root)["agent_id"].as_str().unwrap().to_string();
        f.set_parent(&child, Some(&parent));
        let after = f
            .events
            .events_after(0)
            .last()
            .map(|(seq, _)| *seq)
            .unwrap_or(0);

        f.app.handle_internal_event(AppEvent::PaneDied {
            pane_id: f.second,
            exit_reason: crate::platform::ChildExitReason::Exited,
        });
        assert!(f.app.state.agent_identities_dirty);
        f.app.sync_agent_identities();

        assert!(f.pane(f.root)["parent_agent_id"].is_null());
        assert_eq!(f.pane(f.root)["agent_id"], child.as_str());
        assert!(f.events.events_after(after).iter().any(|(_, event)| {
            event.event == EventKind::PaneUpdated
                && matches!(&event.data, EventData::PaneUpdated { pane }
                    if pane.agent_id.as_deref() == Some(child.as_str())
                        && pane.parent_agent_id.is_none())
        }));
    }
}
