//! Agent waits: the API, and firing a wait as a message to its decider.

use super::responses::{encode_error, encode_error_body, encode_success};
use crate::api::schema::{
    EventData, EventEnvelope, EventKind, MessageKind, ResponseResult, WaitCancelParams,
    WaitDecider, WaitDeclareParams, WaitExtendParams, WaitInfo, WaitListParams, WaitListState,
    WaitTarget,
};
use crate::app::messages::NewMessage;
use crate::app::waits::{
    epoch_now_ms, ChangeError, DeclareError, NewWait, Wait, WaitRecipient, WaitSource, WaitState,
    LIVE_LIMIT, LIVE_PER_WAITER_LIMIT, MAX_EXTENSIONS,
};
use crate::app::App;
use crate::terminal::AgentId;

/// Source kinds `wait.declare` accepts in `on`; advertised as `wait_sources`.
pub(crate) const WAIT_SOURCES: &[&str] = &[];

/// The sender name a fired wait's message carries.
const WAIT_SENDER: &str = "herdr wait";
const SUMMARY_LIMIT: usize = 200;

impl App {
    pub(in crate::app) fn handle_wait_declare(
        &mut self,
        id: String,
        params: WaitDeclareParams,
    ) -> String {
        let summary = params.summary.trim().to_owned();
        if summary.is_empty() || summary.chars().count() > SUMMARY_LIMIT {
            return invalid_wait(id, "wait summary must be 1 to 200 characters");
        }
        if let Some(on) = &params.on {
            if !WAIT_SOURCES.contains(&on.kind.as_str()) {
                return encode_error(
                    id,
                    "wait_source_unsupported",
                    format!(
                        "this server cannot wait on {:?} sources; wait_sources lists the kinds it can",
                        on.kind
                    ),
                );
            }
        } else if params.until.is_some() {
            return invalid_wait(id, "until needs on: it names a state of the source");
        }
        let now = epoch_now_ms();
        let deadline_at_ms = match deadline(params.deadline_at_ms, params.deadline_in_ms, now) {
            Ok(deadline) => deadline,
            Err(message) => return invalid_wait(id, message),
        };
        let waiter = match self.message_agent_id(&params.waiter) {
            Ok(agent_id) => agent_id,
            Err(err) => return encode_error_body(id, self.agent_parent_error_body(err)),
        };
        let declared = self.state.waits.declare(NewWait {
            waiter: waiter.clone(),
            on: params.on.map(|on| WaitSource {
                kind: on.kind,
                target: on.target,
            }),
            until: params.until,
            summary,
            deadline_at_ms,
            then: params.then,
            rearm: params.rearm,
            created_at_ms: now,
        });
        let wait = match declared {
            Ok(wait) => wait.info(),
            Err(err) => return declare_error(id, err, params.rearm, &waiter),
        };
        self.waits_changed(&waiter);
        self.emit_event(EventEnvelope {
            event: EventKind::WaitDeclared,
            data: EventData::WaitDeclared { wait: wait.clone() },
        });
        encode_success(id, ResponseResult::WaitInfo { wait })
    }

    pub(in crate::app) fn handle_wait_list(
        &mut self,
        id: String,
        params: WaitListParams,
    ) -> String {
        let waiter = match params
            .waiter
            .as_deref()
            .map(|target| self.existing_agent_id(target))
        {
            Some(Ok(agent_id)) => Some(agent_id),
            Some(Err(err)) => return encode_error_body(id, self.agent_parent_error_body(err)),
            None => None,
        };
        let waits: Vec<WaitInfo> = self
            .state
            .waits
            .iter()
            .filter(|wait| {
                // A filter naming an agent without an id matches nothing.
                waiter
                    .as_ref()
                    .is_none_or(|waiter| waiter.as_ref() == Some(&wait.waiter))
                    && state_matches(params.state, &wait.state)
            })
            .map(Wait::info)
            .collect();
        encode_success(id, ResponseResult::WaitList { waits })
    }

    pub(in crate::app) fn handle_wait_get(&mut self, id: String, target: WaitTarget) -> String {
        match self.state.waits.get(target.wait_id) {
            Some(wait) => encode_success(id, ResponseResult::WaitInfo { wait: wait.info() }),
            None => wait_not_found(id, target.wait_id),
        }
    }

    pub(in crate::app) fn handle_wait_extend(
        &mut self,
        id: String,
        params: WaitExtendParams,
    ) -> String {
        if let Err(response) = self.check_wait_asker(&id, params.wait_id, params.by.as_deref()) {
            return response;
        }
        let deadline_at_ms =
            match deadline(params.deadline_at_ms, params.deadline_in_ms, epoch_now_ms()) {
                Ok(deadline) => deadline,
                Err(message) => return invalid_wait(id, message),
            };
        match self.state.waits.extend(params.wait_id, deadline_at_ms) {
            Ok(wait) => {
                let (wait, waiter) = (wait.info(), wait.waiter.clone());
                self.waits_changed(&waiter);
                encode_success(id, ResponseResult::WaitInfo { wait })
            }
            Err(err) => change_error(id, err, params.wait_id),
        }
    }

    pub(in crate::app) fn handle_wait_cancel(
        &mut self,
        id: String,
        params: WaitCancelParams,
    ) -> String {
        if let Err(response) = self.check_wait_asker(&id, params.wait_id, params.by.as_deref()) {
            return response;
        }
        let was_live = self
            .state
            .waits
            .get(params.wait_id)
            .is_some_and(Wait::is_live);
        match self.state.waits.cancel(params.wait_id, epoch_now_ms()) {
            Ok(wait) => {
                let (wait, waiter) = (wait.info(), wait.waiter.clone());
                self.waits_changed(&waiter);
                if was_live {
                    self.emit_event(EventEnvelope {
                        event: EventKind::WaitFinished,
                        data: EventData::WaitFinished { wait: wait.clone() },
                    });
                }
                encode_success(id, ResponseResult::WaitInfo { wait })
            }
            Err(err) => change_error(id, err, params.wait_id),
        }
    }

    /// Fires every live wait whose deadline has passed. Cheap when none is due: one index lookup.
    pub(crate) fn fire_due_waits(&mut self) {
        let now = epoch_now_ms();
        if self
            .state
            .waits
            .next_deadline_ms()
            .is_none_or(|deadline| deadline > now)
        {
            return;
        }
        for wait_id in self.state.waits.due(now) {
            self.fire_wait(wait_id, now);
        }
    }

    /// The next deadline as an `Instant`, converted against the wall clock now.
    pub(crate) fn next_wait_deadline(&self, now: std::time::Instant) -> Option<std::time::Instant> {
        let deadline = self.state.waits.next_deadline_ms()?;
        let wait = deadline.saturating_sub(epoch_now_ms());
        // A deadline past what an Instant can hold needs no wake-up: the loop polls anyway.
        now.checked_add(std::time::Duration::from_millis(wait))
    }

    fn fire_wait(&mut self, wait_id: u64, now: u64) {
        let Some(wait) = self.state.waits.get(wait_id).cloned() else {
            return;
        };
        let reason = if wait.deadline_at_ms < self.state.server_started_at_ms {
            "deadline passed while the server was down"
        } else {
            "deadline passed"
        };
        let escalated = wait.extensions >= MAX_EXTENSIONS;
        let mut deciders = vec![wait.then];
        if escalated {
            deciders.extend(one_level_up(wait.then));
        }
        let mut recipients: Vec<WaitRecipient> = Vec::new();
        for decider in deciders {
            let recipient = self.resolve_wait_decider(decider, &wait.waiter);
            if !recipients.contains(&recipient) {
                recipients.push(recipient);
            }
        }
        let Some(fired) = self.state.waits.fire(
            wait_id,
            now,
            reason.to_owned(),
            recipients.clone(),
            escalated,
        ) else {
            return;
        };
        let info = fired.info();
        tracing::info!(wait_id, reason, ?recipients, "wait fired");
        let body = self.wait_message_body(&wait, escalated);
        for recipient in &recipients {
            if let WaitRecipient::Agent(to) = recipient {
                self.queue_message(NewMessage {
                    kind: MessageKind::Tell,
                    from: None,
                    from_name: WAIT_SENDER.to_owned(),
                    to: to.clone(),
                    summary: format!("wait {wait_id} fired: {reason}: {}", wait.summary),
                    body: Some(body.clone()),
                    reply_to: None,
                    sent_at_ms: now,
                    wait_id: Some(wait_id),
                });
            }
        }
        self.waits_changed(&wait.waiter);
        self.emit_event(EventEnvelope {
            event: EventKind::WaitFired,
            data: EventData::WaitFired { wait: info },
        });
    }

    /// `self` is the waiter; `parent` its parent now, or the user when it has none.
    fn resolve_wait_decider(&self, decider: WaitDecider, waiter: &AgentId) -> WaitRecipient {
        match decider {
            WaitDecider::Waiter => WaitRecipient::Agent(waiter.clone()),
            WaitDecider::Parent => self
                .agent_parent(waiter)
                .map_or(WaitRecipient::User, WaitRecipient::Agent),
            WaitDecider::User => WaitRecipient::User,
        }
    }

    fn agent_parent(&self, agent_id: &AgentId) -> Option<AgentId> {
        self.terminal_with_agent_id(agent_id)
            .and_then(|terminal_id| self.state.terminals.get(&terminal_id))
            .and_then(|terminal| terminal.parent_agent_id.clone())
    }

    fn wait_message_body(&self, wait: &Wait, escalated: bool) -> String {
        let waiter = self
            .terminal_with_agent_id(&wait.waiter)
            .and_then(|terminal_id| self.state.terminals.get(&terminal_id))
            .and_then(|terminal| terminal.agent_name.clone())
            .map_or_else(
                || wait.waiter.to_string(),
                |name| format!("{name} ({})", wait.waiter),
            );
        let then = match wait.then {
            WaitDecider::Waiter => "self",
            WaitDecider::Parent => "parent",
            WaitDecider::User => "user",
        };
        let mut body = format!(
            "waiter: {waiter}\nthen: {then}\nextensions: {} of {MAX_EXTENSIONS}",
            wait.extensions
        );
        if escalated {
            body.push_str(", spent: escalated one level up");
        }
        body.push_str(&format!(
            "\n\nre-arm with `herdr wait declare {} --rearm {} --summary … --deadline …`, or see `herdr wait show {}`",
            wait.waiter, wait.id, wait.id
        ));
        body
    }

    /// The waiter, its parent and the user may change a wait. A pane without an agent asks as
    /// the user, since that is a person at a shell.
    fn check_wait_asker(&self, id: &str, wait_id: u64, by: Option<&str>) -> Result<(), String> {
        let Some(wait) = self.state.waits.get(wait_id) else {
            return Err(wait_not_found(id.to_owned(), wait_id));
        };
        let Some(target) = by else {
            return Ok(());
        };
        let asker = match self.existing_agent_id(target) {
            Ok(Some(agent_id)) => agent_id,
            Ok(None) => return Ok(()),
            Err(err) => {
                return Err(encode_error_body(
                    id.to_owned(),
                    self.agent_parent_error_body(err),
                ))
            }
        };
        if asker == wait.waiter || self.agent_parent(&wait.waiter).as_ref() == Some(&asker) {
            return Ok(());
        }
        Err(encode_error(
            id.to_owned(),
            "wait_not_permitted",
            format!(
                "agent {asker} may not change wait {wait_id}: only its waiter, the waiter's parent or the user may"
            ),
        ))
    }

    /// Cancels the live waits of agents that are gone; run where gone agents are found.
    pub(in crate::app) fn cancel_waits_of_gone_agents(
        &mut self,
        live: &std::collections::HashSet<AgentId>,
    ) {
        let (cancelled, changed) = self.state.waits.cancel_waiters_not_in(live, epoch_now_ms());
        if !changed {
            return;
        }
        if !cancelled.is_empty() {
            tracing::info!(?cancelled, "cancelled the waits of agents that are gone");
        }
        self.state.mark_session_dirty();
        self.schedule_session_save();
        for wait_id in cancelled {
            if let Some(wait) = self.state.waits.get(wait_id) {
                let wait = wait.info();
                self.emit_event(EventEnvelope {
                    event: EventKind::WaitFinished,
                    data: EventData::WaitFinished { wait },
                });
            }
        }
    }

    /// The waiter reported, so what its fired waits needed from the user is settled.
    pub(in crate::app) fn clear_wait_needs_user(&mut self, waiter: &AgentId) -> bool {
        let cleared = self.state.waits.clear_needs_user(waiter);
        if cleared {
            self.state.mark_session_dirty();
            self.schedule_session_save();
        }
        cleared
    }

    /// Saves, and republishes the waiter's pane: its `live_waits` or `wait_needs_user` moved.
    fn waits_changed(&mut self, waiter: &AgentId) {
        self.state.mark_session_dirty();
        self.schedule_session_save();
        if let Some((ws_idx, pane_id)) = self
            .terminal_with_agent_id(waiter)
            .and_then(|terminal_id| self.pane_for_terminal(&terminal_id))
        {
            self.emit_pane_updated(ws_idx, pane_id);
        }
    }
}

fn one_level_up(decider: WaitDecider) -> Option<WaitDecider> {
    match decider {
        WaitDecider::Waiter => Some(WaitDecider::Parent),
        WaitDecider::Parent => Some(WaitDecider::User),
        WaitDecider::User => None,
    }
}

fn state_matches(filter: WaitListState, state: &WaitState) -> bool {
    matches!(
        (filter, state),
        (WaitListState::All, _)
            | (WaitListState::Live, WaitState::Live)
            | (WaitListState::Fired, WaitState::Fired { .. })
            | (WaitListState::Satisfied, WaitState::Satisfied { .. })
            | (WaitListState::Cancelled, WaitState::Cancelled { .. })
    )
}

/// Exactly one of an absolute deadline and one relative to now, and not already passed.
fn deadline(at_ms: Option<u64>, in_ms: Option<u64>, now: u64) -> Result<u64, &'static str> {
    let deadline = match (at_ms, in_ms) {
        (Some(at), None) => at,
        (None, Some(after)) => now.saturating_add(after),
        _ => return Err("give exactly one of deadline_at_ms and deadline_in_ms"),
    };
    if deadline <= now {
        return Err("the deadline has already passed");
    }
    Ok(deadline)
}

fn declare_error(id: String, err: DeclareError, rearm: Option<u64>, waiter: &AgentId) -> String {
    let rearm = rearm.unwrap_or_default();
    match err {
        DeclareError::WaiterLimit => encode_error(
            id,
            "wait_limit",
            format!("agent {waiter} already has {LIVE_PER_WAITER_LIMIT} live waits"),
        ),
        DeclareError::SessionLimit => encode_error(
            id,
            "wait_limit",
            format!("the session already has {LIVE_LIMIT} live waits"),
        ),
        DeclareError::MissingDecider => invalid_wait(
            id,
            "then is required: self, parent or user (a re-arm inherits it)",
        ),
        DeclareError::RearmNotFound => wait_not_found(id, rearm),
        DeclareError::RearmNotFired => invalid_wait(
            id,
            format!("wait {rearm} has not fired, so it cannot be re-armed"),
        ),
        DeclareError::RearmOtherWaiter => invalid_wait(
            id,
            format!("wait {rearm} belongs to another agent; only its waiter can re-arm it"),
        ),
        DeclareError::RearmDeciderMismatch => invalid_wait(
            id,
            format!("a re-arm inherits then from wait {rearm}; leave then out or repeat it"),
        ),
    }
}

fn change_error(id: String, err: ChangeError, wait_id: u64) -> String {
    match err {
        ChangeError::NotFound => wait_not_found(id, wait_id),
        ChangeError::NotLive => encode_error(
            id,
            "wait_not_live",
            format!("wait {wait_id} has already finished"),
        ),
        ChangeError::ExtensionLimit => encode_error(
            id,
            "wait_extension_limit",
            format!(
                "wait {wait_id} has used its {MAX_EXTENSIONS} extensions; let it fire, or cancel it"
            ),
        ),
    }
}

fn invalid_wait(id: String, message: impl Into<String>) -> String {
    encode_error(id, "invalid_wait", message.into())
}

fn wait_not_found(id: String, wait_id: u64) -> String {
    encode_error(id, "wait_not_found", format!("wait {wait_id} not found"))
}

#[cfg(test)]
mod tests {
    use crate::api::schema::{
        EventData, EventKind, MessageKind, MessageSendParams, Method, PaneTarget, Request,
        TaskStatus, WaitCancelParams, WaitDecider, WaitDeclareParams, WaitExtendParams,
        WaitSourceInfo,
    };
    use crate::app::waits::{epoch_now_ms, NewWait, WaitRecipient, WaitState};
    use crate::app::App;
    use crate::detect::{Agent, AgentState};
    use crate::layout::PaneId;
    use crate::terminal::AgentId;

    const EMPTY: &str = "\u{276f} ";
    const LATER: u64 = 3_600_000;

    struct Fixture {
        app: App,
        events: crate::api::EventHub,
        worker: PaneId,
        lead: PaneId,
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
        let mut workspace = crate::workspace::Workspace::test_new("waits");
        let worker = workspace.tabs[0].root_pane;
        let lead = workspace.test_split(ratatui::layout::Direction::Horizontal);
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        let mut f = Fixture {
            app,
            events,
            worker,
            lead,
        };
        f.agent(worker, Agent::Claude, "worker");
        f.agent(lead, Agent::Pi, "lead");
        f.app.sync_agent_identities();
        f
    }

    impl Fixture {
        fn agent(&mut self, pane: PaneId, agent: Agent, name: &str) {
            let terminal = self.terminal(pane);
            terminal.set_detected_state(Some(agent), AgentState::Working);
            terminal.set_agent_name(name.into());
        }

        fn terminal(&mut self, pane: PaneId) -> &mut crate::terminal::TerminalState {
            let terminal_id = self.app.state.terminal_id_for_pane(0, pane).unwrap();
            self.app.state.terminals.get_mut(&terminal_id).unwrap()
        }

        fn id(&mut self, pane: PaneId) -> AgentId {
            self.terminal(pane).agent_id.clone().unwrap()
        }

        fn make_lead_parent(&mut self) {
            let lead = self.id(self.lead);
            self.terminal(self.worker).parent_agent_id = Some(lead);
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

        fn declare(&mut self, then: Option<WaitDecider>) -> serde_json::Value {
            let waiter = self.public(self.worker);
            self.call(Method::WaitDeclare(WaitDeclareParams {
                waiter,
                summary: "CI on !12".into(),
                deadline_at_ms: None,
                deadline_in_ms: Some(LATER),
                then,
                on: None,
                until: None,
                rearm: None,
            }))
        }

        /// The worker's wait, its deadline already passed.
        fn due_wait(&mut self, then: WaitDecider, extensions: u8) -> u64 {
            let waiter = self.id(self.worker);
            let waits = &mut self.app.state.waits;
            let id = waits
                .declare(NewWait {
                    waiter,
                    on: None,
                    until: None,
                    summary: "CI on !12".into(),
                    deadline_at_ms: epoch_now_ms() + LATER,
                    then: Some(then),
                    rearm: None,
                    created_at_ms: 1,
                })
                .unwrap()
                .id;
            for _ in 0..extensions {
                waits.extend(id, epoch_now_ms() + LATER).unwrap();
            }
            waits.set_deadline_for_test(id, epoch_now_ms() - 1);
            id
        }

        /// Messages queued so far: (to, summary, wait_id).
        fn messages(&self) -> Vec<(AgentId, String, Option<u64>)> {
            self.app
                .state
                .messages
                .iter()
                .map(|message| (message.to.clone(), message.summary.clone(), message.wait_id))
                .collect()
        }

        fn fired_to(&self, id: u64) -> (Vec<WaitRecipient>, bool) {
            match &self.app.state.waits.get(id).unwrap().state {
                WaitState::Fired { to, escalated, .. } => (to.clone(), *escalated),
                state => panic!("wait {id} has not fired: {state:?}"),
            }
        }

        fn event_kinds(&self) -> Vec<EventKind> {
            self.events
                .events_after(0)
                .into_iter()
                .map(|(_, event)| event.event)
                .filter(|kind| {
                    matches!(
                        kind,
                        EventKind::WaitDeclared
                            | EventKind::WaitFired
                            | EventKind::WaitFinished
                            | EventKind::MessageQueued
                    )
                })
                .collect()
        }
    }

    #[test]
    fn a_due_wait_fires_a_tell_to_the_waiter_through_the_message_store() {
        let mut f = fixture();
        let worker = f.id(f.worker);
        let id = f.due_wait(WaitDecider::Waiter, 0);

        f.app.fire_due_waits();

        assert_eq!(
            f.messages(),
            vec![(
                worker.clone(),
                format!("wait {id} fired: deadline passed: CI on !12"),
                Some(id)
            )]
        );
        let message = f.app.state.messages.iter().next().unwrap();
        assert_eq!(message.kind, MessageKind::Tell);
        assert_eq!(
            (message.from.as_ref(), message.from_name.as_str()),
            (None, "herdr wait")
        );
        assert!(message.body.as_deref().unwrap().contains("--rearm"));
        assert_eq!(f.fired_to(id), (vec![WaitRecipient::Agent(worker)], false));
        assert_eq!(f.app.state.waits.next_deadline_ms(), None);
        assert_eq!(
            f.event_kinds(),
            vec![EventKind::MessageQueued, EventKind::WaitFired]
        );
        f.app.fire_due_waits();
        assert_eq!(f.messages().len(), 1, "a wait fires once");
        f.app.state.assert_invariants_for_test();
    }

    #[test]
    fn nothing_fires_before_the_deadline_and_the_loop_wakes_for_it() {
        let mut f = fixture();
        let declared = f.declare(Some(WaitDecider::Waiter));
        assert_eq!(declared["result"]["wait"]["state"], "live", "{declared}");
        f.app.fire_due_waits();
        assert!(f.messages().is_empty());

        let now = std::time::Instant::now();
        let wake = f.app.next_wait_deadline(now).unwrap();
        assert!(wake > now + std::time::Duration::from_secs(3_500));
        assert!(wake <= now + std::time::Duration::from_secs(3_600));
        assert!(
            f.app
                .next_headless_loop_deadline_with_git_refresh(now, false, false)
                <= Some(wake)
        );
    }

    #[test]
    fn a_deadline_beyond_any_instant_does_not_panic_the_loop() {
        let mut f = fixture();
        let declared = f.call(Method::WaitDeclare(WaitDeclareParams {
            waiter: f.public(f.worker),
            summary: "forever".into(),
            deadline_at_ms: Some(u64::MAX),
            deadline_in_ms: None,
            then: Some(WaitDecider::Waiter),
            on: None,
            until: None,
            rearm: None,
        }));
        assert_eq!(declared["result"]["wait"]["state"], "live", "{declared}");
        let now = std::time::Instant::now();
        let day = std::time::Duration::from_secs(86_400);
        assert!(f
            .app
            .next_wait_deadline(now)
            .is_none_or(|wake| wake > now + day));
        f.app
            .next_headless_loop_deadline_with_git_refresh(now, false, false);
        f.app.fire_due_waits();
        assert!(f.messages().is_empty());
    }

    #[tokio::test]
    async fn a_fire_joins_the_recipients_backlog_as_one_block() {
        let mut f = fixture();
        let (runtime, mut typed) = crate::terminal::TerminalRuntime::test_with_channel(80, 24);
        let rule = "\u{2500}".repeat(40);
        runtime.test_process_pty_bytes(
            format!("\u{25cf} Done.\r\n\r\n{rule}\r\n{EMPTY}\r\n{rule}\r\n  ~/repo\r\n").as_bytes(),
        );
        f.app.state.insert_test_runtime(f.worker, runtime);
        let to = f.public(f.worker);
        let sent = f.call(Method::MessageSend(MessageSendParams {
            to,
            summary: "heads up".into(),
            kind: MessageKind::Tell,
            body: None,
            from: None,
            reply_to: None,
        }));
        assert_eq!(sent["result"]["message"]["state"], "pending", "{sent}");
        let id = f.due_wait(WaitDecider::Waiter, 0);
        f.app.fire_due_waits();
        assert!(typed.try_recv().is_err(), "nothing typed while it works");

        let seq = f.app.state.next_agent_state_change_seq + 1;
        f.app.state.next_agent_state_change_seq = seq;
        let terminal = f.terminal(f.worker);
        terminal.set_detected_state(Some(Agent::Claude), AgentState::Idle);
        terminal.last_agent_state_change_seq = Some(seq);
        f.app.deliver_pending_messages();

        let text = typed.recv().await.expect("a block is typed");
        assert_eq!(&typed.recv().await.expect("and submitted")[..], b"\r");
        assert_eq!(
            String::from_utf8_lossy(&text),
            format!(
                "[herdr] 2 messages, oldest first: \
                 [from user] heads up (tell, just now, herdr message show 1) \
                 [from herdr wait] wait {id} fired: deadline passed: CI on !12 (tell, just now, herdr message show 2)"
            )
        );
    }

    #[test]
    fn the_parent_is_resolved_when_the_wait_fires() {
        let mut f = fixture();
        let declared = f.declare(Some(WaitDecider::Parent));
        let id = declared["result"]["wait"]["wait_id"].as_u64().unwrap();
        f.make_lead_parent();
        f.app.state.waits.set_deadline_for_test(id, 1);

        f.app.fire_due_waits();

        let lead = f.id(f.lead);
        assert_eq!(f.messages().len(), 1);
        assert_eq!(f.messages()[0].0, lead.clone());
        assert_eq!(f.fired_to(id), (vec![WaitRecipient::Agent(lead)], false));
        assert!(f
            .app
            .agent_info(0, f.worker)
            .unwrap()
            .wait_needs_user
            .is_none());
    }

    #[test]
    fn a_parentless_parent_wait_flags_the_user_until_the_waiter_reports() {
        let mut f = fixture();
        let id = f.due_wait(WaitDecider::Parent, 0);

        f.app.fire_due_waits();

        assert!(
            f.messages().is_empty(),
            "the user has no inbox: the flag is the fire"
        );
        assert_eq!(f.fired_to(id), (vec![WaitRecipient::User], false));
        let flag = f
            .app
            .agent_info(0, f.worker)
            .unwrap()
            .wait_needs_user
            .unwrap();
        assert_eq!(
            (flag.wait_id, flag.reason.as_str()),
            (id, "deadline passed")
        );
        assert_eq!(f.app.agent_info(0, f.worker).unwrap().task_status, None);

        let worker = f.public(f.worker);
        f.app
            .set_agent_task_status(&worker, TaskStatus::Running)
            .ok()
            .unwrap();
        assert!(f
            .app
            .agent_info(0, f.worker)
            .unwrap()
            .wait_needs_user
            .is_none());
        f.app.state.assert_invariants_for_test();
    }

    #[test]
    fn spent_extensions_escalate_one_level_up() {
        let mut f = fixture();
        f.make_lead_parent();
        let (worker, lead) = (f.id(f.worker), f.id(f.lead));

        let to_self = f.due_wait(WaitDecider::Waiter, 2);
        f.app.fire_due_waits();
        assert_eq!(
            f.fired_to(to_self),
            (
                vec![
                    WaitRecipient::Agent(worker.clone()),
                    WaitRecipient::Agent(lead.clone())
                ],
                true
            )
        );
        let recipients: Vec<AgentId> = f.messages().into_iter().map(|(to, ..)| to).collect();
        assert_eq!(recipients, vec![worker, lead.clone()]);

        let to_parent = f.due_wait(WaitDecider::Parent, 2);
        f.app.fire_due_waits();
        assert_eq!(
            f.fired_to(to_parent),
            (vec![WaitRecipient::Agent(lead), WaitRecipient::User], true)
        );
        assert_eq!(
            f.app
                .agent_info(0, f.worker)
                .unwrap()
                .wait_needs_user
                .map(|flag| flag.wait_id),
            Some(to_parent)
        );

        let unspent = f.due_wait(WaitDecider::Parent, 1);
        f.app.fire_due_waits();
        assert!(
            !f.fired_to(unspent).1,
            "one extension left is no escalation"
        );
    }

    #[test]
    fn a_deadline_missed_while_the_server_was_down_fires_on_load_and_says_so() {
        let mut before = fixture();
        let id = before.due_wait(WaitDecider::Waiter, 0);
        let saved = serde_json::to_string(&before.app.state.waits).unwrap();

        let mut f = fixture();
        f.app.state.waits = serde_json::from_str(&saved).unwrap();
        f.app.state.server_started_at_ms = epoch_now_ms();
        let worker = before.id(before.worker);
        f.terminal(f.worker).agent_id = Some(worker);
        f.app.fire_due_waits();

        assert_eq!(
            f.messages()[0].1,
            format!("wait {id} fired: deadline passed while the server was down: CI on !12")
        );
        f.app.state.assert_invariants_for_test();
    }

    #[test]
    fn a_waiter_that_closes_has_its_live_waits_cancelled_silently() {
        let mut f = fixture();
        f.make_lead_parent();
        let declared = f.declare(Some(WaitDecider::Parent));
        let id = declared["result"]["wait"]["wait_id"].as_u64().unwrap();
        assert_eq!(f.app.agent_info(0, f.worker).unwrap().live_waits, Some(1));

        let pane_id = f.public(f.worker);
        let closed = f.call(Method::PaneClose(PaneTarget { pane_id }));
        assert_eq!(closed["result"]["type"], "ok", "{closed}");
        f.app.sync_agent_identities();

        assert!(matches!(
            f.app.state.waits.get(id).unwrap().state,
            WaitState::Cancelled { .. }
        ));
        assert!(f.messages().is_empty(), "nobody is told");
        assert_eq!(
            f.event_kinds(),
            vec![EventKind::WaitDeclared, EventKind::WaitFinished]
        );
        f.app.state.assert_invariants_for_test();
    }

    #[test]
    fn only_the_waiter_its_parent_or_the_user_may_extend_or_cancel() {
        let mut f = fixture();
        let declared = f.declare(Some(WaitDecider::Waiter));
        let id = declared["result"]["wait"]["wait_id"].as_u64().unwrap();
        let (worker, lead) = (f.public(f.worker), f.public(f.lead));
        let extend = |by: Option<&str>| {
            Method::WaitExtend(WaitExtendParams {
                wait_id: id,
                deadline_at_ms: None,
                deadline_in_ms: Some(LATER),
                by: by.map(str::to_owned),
            })
        };

        let refused = f.call(extend(Some(&lead)));
        assert_eq!(refused["error"]["code"], "wait_not_permitted", "{refused}");
        f.make_lead_parent();
        assert_eq!(
            f.call(extend(Some(&lead)))["result"]["wait"]["extensions"],
            1
        );
        assert_eq!(
            f.call(extend(Some(&worker)))["result"]["wait"]["extensions"],
            2
        );
        let spent = f.call(extend(None));
        assert_eq!(spent["error"]["code"], "wait_extension_limit", "{spent}");

        let cancelled = f.call(Method::WaitCancel(WaitCancelParams {
            wait_id: id,
            by: None,
        }));
        assert_eq!(
            cancelled["result"]["wait"]["state"], "cancelled",
            "{cancelled}"
        );
        let again = f.call(Method::WaitCancel(WaitCancelParams {
            wait_id: id,
            by: None,
        }));
        assert_eq!(again["error"]["code"], "wait_not_live");
        assert!(f.event_kinds().contains(&EventKind::WaitFinished));
    }

    #[test]
    fn declare_refuses_what_pr1_cannot_keep() {
        let mut f = fixture();
        let waiter = f.public(f.worker);
        let declare = |change: &dyn Fn(&mut WaitDeclareParams)| {
            let mut params = WaitDeclareParams {
                waiter: waiter.clone(),
                summary: "CI".into(),
                deadline_at_ms: None,
                deadline_in_ms: Some(LATER),
                then: Some(WaitDecider::Waiter),
                on: None,
                until: None,
                rearm: None,
            };
            change(&mut params);
            Method::WaitDeclare(params)
        };
        for (change, code) in [
            (
                &(|p: &mut WaitDeclareParams| {
                    p.on = Some(WaitSourceInfo {
                        kind: "node".into(),
                        target: "1234abcd".into(),
                    })
                }) as &dyn Fn(&mut WaitDeclareParams),
                "wait_source_unsupported",
            ),
            (
                &|p: &mut WaitDeclareParams| p.until = Some("done".into()),
                "invalid_wait",
            ),
            (&|p: &mut WaitDeclareParams| p.then = None, "invalid_wait"),
            (
                &|p: &mut WaitDeclareParams| p.summary = " ".into(),
                "invalid_wait",
            ),
            (
                &|p: &mut WaitDeclareParams| p.summary = "x".repeat(201),
                "invalid_wait",
            ),
            (
                &|p: &mut WaitDeclareParams| p.deadline_in_ms = None,
                "invalid_wait",
            ),
            (
                &|p: &mut WaitDeclareParams| p.deadline_at_ms = Some(5),
                "invalid_wait",
            ),
            (
                &|p: &mut WaitDeclareParams| {
                    p.deadline_in_ms = None;
                    p.deadline_at_ms = Some(5);
                },
                "invalid_wait",
            ),
            (
                &|p: &mut WaitDeclareParams| p.rearm = Some(9),
                "wait_not_found",
            ),
        ] {
            let response = f.call(declare(change));
            assert_eq!(response["error"]["code"], code, "{response}");
        }
        assert!(f.app.state.waits.is_empty());
    }

    #[test]
    fn a_rearm_inherits_extensions_and_then_and_clears_the_users_flag() {
        let mut f = fixture();
        let fired = f.due_wait(WaitDecider::User, 2);
        f.app.fire_due_waits();
        assert!(f
            .app
            .agent_info(0, f.worker)
            .unwrap()
            .wait_needs_user
            .is_some());

        let waiter = f.public(f.worker);
        let rearmed = f.call(Method::WaitDeclare(WaitDeclareParams {
            waiter,
            summary: "CI on !12, again".into(),
            deadline_at_ms: None,
            deadline_in_ms: Some(LATER),
            then: None,
            on: None,
            until: None,
            rearm: Some(fired),
        }));
        let wait = &rearmed["result"]["wait"];
        assert_eq!(wait["then"], "user", "{rearmed}");
        assert_eq!(wait["extensions"], 2);
        assert_eq!(wait["rearm_of"], fired);
        assert_ne!(wait["wait_id"], fired);
        assert!(f
            .app
            .agent_info(0, f.worker)
            .unwrap()
            .wait_needs_user
            .is_none());
        let fired_info = f.call(Method::WaitGet(crate::api::schema::WaitTarget {
            wait_id: fired,
        }));
        assert_eq!(fired_info["result"]["wait"]["fired"]["to"][0], "user");
    }

    #[test]
    fn events_carry_the_wait() {
        let mut f = fixture();
        f.declare(Some(WaitDecider::Waiter));
        let declared = f
            .events
            .events_after(0)
            .into_iter()
            .find_map(|(_, event)| match event.data {
                EventData::WaitDeclared { wait } => Some(wait),
                _ => None,
            })
            .unwrap();
        assert_eq!(declared.summary, "CI on !12");
        assert_eq!(declared.waiter_agent_id, f.id(f.worker).to_string());
    }
}
