use std::collections::{HashMap, HashSet};

use super::{terminal_targets::TerminalTargetError, App};
use crate::terminal::{AgentId, TerminalId};

pub(super) enum AgentParentError {
    Target(TerminalTargetError),
    InvalidParent(String),
    ParentNotFound(String),
    Cycle(String),
    CrossWorkspace(String),
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
        changed.extend(self.drop_cross_workspace_parents());
        let dropped = self.state.messages.retain_recipients(&live);
        if dropped > 0 {
            tracing::info!(dropped, "dropped messages to agents that are gone");
            self.state.mark_session_dirty();
            self.schedule_session_save();
        }
        self.cancel_waits_of_gone_agents(&live);
        self.settle_waits_on_gone_nodes(&live);
        self.sync_declared_waits();
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
        let Some(parent_terminal) = self.terminal_with_agent_id(&parent_id) else {
            return Err(AgentParentError::ParentNotFound(parent));
        };
        let workspace_of = |terminal| self.pane_for_terminal(terminal).map(|(ws_idx, _)| ws_idx);
        if workspace_of(&parent_terminal) != workspace_of(terminal_id) {
            return Err(AgentParentError::CrossWorkspace(parent));
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

    /// Clears every Parent that names an agent in another workspace, since a
    /// Parent stays inside one (ADR 0008). Returns the terminals it changed;
    /// the caller saves and publishes `pane.updated` for them.
    pub(crate) fn drop_cross_workspace_parents(&mut self) -> Vec<TerminalId> {
        let workspace_of: HashMap<&TerminalId, usize> = self
            .state
            .workspaces
            .iter()
            .enumerate()
            .flat_map(|(ws_idx, ws)| {
                ws.tabs
                    .iter()
                    .flat_map(|tab| tab.panes.values())
                    .map(move |pane| (&pane.attached_terminal_id, ws_idx))
            })
            .collect();
        let agent_workspace: HashMap<&AgentId, usize> = self
            .state
            .terminals
            .values()
            .filter_map(|terminal| {
                Some((
                    terminal.agent_id.as_ref()?,
                    *workspace_of.get(&terminal.id)?,
                ))
            })
            .collect();
        let crossing: Vec<TerminalId> = self
            .state
            .terminals
            .values()
            .filter(|terminal| {
                let parent_ws = terminal
                    .parent_agent_id
                    .as_ref()
                    .and_then(|parent| agent_workspace.get(parent));
                let own_ws = workspace_of.get(&terminal.id);
                parent_ws
                    .zip(own_ws)
                    .is_some_and(|(parent, own)| parent != own)
            })
            .map(|terminal| terminal.id.clone())
            .collect();
        for terminal_id in &crossing {
            if let Some(terminal) = self.state.terminals.get_mut(terminal_id) {
                terminal.parent_agent_id = None;
            }
        }
        crossing
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
            AgentParentError::CrossWorkspace(parent) => crate::api::schema::ErrorBody {
                code: "agent_parent_cross_workspace".into(),
                message: format!("agent parent {parent} is in another workspace"),
            },
        }
    }

    /// An agent id, a pane id (with or without an agent), or an agent name.
    pub(super) fn resolve_agent_identity_target(
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
    use crate::app::App;
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

    #[tokio::test]
    async fn agent_start_without_a_parent_keeps_the_one_set_on_its_pane() {
        let mut f = fixture();
        f.detect(f.root, Agent::Claude);
        let parent = f.pane(f.root)["agent_id"].as_str().unwrap().to_string();
        let terminal_id = f.app.state.terminal_id_for_pane(0, f.second).unwrap();
        let (runtime, _receiver) =
            crate::terminal::TerminalRuntime::test_with_channel_capacity(80, 24, 4);
        f.app.terminal_runtimes.insert(terminal_id, runtime);
        let shell = f.public(f.second);
        let linked = f.set_parent(&shell, Some(&parent));
        let child = linked["result"]["pane"]["agent_id"]
            .as_str()
            .unwrap()
            .to_string();

        let response = f.call(Method::AgentStart(crate::api::schema::AgentStartParams {
            name: "worker".into(),
            kind: "pi".into(),
            pane_id: shell,
            args: Vec::new(),
            parent: None,
            timeout_ms: Some(4_000),
        }));

        let agent = &response["result"]["agent"];
        assert_eq!(agent["agent_id"], child.as_str());
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

        let pane_id = f.public(f.second);
        let closed = f.call(Method::PaneClose(PaneTarget { pane_id }));
        assert_eq!(closed["result"]["type"], "ok");
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

    /// Workspace 0 is the adversarial identity workspace (raw pane ids and tab
    /// positions differ from public numbers); workspace 1 is a plain one.
    /// Every pane runs a detected agent, and `agents[ws]` lists their ids.
    struct Spread {
        f: Fixture,
        agents: [Vec<String>; 2],
    }

    fn spread() -> Spread {
        let mut f = fixture();
        f.app.state.workspaces = vec![
            crate::workspace::Workspace::test_adversarial_identity_state(),
            crate::workspace::Workspace::test_new("other"),
        ];
        f.root = f.app.state.workspaces[0].tabs[0].root_pane;
        f.app.state.ensure_test_terminals();
        for terminal in f.app.state.terminals.values_mut() {
            terminal.set_detected_state(Some(Agent::Pi), AgentState::Idle);
        }
        f.app.sync_agent_identities();
        let mut agents = [Vec::new(), Vec::new()];
        for (ws_idx, ids) in agents.iter_mut().enumerate() {
            for tab in &f.app.state.workspaces[ws_idx].tabs {
                for pane in tab.panes.values() {
                    let terminal = &f.app.state.terminals[&pane.attached_terminal_id];
                    ids.push(terminal.agent_id.as_ref().unwrap().as_str().to_string());
                }
            }
            ids.sort();
        }
        assert!(
            agents[0].len() >= 4,
            "the adversarial workspace has several panes"
        );
        Spread { f, agents }
    }

    fn sorted(mut ids: Vec<String>) -> Vec<String> {
        ids.sort();
        ids
    }

    impl Spread {
        fn agent(&mut self, id: &str) -> serde_json::Value {
            self.f
                .call(Method::AgentGet(AgentTarget { target: id.into() }))["result"]["agent"]
                .clone()
        }

        fn parent_of(&mut self, id: &str) -> Option<String> {
            self.agent(id)["parent_agent_id"]
                .as_str()
                .map(str::to_string)
        }

        fn last_seq(&self) -> u64 {
            self.f
                .events
                .events_after(0)
                .last()
                .map(|(seq, _)| *seq)
                .unwrap_or(0)
        }

        /// Agent ids of the `pane.updated` events after `seq` that show no parent.
        fn unparented_updates(&self, seq: u64) -> Vec<String> {
            let mut ids: Vec<String> = self
                .f
                .events
                .events_after(seq)
                .iter()
                .filter_map(|(_, event)| match &event.data {
                    EventData::PaneUpdated { pane } if pane.parent_agent_id.is_none() => {
                        pane.agent_id.clone()
                    }
                    _ => None,
                })
                .collect();
            ids.sort();
            ids.dedup();
            ids
        }

        fn move_to_new_tab(&mut self, id: &str, ws_idx: usize) -> serde_json::Value {
            let workspace_id = self.f.app.public_workspace_id(ws_idx);
            self.move_pane(
                id,
                crate::api::schema::PaneMoveDestination::NewTab {
                    workspace_id: Some(workspace_id),
                    label: None,
                },
            )
        }

        fn move_pane(
            &mut self,
            id: &str,
            destination: crate::api::schema::PaneMoveDestination,
        ) -> serde_json::Value {
            let pane_id = self.agent(id)["pane_id"].as_str().unwrap().to_string();
            self.f
                .call(Method::PaneMove(crate::api::schema::PaneMoveParams {
                    pane_id,
                    destination,
                    focus: false,
                }))
        }

        /// Sets a Parent the way a snapshot or handoff carries it: unchecked.
        fn plant_parent(&mut self, child: &str, parent: &str) {
            let child = crate::terminal::AgentId::parse(child).unwrap();
            let terminal_id = self.f.app.terminal_with_agent_id(&child).unwrap();
            self.f
                .app
                .state
                .terminals
                .get_mut(&terminal_id)
                .unwrap()
                .parent_agent_id = crate::terminal::AgentId::parse(parent);
        }

        fn assert_no_cross_workspace_parent(&self) {
            self.f.app.state.assert_invariants_for_test();
            for terminal in self.f.app.state.terminals.values() {
                let Some(parent) = &terminal.parent_agent_id else {
                    continue;
                };
                let parent_terminal = self.f.app.terminal_with_agent_id(parent).unwrap();
                assert_eq!(
                    self.f.app.pane_for_terminal(&terminal.id).map(|(ws, _)| ws),
                    self.f
                        .app
                        .pane_for_terminal(&parent_terminal)
                        .map(|(ws, _)| ws),
                    "{:?} keeps parent {parent:?} from another workspace",
                    terminal.agent_id
                );
            }
        }
    }

    #[test]
    fn set_parent_refuses_a_parent_in_another_workspace() {
        let mut s = spread();
        let (here, there) = (s.agents[0][0].clone(), s.agents[1][0].clone());
        let sibling = s.agents[0][1].clone();
        s.f.set_parent(&here, Some(&sibling));

        let refused = s.f.set_parent(&here, Some(&there));

        assert_eq!(refused["error"]["code"], "agent_parent_cross_workspace");
        assert_eq!(s.parent_of(&here), Some(sibling));
        let reverse = s.f.set_parent(&there, Some(&here));
        assert_eq!(reverse["error"]["code"], "agent_parent_cross_workspace");
        assert_eq!(s.parent_of(&there), None);
        s.assert_no_cross_workspace_parent();
    }

    #[tokio::test]
    async fn agent_start_refuses_a_parent_in_another_workspace_before_typing() {
        let mut s = spread();
        let there = s.agents[1][0].clone();
        let shell = s.f.public(s.f.root);
        let terminal_id = s.f.app.state.terminal_id_for_pane(0, s.f.root).unwrap();
        s.f.app
            .state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .set_detected_state(None, AgentState::Unknown);
        let (runtime, mut receiver) =
            crate::terminal::TerminalRuntime::test_with_channel_capacity(80, 24, 4);
        s.f.app.terminal_runtimes.insert(terminal_id, runtime);

        let refused =
            s.f.call(Method::AgentStart(crate::api::schema::AgentStartParams {
                name: "worker".into(),
                kind: "pi".into(),
                pane_id: shell,
                args: Vec::new(),
                parent: Some(there),
                timeout_ms: Some(4_000),
            }));

        assert_eq!(refused["error"]["code"], "agent_parent_cross_workspace");
        assert!(
            receiver.try_recv().is_err(),
            "nothing typed for a parent in another workspace"
        );
    }

    #[test]
    fn moving_a_pane_to_another_workspace_drops_its_parent_and_its_childrens() {
        let mut s = spread();
        let [a, b, c, d] = [0, 1, 2, 3].map(|i| s.agents[0][i].clone());
        s.f.set_parent(&b, Some(&a));
        s.f.set_parent(&c, Some(&b));
        s.f.set_parent(&d, Some(&a));
        let seq = s.last_seq();

        let moved = s.move_to_new_tab(&b, 1);

        assert_eq!(moved["result"]["type"], "pane_move", "{moved}");
        assert!(moved["result"]["move_result"]["pane"]["parent_agent_id"].is_null());
        assert_eq!(s.parent_of(&b), None, "the moved agent loses its parent");
        assert_eq!(
            s.parent_of(&c),
            None,
            "a child left behind loses its parent"
        );
        assert_eq!(s.parent_of(&d), Some(a), "other links stay");
        assert_eq!(s.unparented_updates(seq), sorted(vec![b, c]));
        s.assert_no_cross_workspace_parent();
    }

    #[test]
    fn moving_a_pane_within_its_workspace_keeps_every_parent() {
        let mut s = spread();
        let [a, b, c] = [0, 1, 2].map(|i| s.agents[0][i].clone());
        s.f.set_parent(&b, Some(&a));
        s.f.set_parent(&c, Some(&b));
        let seq = s.last_seq();

        let moved = s.move_to_new_tab(&b, 0);

        assert_eq!(moved["result"]["type"], "pane_move", "{moved}");
        assert_eq!(s.parent_of(&b), Some(a));
        assert_eq!(s.parent_of(&c), Some(b));
        assert!(s.unparented_updates(seq).is_empty());
        s.assert_no_cross_workspace_parent();
    }

    #[test]
    fn restored_state_drops_cross_workspace_parents_once_identities_sync() {
        let mut s = spread();
        let [a, b] = [0, 1].map(|i| s.agents[0][i].clone());
        let there = s.agents[1][0].clone();
        for (child, parent) in [(&b, &a), (&a, &there), (&there, &b)] {
            s.plant_parent(child, parent);
        }
        s.f.app.state.agent_identities_dirty = true;
        let seq = s.last_seq();

        s.f.app.sync_agent_identities();

        assert_eq!(s.parent_of(&b), Some(a.clone()));
        assert_eq!(s.parent_of(&a), None);
        assert_eq!(s.parent_of(&there), None);
        assert_eq!(s.unparented_updates(seq), sorted(vec![a, there]));
        s.assert_no_cross_workspace_parent();
    }

    #[test]
    fn moving_to_an_existing_tab_or_a_new_workspace_drops_the_crossing_parents() {
        for destination in ["existing tab", "new workspace"] {
            let mut s = spread();
            let [a, b, c, d] = [0, 1, 2, 3].map(|i| s.agents[0][i].clone());
            s.f.set_parent(&b, Some(&a));
            s.f.set_parent(&c, Some(&b));
            s.f.set_parent(&d, Some(&a));
            let seq = s.last_seq();
            let destination = if destination == "existing tab" {
                crate::api::schema::PaneMoveDestination::Tab {
                    tab_id: s.f.app.public_tab_id(1, 0).unwrap(),
                    target_pane_id: None,
                    split: crate::api::schema::SplitDirection::Right,
                    ratio: None,
                }
            } else {
                crate::api::schema::PaneMoveDestination::NewWorkspace {
                    label: None,
                    tab_label: None,
                }
            };

            let moved = s.move_pane(&b, destination);

            assert_eq!(moved["result"]["type"], "pane_move", "{moved}");
            assert!(moved["result"]["move_result"]["pane"]["parent_agent_id"].is_null());
            assert_eq!(s.parent_of(&b), None);
            assert_eq!(s.parent_of(&c), None);
            assert_eq!(s.parent_of(&d), Some(a));
            assert_eq!(s.unparented_updates(seq), sorted(vec![b, c]));
            s.assert_no_cross_workspace_parent();
        }
    }

    #[test]
    fn a_move_that_closes_its_workspace_keeps_the_parents_that_shift_index() {
        let mut s = spread();
        // The plain workspace goes first, so closing it shifts the adversarial one to index 0.
        s.f.app.state.workspaces.swap(0, 1);
        s.agents.swap(0, 1);
        let solo = s.agents[0][0].clone();
        let [a, b, c] = [0, 1, 2].map(|i| s.agents[1][i].clone());
        s.f.set_parent(&b, Some(&a));
        s.f.set_parent(&c, Some(&b));
        let workspaces = s.f.app.state.workspaces.len();
        let seq = s.last_seq();

        let moved = s.move_pane(
            &solo,
            crate::api::schema::PaneMoveDestination::NewWorkspace {
                label: None,
                tab_label: None,
            },
        );

        assert_eq!(moved["result"]["type"], "pane_move", "{moved}");
        assert!(moved["result"]["move_result"]["closed_workspace_id"].is_string());
        assert_eq!(s.f.app.state.workspaces.len(), workspaces);
        assert_eq!(s.parent_of(&b), Some(a));
        assert_eq!(s.parent_of(&c), Some(b));
        assert!(s.unparented_updates(seq).is_empty());
        s.assert_no_cross_workspace_parent();
    }

    /// `b` keeps its same-workspace parent `a`; `a` and `there` were saved with parents
    /// in another workspace. Returns the snapshot and those three ids.
    #[cfg(unix)]
    fn snapshot_with_crossing_parents() -> (crate::persist::SessionSnapshot, [String; 3]) {
        let mut s = spread();
        let [a, b] = [0, 1].map(|i| s.agents[0][i].clone());
        let there = s.agents[1][0].clone();
        for (child, parent) in [(&b, &a), (&a, &there), (&there, &b)] {
            s.plant_parent(child, parent);
        }
        let snapshot = crate::persist::capture(
            &s.f.app.state.workspaces,
            &s.f.app.state.terminals,
            &s.f.app.terminal_runtimes,
            Some(0),
            0,
        );
        (snapshot, [a, b, there])
    }

    #[cfg(unix)]
    fn restore_config() -> crate::config::Config {
        let mut config = crate::config::Config::default();
        config.terminal.default_shell = "/bin/sh".into();
        config
    }

    /// Each agent's parent as the first API request after startup sees it.
    #[cfg(unix)]
    fn parents_seen_first(app: &mut App) -> std::collections::BTreeMap<String, Option<String>> {
        let response = app.handle_api_request(Request {
            id: "t".into(),
            method: Method::PaneList(crate::api::schema::PaneListParams::default()),
        });
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();
        response["result"]["panes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|pane| {
                let agent = pane["agent_id"].as_str()?.to_string();
                Some((agent, pane["parent_agent_id"].as_str().map(str::to_string)))
            })
            .collect()
    }

    // Windows CI restored only one of these panes (PR #30); the sync this relies on is
    // covered there by restored_state_drops_cross_workspace_parents_once_identities_sync.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_restored_session_drops_cross_workspace_parents() {
        let _guard = crate::config::test_config_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let home =
            std::env::temp_dir().join(format!("herdr-parent-restore-{}", std::process::id()));
        let previous = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", &home);
        let (snapshot, [a, b, there]) = snapshot_with_crossing_parents();
        let path = crate::session::data_dir().join("session.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string(&snapshot).unwrap()).unwrap();
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let policy = crate::app::AppPolicy {
            restore_session: true,
            ..crate::app::AppPolicy::TEST
        };

        let app = App::try_new(
            &restore_config(),
            policy,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        let parents = app.map(|mut app| parents_seen_first(&mut app));

        match previous {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        let _ = std::fs::remove_dir_all(&home);
        let parents = parents.unwrap();
        assert_eq!(parents.get(&b), Some(&Some(a.clone())), "{parents:?}");
        assert_eq!(parents.get(&a), Some(&None), "{parents:?}");
        assert_eq!(parents.get(&there), Some(&None), "{parents:?}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_live_handoff_drops_cross_workspace_parents() {
        let (snapshot, [a, b, there]) = snapshot_with_crossing_parents();
        let config = restore_config();
        let (events, _event_rx) = tokio::sync::mpsc::channel(64);
        // The outgoing server: real PTYs for every saved pane, handed over by fd.
        let (workspaces, terminals, runtimes) = crate::persist::restore(
            &snapshot,
            None,
            24,
            80,
            4096,
            &config.terminal.default_shell,
            crate::config::ShellModeConfig::NonLogin,
            false,
            events,
            std::sync::Arc::new(tokio::sync::Notify::new()),
            std::sync::Arc::new(crate::render_signal::RenderSignal::new()),
        );
        let runtimes = crate::terminal::TerminalRuntimeRegistry::from(runtimes);
        let handed = crate::persist::capture(&workspaces, &terminals, &runtimes, Some(0), 0);
        let mut imports = std::collections::HashMap::new();
        for tab in workspaces.iter().flat_map(|workspace| &workspace.tabs) {
            for (pane_id, pane) in &tab.panes {
                let runtime = runtimes.get(&pane.attached_terminal_id).unwrap();
                runtime
                    .pause_handoff_reader(std::time::Duration::from_secs(2))
                    .unwrap();
                let state = runtime.handoff_runtime_state(pane_id.raw());
                let state = serde_json::from_value(serde_json::to_value(state).unwrap()).unwrap();
                imports.insert(
                    pane_id.raw(),
                    crate::handoff_runtime::ImportedHandoffRuntime {
                        master_fd: runtime.duplicate_handoff_fd().unwrap(),
                        state,
                    },
                );
            }
        }
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();

        let mut app = App::new_from_handoff(
            &config,
            None,
            api_rx,
            crate::api::EventHub::default(),
            &handed,
            &mut imports,
        )
        .unwrap();
        let parents = parents_seen_first(&mut app);

        assert_eq!(parents.get(&b), Some(&Some(a.clone())), "{parents:?}");
        assert_eq!(parents.get(&a), Some(&None), "{parents:?}");
        assert_eq!(parents.get(&there), Some(&None), "{parents:?}");
        drop(app);
        drop(runtimes);
    }
}
