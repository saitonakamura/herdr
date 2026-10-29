use std::collections::{HashMap, HashSet};

use super::agent_identity::AgentParentError;
use super::App;
use crate::api::schema::{
    AgentCloseParams, AgentCloseScope, ClosedAgentInfo, ErrorBody, ErrorResponse, PaneTarget,
    TaskStatus,
};
use crate::detect::AgentState;
use crate::layout::PaneId;
use crate::terminal::TerminalId;

impl App {
    pub(super) fn set_agent_task_status(
        &mut self,
        target: &str,
        status: TaskStatus,
    ) -> Result<crate::api::schema::PaneInfo, AgentParentError> {
        let (ws_idx, pane_id, terminal_id) = self.resolve_agent_identity_target(target)?;
        let mut changed = self.assign_agent_identity(&terminal_id, None);
        if let Some(terminal) = self.state.terminals.get_mut(&terminal_id) {
            changed |= terminal.task_status != Some(status);
            terminal.task_status = Some(status);
        }
        if let Some(agent_id) = self
            .state
            .terminals
            .get(&terminal_id)
            .and_then(|terminal| terminal.agent_id.clone())
        {
            changed |= self.clear_wait_needs_user(&agent_id);
        }
        if changed {
            self.state.mark_session_dirty();
            self.schedule_session_save();
            self.emit_pane_updated(ws_idx, pane_id);
        }
        self.pane_info(ws_idx, pane_id).ok_or_else(|| {
            AgentParentError::Target(super::terminal_targets::TerminalTargetError::NotFound {
                target: target.to_string(),
            })
        })
    }

    /// Closes each target with every descendant, deepest first and the caller's
    /// own pane last. Returns what closed (or would close) and whether it was a dry run.
    pub(super) fn close_agent_subtrees(
        &mut self,
        request_id: &str,
        params: AgentCloseParams,
    ) -> Result<(Vec<ClosedAgentInfo>, bool), ErrorBody> {
        if params.targets.is_empty() {
            return Err(ErrorBody {
                code: "invalid_request".into(),
                message: "agent close needs at least one target".into(),
            });
        }
        let mut roots = Vec::new();
        for target in &params.targets {
            let (_, _, terminal_id) = self
                .resolve_agent_identity_target(target)
                .map_err(|err| self.agent_parent_error_body(err))?;
            roots.push(terminal_id);
        }
        let caller = params
            .caller_pane_id
            .as_deref()
            .and_then(|pane| self.parse_current_public_pane_id(pane))
            .and_then(|(ws_idx, pane_id)| self.state.terminal_id_for_pane(ws_idx, pane_id));

        let mut members = self.subtree_members(roots);
        // The caller's pane goes last: closing it may end the process waiting on this response.
        if let Some(position) = caller
            .as_ref()
            .and_then(|caller| members.iter().position(|member| member == caller))
        {
            let caller = members.remove(position);
            members.push(caller);
        }
        if !params.force {
            self.refuse_busy_members(&members, caller.as_ref())?;
        }

        let mut gone = HashSet::new();
        let mut closed = Vec::new();
        for terminal_id in members {
            let Some((ws_idx, pane_id)) = self.pane_for_terminal(&terminal_id) else {
                continue;
            };
            let Some(public_pane_id) = self.public_pane_id(ws_idx, pane_id) else {
                continue;
            };
            let scope = self.close_scope(ws_idx, pane_id, &gone);
            let terminal = self.state.terminals.get(&terminal_id);
            let mut info = ClosedAgentInfo {
                agent_id: terminal
                    .and_then(|terminal| terminal.agent_id.as_ref())
                    .map(ToString::to_string),
                name: terminal.and_then(|terminal| terminal.agent_name.clone()),
                pane_id: public_pane_id.clone(),
                scope,
                error: None,
            };
            if params.dry_run {
                gone.insert(pane_id);
            } else if let Err(response) = self.close_pane(
                request_id.to_string(),
                &PaneTarget {
                    pane_id: public_pane_id,
                },
            ) {
                info.error = Some(
                    serde_json::from_str::<ErrorResponse>(&response)
                        .map(|response| response.error.message)
                        .unwrap_or(response),
                );
            }
            closed.push(info);
        }
        Ok((closed, params.dry_run))
    }

    /// The roots and every agent below them by Parent, deepest first; ties keep
    /// a stable order so the same tree always closes the same way.
    fn subtree_members(&self, roots: Vec<TerminalId>) -> Vec<TerminalId> {
        let mut children: HashMap<crate::terminal::AgentId, Vec<TerminalId>> = HashMap::new();
        for terminal in self.state.terminals.values() {
            if let Some(parent) = &terminal.parent_agent_id {
                children
                    .entry(parent.clone())
                    .or_default()
                    .push(terminal.id.clone());
            }
        }
        let mut members = Vec::new();
        let mut seen = HashSet::new();
        let mut queue = roots;
        while let Some(terminal_id) = queue.pop() {
            if !seen.insert(terminal_id.clone()) {
                continue;
            }
            if let Some(agent_id) = self
                .state
                .terminals
                .get(&terminal_id)
                .and_then(|terminal| terminal.agent_id.as_ref())
            {
                queue.extend(children.get(agent_id).cloned().unwrap_or_default());
            }
            members.push(terminal_id);
        }
        // Depth within the closing set, so a target that is another's descendant sorts below it.
        let depth = |terminal_id: &TerminalId| {
            let mut depth = 0;
            let mut cursor = self
                .state
                .terminals
                .get(terminal_id)
                .and_then(|terminal| terminal.parent_agent_id.clone());
            while let Some(parent) = cursor {
                let Some(parent_terminal) = self.terminal_with_agent_id(&parent) else {
                    break;
                };
                if !seen.contains(&parent_terminal) || depth > seen.len() {
                    break;
                }
                depth += 1;
                cursor = self
                    .state
                    .terminals
                    .get(&parent_terminal)
                    .and_then(|terminal| terminal.parent_agent_id.clone());
            }
            depth
        };
        let mut ordered: Vec<_> = members
            .into_iter()
            .map(|terminal_id| (depth(&terminal_id), terminal_id))
            .collect();
        ordered.sort_by(|(a_depth, a), (b_depth, b)| {
            b_depth
                .cmp(a_depth)
                .then_with(|| a.as_str().cmp(b.as_str()))
        });
        ordered
            .into_iter()
            .map(|(_, terminal_id)| terminal_id)
            .collect()
    }

    fn refuse_busy_members(
        &self,
        members: &[TerminalId],
        caller: Option<&TerminalId>,
    ) -> Result<(), ErrorBody> {
        let busy: Vec<String> = members
            .iter()
            .filter(|terminal_id| Some(*terminal_id) != caller)
            .filter_map(|terminal_id| {
                let terminal = self.state.terminals.get(terminal_id)?;
                let state = match terminal.state {
                    AgentState::Working => "working",
                    AgentState::Blocked => "blocked",
                    _ => return None,
                };
                let (ws_idx, pane_id) = self.pane_for_terminal(terminal_id)?;
                let pane = self.public_pane_id(ws_idx, pane_id)?;
                let who = terminal
                    .agent_name
                    .clone()
                    .or_else(|| terminal.agent_id.as_ref().map(ToString::to_string))
                    .unwrap_or_else(|| pane.clone());
                Some(format!("{who} in {pane} is {state}"))
            })
            .collect();
        if busy.is_empty() {
            return Ok(());
        }
        Err(ErrorBody {
            code: "agent_subtree_busy".into(),
            message: format!(
                "nothing closed, still working or blocked: {}",
                busy.join("; ")
            ),
        })
    }

    /// What closing this pane takes with it, counting panes in `gone` as already closed.
    fn close_scope(
        &self,
        ws_idx: usize,
        pane_id: PaneId,
        gone: &HashSet<PaneId>,
    ) -> AgentCloseScope {
        let Some(ws) = self.state.workspaces.get(ws_idx) else {
            return AgentCloseScope::Pane;
        };
        let open = |tab: &crate::workspace::Tab| {
            tab.layout
                .pane_ids()
                .into_iter()
                .filter(|pane| !gone.contains(pane))
                .count()
        };
        let Some(tab) = ws
            .find_tab_index_for_pane(pane_id)
            .and_then(|tab_idx| ws.tabs.get(tab_idx))
        else {
            return AgentCloseScope::Pane;
        };
        if open(tab) > 1 {
            return AgentCloseScope::Pane;
        }
        let open_tabs = ws.tabs.iter().filter(|tab| open(tab) > 0).count();
        if open_tabs > 1 {
            AgentCloseScope::Tab
        } else {
            AgentCloseScope::Workspace
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::schema::{
        AgentCloseParams, AgentSetParentParams, AgentSetTaskStatusParams, AgentTarget, EventData,
        EventKind, Method, PaneTarget, Request, TaskStatus,
    };
    use crate::app::App;
    use crate::detect::{Agent, AgentState};
    use crate::layout::PaneId;

    /// Tab 1: `boss` and an unrelated `bystander` shell. Tab 2: `worker`, a
    /// child of boss, and `grand`, a child of worker.
    struct Fixture {
        app: App,
        events: crate::api::EventHub,
        boss: PaneId,
        bystander: PaneId,
        worker: PaneId,
        grand: PaneId,
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
        let mut workspace = crate::workspace::Workspace::test_new("tasks");
        let boss = workspace.tabs[0].root_pane;
        let bystander = workspace.test_split(ratatui::layout::Direction::Horizontal);
        let tab = workspace.test_add_tab(None);
        workspace.active_tab = tab;
        let worker = workspace.tabs[tab].root_pane;
        let grand = workspace.test_split(ratatui::layout::Direction::Horizontal);
        workspace.active_tab = 0;
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        let mut f = Fixture {
            app,
            events,
            boss,
            bystander,
            worker,
            grand,
        };
        let boss_id = f.agent_id(boss);
        let worker_id = f.agent_id(worker);
        f.link(worker, &boss_id);
        f.link(grand, &worker_id);
        f
    }

    impl Fixture {
        fn call(&mut self, method: Method) -> serde_json::Value {
            let response = self.app.handle_api_request(Request {
                id: "t".into(),
                method,
            });
            serde_json::from_str(&response).unwrap()
        }

        fn public(&self, pane: PaneId) -> String {
            self.app.public_pane_id(0, pane).unwrap()
        }

        fn agent_id(&mut self, pane: PaneId) -> String {
            let target = self.public(pane);
            let response = self.call(Method::AgentSetParent(AgentSetParentParams {
                target,
                parent: None,
            }));
            response["result"]["pane"]["agent_id"]
                .as_str()
                .unwrap()
                .to_string()
        }

        fn link(&mut self, pane: PaneId, parent: &str) {
            let target = self.public(pane);
            let response = self.call(Method::AgentSetParent(AgentSetParentParams {
                target,
                parent: Some(parent.into()),
            }));
            assert!(response.get("result").is_some(), "{response}");
        }

        fn set_state(&mut self, pane: PaneId, state: AgentState) {
            let terminal_id = self.app.state.terminal_id_for_pane(0, pane).unwrap();
            self.app
                .state
                .terminals
                .get_mut(&terminal_id)
                .unwrap()
                .set_detected_state(Some(Agent::Pi), state);
        }

        fn close(&mut self, targets: &[PaneId], force: bool, dry_run: bool) -> serde_json::Value {
            let targets = targets.iter().map(|pane| self.public(*pane)).collect();
            self.close_as(targets, force, dry_run, None)
        }

        fn close_as(
            &mut self,
            targets: Vec<String>,
            force: bool,
            dry_run: bool,
            caller_pane_id: Option<String>,
        ) -> serde_json::Value {
            self.call(Method::AgentClose(AgentCloseParams {
                targets,
                force,
                dry_run,
                caller_pane_id,
            }))
        }

        fn alive(&self, pane: PaneId) -> bool {
            self.app
                .state
                .workspaces
                .iter()
                .any(|ws| ws.tabs.iter().any(|tab| tab.panes.contains_key(&pane)))
        }
    }

    fn closed(response: &serde_json::Value) -> Vec<(String, String)> {
        response["result"]["agents"]
            .as_array()
            .unwrap_or_else(|| panic!("no agents in {response}"))
            .iter()
            .map(|agent| {
                (
                    agent["pane_id"].as_str().unwrap().to_string(),
                    agent["scope"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn task_status_is_stored_next_to_state_and_published() {
        let mut f = fixture();
        f.set_state(f.worker, AgentState::Working);
        let after = f
            .events
            .events_after(0)
            .last()
            .map(|(seq, _)| *seq)
            .unwrap_or(0);
        let target = f.public(f.worker);

        let response = f.call(Method::AgentSetTaskStatus(AgentSetTaskStatusParams {
            target: target.clone(),
            status: TaskStatus::NeedsParent,
        }));

        assert_eq!(response["result"]["pane"]["task_status"], "needs_parent");
        assert_eq!(response["result"]["pane"]["agent_status"], "working");
        let agent = f.call(Method::AgentGet(AgentTarget { target }));
        assert_eq!(agent["result"]["agent"]["task_status"], "needs_parent");
        assert!(f.events.events_after(after).iter().any(|(_, event)| {
            event.event == EventKind::PaneUpdated
                && matches!(&event.data, EventData::PaneUpdated { pane }
                    if pane.task_status == Some(TaskStatus::NeedsParent))
        }));
    }

    #[test]
    fn closing_an_agent_takes_its_subtree_deepest_first() {
        let mut f = fixture();
        let (grand, worker) = (f.public(f.grand), f.public(f.worker));

        let response = f.close(&[f.worker], false, false);

        assert_eq!(
            closed(&response),
            vec![(grand, "pane".into()), (worker, "tab".into())]
        );
        assert!(!f.alive(f.grand) && !f.alive(f.worker));
        assert!(f.alive(f.boss) && f.alive(f.bystander));
    }

    #[test]
    fn a_tab_or_workspace_closes_only_when_the_subtree_owns_all_of_it() {
        let mut f = fixture();
        let boss = f.public(f.boss);

        let response = f.close(&[f.boss], false, false);
        assert_eq!(closed(&response).last(), Some(&(boss, "pane".into())));
        assert!(
            f.alive(f.bystander),
            "a pane outside the subtree keeps its tab"
        );

        let mut f = fixture();
        let response = f.close(&[f.boss, f.bystander], false, false);
        assert_eq!(
            closed(&response).last().map(|(_, scope)| scope.as_str()),
            Some("workspace")
        );
        assert!(f.app.state.workspaces.is_empty());
    }

    #[test]
    fn a_working_or_blocked_member_refuses_the_close_unless_forced() {
        let mut f = fixture();
        f.set_state(f.grand, AgentState::Blocked);

        let refused = f.close(&[f.boss], false, false);
        assert_eq!(refused["error"]["code"], "agent_subtree_busy");
        assert!(refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains(&f.public(f.grand)));
        assert!(f.alive(f.grand) && f.alive(f.worker) && f.alive(f.boss));

        let forced = f.close(&[f.boss], true, false);
        assert_eq!(closed(&forced).len(), 3);
        assert!(!f.alive(f.grand));
    }

    #[test]
    fn the_caller_is_never_its_own_blocker_and_closes_last() {
        let mut f = fixture();
        f.set_state(f.grand, AgentState::Working);
        let (caller, worker) = (f.public(f.grand), f.public(f.worker));

        let response = f.close_as(vec![worker.clone()], false, false, Some(caller.clone()));

        assert_eq!(
            closed(&response),
            vec![(worker, "pane".into()), (caller, "tab".into())]
        );
        assert!(!f.alive(f.worker) && !f.alive(f.grand));
    }

    #[test]
    fn a_dry_run_reports_the_plan_and_closes_nothing() {
        let mut f = fixture();
        let (grand, worker, boss) = (f.public(f.grand), f.public(f.worker), f.public(f.boss));

        let response = f.close(&[f.boss], false, true);

        assert_eq!(response["result"]["dry_run"], true);
        assert_eq!(
            closed(&response),
            vec![
                (grand, "pane".into()),
                (worker, "tab".into()),
                (boss, "pane".into())
            ]
        );
        assert!(f.alive(f.grand) && f.alive(f.worker) && f.alive(f.boss));
        let pane_id = f.public(f.grand);
        assert!(f.call(Method::PaneGet(PaneTarget { pane_id }))["result"].is_object());
    }
}
