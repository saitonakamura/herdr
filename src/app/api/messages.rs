//! Messages between agents: the API and delivery into the recipient's pane.

use super::agents::AgentSubmissionError;
use super::responses::{encode_error, encode_error_body, encode_success};
use crate::api::schema::{
    EventData, EventEnvelope, EventKind, MessageInfo, MessageListParams, MessageSendParams,
    MessageTarget, ResponseResult,
};
use crate::app::agent_identity::AgentParentError;
use crate::app::messages::{delivery_block, NewMessage, WithdrawError};
use crate::app::App;
use crate::detect::{AgentState, BlockedReason};
use crate::terminal::{AgentId, TerminalId};

/// The name a message from the user carries.
const USER_SENDER: &str = "user";

impl App {
    pub(in crate::app) fn handle_message_send(
        &mut self,
        id: String,
        params: MessageSendParams,
    ) -> String {
        let summary = params.summary.trim().to_owned();
        if summary.is_empty() {
            return encode_error(id, "empty_message", "message summary must not be empty");
        }
        if let Some(reply_to) = params.reply_to {
            if self.state.messages.get(reply_to).is_none() {
                return message_not_found(id, reply_to);
            }
        }
        let to = match self.message_agent_id(&params.to) {
            Ok(agent_id) => agent_id,
            Err(err) => return encode_error_body(id, self.agent_parent_error_body(err)),
        };
        let (from, from_name) = match &params.from {
            Some(target) => match self.message_agent_id(target) {
                Ok(agent_id) => {
                    let name = self.agent_display_name(&agent_id);
                    (Some(agent_id), name)
                }
                Err(err) => return encode_error_body(id, self.agent_parent_error_body(err)),
            },
            None => (None, USER_SENDER.to_owned()),
        };
        let body = params.body.filter(|body| !body.trim().is_empty());
        let message_id = self.queue_message(NewMessage {
            kind: params.kind,
            from,
            from_name,
            to,
            summary,
            body,
            reply_to: params.reply_to,
            sent_at_ms: now_ms(),
            wait_id: None,
        });
        match self.state.messages.get(message_id) {
            Some(message) => encode_success(
                id,
                ResponseResult::MessageInfo {
                    message: message.info(),
                },
            ),
            None => message_not_found(id, message_id),
        }
    }

    pub(in crate::app) fn handle_message_list(
        &mut self,
        id: String,
        params: MessageListParams,
    ) -> String {
        let to = match params
            .to
            .as_deref()
            .map(|target| self.existing_agent_id(target))
        {
            Some(Ok(agent_id)) => Some(agent_id),
            Some(Err(err)) => return encode_error_body(id, self.agent_parent_error_body(err)),
            None => None,
        };
        let from = match params
            .from
            .as_deref()
            .map(|target| self.existing_agent_id(target))
        {
            Some(Ok(agent_id)) => Some(agent_id),
            Some(Err(err)) => return encode_error_body(id, self.agent_parent_error_body(err)),
            None => None,
        };
        let messages: Vec<MessageInfo> = self
            .state
            .messages
            .iter()
            .filter(|message| {
                // A filter naming an agent without an id matches nothing.
                to.as_ref()
                    .is_none_or(|to| to.as_ref() == Some(&message.to))
                    && from
                        .as_ref()
                        .is_none_or(|from| from.is_some() && message.from == *from)
                    && (!params.pending || message.is_pending())
            })
            .map(|message| message.info())
            .collect();
        encode_success(id, ResponseResult::MessageList { messages })
    }

    pub(in crate::app) fn handle_message_get(
        &mut self,
        id: String,
        target: MessageTarget,
    ) -> String {
        match self.state.messages.get(target.message_id) {
            Some(message) => encode_success(
                id,
                ResponseResult::MessageInfo {
                    message: message.info(),
                },
            ),
            None => message_not_found(id, target.message_id),
        }
    }

    pub(in crate::app) fn handle_message_withdraw(
        &mut self,
        id: String,
        target: MessageTarget,
    ) -> String {
        match self.state.messages.withdraw(target.message_id) {
            Ok(message) => {
                self.messages_changed();
                encode_success(
                    id,
                    ResponseResult::MessageInfo {
                        message: message.info(),
                    },
                )
            }
            Err(WithdrawError::NotFound) => message_not_found(id, target.message_id),
            Err(WithdrawError::Delivered) => encode_error(
                id,
                "message_delivered",
                format!(
                    "message {} was already delivered and cannot be withdrawn",
                    target.message_id
                ),
            ),
        }
    }

    /// Queues a message and delivers it at once when its recipient can take it. Everything that
    /// sends a message goes through here, so it batches with whatever else is pending.
    pub(super) fn queue_message(&mut self, new: NewMessage) -> u64 {
        let to = new.to.clone();
        let message_id = self.state.messages.push(new).id;
        self.messages_changed();
        if let Some(message) = self.state.messages.get(message_id) {
            let message = message.info();
            self.emit_event(EventEnvelope {
                event: EventKind::MessageQueued,
                data: EventData::MessageQueued { message },
            });
        }
        self.try_deliver_message_to(&to);
        message_id
    }

    /// Delivers what is due, at most one block per recipient per state change.
    pub(crate) fn deliver_pending_messages(&mut self) {
        self.state.message_delivery_due = false;
        if !self.state.messages.has_pending() {
            self.state.message_delivery_attempts.clear();
            return;
        }
        for recipient in self.state.messages.pending_recipients() {
            self.try_deliver_message_to(&recipient);
        }
    }

    /// Types all of the recipient's pending messages into its pane as one block when it can take
    /// it: idle with no draft in its composer, or blocked on a question with an empty composer.
    fn try_deliver_message_to(&mut self, recipient: &AgentId) {
        let Some(terminal_id) = self.terminal_with_agent_id(recipient) else {
            return;
        };
        let Some((ws_idx, pane_id)) = self.pane_for_terminal(&terminal_id) else {
            return;
        };
        let state_change_seq = self
            .state
            .terminals
            .get(&terminal_id)
            .and_then(|terminal| terminal.last_agent_state_change_seq);
        if self.state.message_delivery_attempts.get(recipient) == Some(&state_change_seq) {
            return;
        }
        let now = now_ms();
        let pending = self.state.messages.pending_for(recipient);
        if pending.is_empty() {
            return;
        }
        let message_ids: Vec<u64> = pending.iter().map(|message| message.id).collect();
        let lines = delivery_block(&pending, recipient, now);
        self.state
            .message_delivery_attempts
            .insert(recipient.clone(), state_change_seq);
        let Some((expected_agent, bracketed_paste)) =
            self.message_delivery_agent(ws_idx, pane_id, &terminal_id)
        else {
            return;
        };
        // Unbracketed, a newline is Enter and would submit the block one line at a time.
        let block = lines.join(if bracketed_paste { "\n" } else { " " });
        match self.queue_agent_submission(ws_idx, pane_id, expected_agent, &block, None) {
            Ok(_completion) => {}
            Err(AgentSubmissionError::NotFound) => return,
            Err(AgentSubmissionError::Failed(err)) => {
                tracing::warn!(?message_ids, err = %err, "message delivery failed");
                return;
            }
        }
        // The block went in whole, so its messages are delivered together. A withdraw runs on this
        // same loop: before now it removed its message from the block, after now it is refused.
        for message_id in message_ids {
            if let Some(message) = self.state.messages.mark_delivered(message_id, now) {
                let message = message.info();
                self.emit_event(EventEnvelope {
                    event: EventKind::MessageDelivered,
                    data: EventData::MessageDelivered { message },
                });
            }
        }
        self.messages_changed();
    }

    /// The agent to type into, and whether its pane takes a bracketed paste, when it can take
    /// messages now.
    fn message_delivery_agent(
        &self,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
        terminal_id: &TerminalId,
    ) -> Option<(crate::detect::Agent, bool)> {
        let terminal = self.state.terminals.get(terminal_id)?;
        let agent = terminal.effective_known_agent()?;
        if terminal.managed_agent_launch_pending() {
            return None;
        }
        let needs_empty_composer = match terminal.state {
            // An awaiting agent waits for exactly this, so it takes messages like an idle one.
            AgentState::Idle | AgentState::Awaiting => false,
            AgentState::Blocked if terminal.blocked_reason() == Some(BlockedReason::Question) => {
                true
            }
            _ => return None,
        };
        let (runtime, _) = self.lookup_runtime(ws_idx, pane_id)?;
        if !crate::app::agents::runtime_hosts_agent(runtime, agent) {
            return None;
        }
        let (screen, _) = crate::app::api_helpers::read_terminal_snapshot(
            runtime,
            crate::api::schema::ReadSource::Screen,
            crate::api::schema::ReadFormat::Ansi,
            None,
        );
        // A draft is the user typing; a line typed over it would submit their half-written prompt.
        match crate::detect::composer::composer_state(&screen.text) {
            crate::detect::composer::Composer::Draft => None,
            crate::detect::composer::Composer::Missing if needs_empty_composer => None,
            _ => Some((agent, runtime.bracketed_paste_enabled())),
        }
    }

    /// Resolves a target to its agent id, giving a pane without one an id.
    pub(super) fn message_agent_id(&mut self, target: &str) -> Result<AgentId, AgentParentError> {
        let (_, _, terminal_id) = self.resolve_agent_identity_target(target)?;
        if self.assign_agent_identity(&terminal_id, None) {
            self.state.mark_session_dirty();
            self.schedule_session_save();
            if let Some((ws_idx, pane_id)) = self.pane_for_terminal(&terminal_id) {
                self.emit_pane_updated(ws_idx, pane_id);
            }
        }
        self.state
            .terminals
            .get(&terminal_id)
            .and_then(|terminal| terminal.agent_id.clone())
            .ok_or_else(|| not_found(target))
    }

    /// Resolves a target to the agent id it already has; a filter never mints one.
    pub(super) fn existing_agent_id(
        &self,
        target: &str,
    ) -> Result<Option<AgentId>, AgentParentError> {
        let (_, _, terminal_id) = self.resolve_agent_identity_target(target)?;
        Ok(self
            .state
            .terminals
            .get(&terminal_id)
            .and_then(|terminal| terminal.agent_id.clone()))
    }

    pub(super) fn agent_display_name(&self, agent_id: &AgentId) -> String {
        self.terminal_with_agent_id(agent_id)
            .and_then(|terminal_id| self.state.terminals.get(&terminal_id))
            .and_then(|terminal| terminal.agent_name.clone())
            .unwrap_or_else(|| agent_id.to_string())
    }

    fn messages_changed(&mut self) {
        self.state.mark_session_dirty();
        self.schedule_session_save();
    }
}

fn not_found(target: &str) -> AgentParentError {
    AgentParentError::Target(
        crate::app::terminal_targets::TerminalTargetError::NotFound {
            target: target.to_string(),
        },
    )
}

fn message_not_found(id: String, message_id: u64) -> String {
    encode_error(
        id,
        "message_not_found",
        format!("message {message_id} not found"),
    )
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
    use crate::api::schema::{
        EventData, EventKind, MessageKind, MessageListParams, MessageSendParams, MessageState,
        MessageTarget, Method, PaneTarget, Request,
    };
    use crate::app::App;
    use crate::detect::{Agent, AgentState, BlockedReason};
    use crate::layout::PaneId;

    const EMPTY: &str = "\u{276f} ";

    struct Fixture {
        app: App,
        events: crate::api::EventHub,
        worker: PaneId,
        lead: PaneId,
    }

    fn screen(composer_row: &str) -> String {
        let rule = "\u{2500}".repeat(40);
        format!("\u{25cf} Done.\r\n\r\n{rule}\r\n{composer_row}\r\n{rule}\r\n  ~/repo\r\n")
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
        let mut workspace = crate::workspace::Workspace::test_new("messages");
        let worker = workspace.tabs[0].root_pane;
        let lead = workspace.test_split(ratatui::layout::Direction::Horizontal);
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        let mut fixture = Fixture {
            app,
            events,
            worker,
            lead,
        };
        fixture.agent(lead, Agent::Pi, AgentState::Idle, "lead");
        fixture
    }

    impl Fixture {
        fn agent(&mut self, pane: PaneId, agent: Agent, state: AgentState, name: &str) {
            let terminal = self.terminal(pane);
            terminal.set_detected_state(Some(agent), state);
            terminal.set_agent_name(name.into());
        }

        fn terminal(&mut self, pane: PaneId) -> &mut crate::terminal::TerminalState {
            let terminal_id = self.app.state.terminal_id_for_pane(0, pane).unwrap();
            self.app.state.terminals.get_mut(&terminal_id).unwrap()
        }

        /// A runtime showing `composer_row` in its prompt box; returns what gets typed into it.
        fn runtime(
            &mut self,
            pane: PaneId,
            composer_row: &str,
        ) -> tokio::sync::mpsc::Receiver<bytes::Bytes> {
            let (runtime, typed) = crate::terminal::TerminalRuntime::test_with_channel(80, 24);
            runtime.test_process_pty_bytes(screen(composer_row).as_bytes());
            self.app.state.insert_test_runtime(pane, runtime);
            typed
        }

        /// The agent's state moves on, as a detected change would.
        fn change_state(&mut self, pane: PaneId, state: AgentState) {
            let seq = self.app.state.next_agent_state_change_seq + 1;
            self.app.state.next_agent_state_change_seq = seq;
            let terminal = self.terminal(pane);
            let agent = terminal.effective_known_agent();
            terminal.set_detected_state(agent, state);
            terminal.last_agent_state_change_seq = Some(seq);
            self.app.state.message_delivery_due = true;
            self.app.deliver_pending_messages();
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

        fn send(&mut self, to: PaneId, summary: &str) -> serde_json::Value {
            let from = Some(self.public(self.worker));
            self.send_from(to, summary, from)
        }

        fn send_from(
            &mut self,
            to: PaneId,
            summary: &str,
            from: Option<String>,
        ) -> serde_json::Value {
            let to = self.public(to);
            self.call(Method::MessageSend(MessageSendParams {
                to,
                summary: summary.into(),
                kind: MessageKind::Report,
                body: Some("the details".into()),
                from,
                reply_to: None,
            }))
        }

        fn state_of(&mut self, message_id: u64) -> String {
            self.call(Method::MessageGet(MessageTarget { message_id }))["result"]["message"]
                ["state"]
                .as_str()
                .unwrap()
                .to_owned()
        }
    }

    async fn typed_line(typed: &mut tokio::sync::mpsc::Receiver<bytes::Bytes>) -> String {
        let text = typed.recv().await.expect("a line is typed");
        let enter = typed.recv().await.expect("and submitted");
        assert_eq!(&enter[..], b"\r");
        String::from_utf8_lossy(&text).into_owned()
    }

    #[tokio::test]
    async fn a_message_to_an_idle_agent_is_typed_at_once_as_one_line() {
        let mut f = fixture();
        f.agent(f.worker, Agent::Claude, AgentState::Working, "worker");
        let mut typed = f.runtime(f.lead, EMPTY);

        let sent = f.send(f.lead, "done: tests pass");

        let message = &sent["result"]["message"];
        assert_eq!(message["state"], "delivered", "{sent}");
        assert_eq!(message["from_name"], "worker");
        assert_eq!(message["kind"], "report");
        assert_eq!(
            typed_line(&mut typed).await,
            "[from worker] done: tests pass (report, just now, herdr message show 1)"
        );
        let kinds: Vec<EventKind> = f
            .events
            .events_after(0)
            .into_iter()
            .map(|(_, event)| event.event)
            .filter(|kind| matches!(kind, EventKind::MessageQueued | EventKind::MessageDelivered))
            .collect();
        assert_eq!(
            kinds,
            vec![EventKind::MessageQueued, EventKind::MessageDelivered]
        );
        let shown = f.call(Method::MessageGet(MessageTarget { message_id: 1 }));
        assert_eq!(shown["result"]["message"]["body"], "the details");
    }

    fn id_of(sent: &serde_json::Value) -> u64 {
        sent["result"]["message"]["message_id"].as_u64().unwrap()
    }

    fn delivered_ids(events: &crate::api::EventHub) -> Vec<u64> {
        events
            .events_after(0)
            .into_iter()
            .filter_map(|(_, event)| match event.data {
                EventData::MessageDelivered { message } => Some(message.message_id),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn a_busy_agent_gets_its_backlog_as_one_block_at_the_next_state_change() {
        let mut f = fixture();
        f.agent(f.worker, Agent::Claude, AgentState::Working, "worker");
        f.change_state(f.lead, AgentState::Working);
        let mut typed = f.runtime(f.lead, EMPTY);

        let first = id_of(&f.send(f.lead, "first"));
        let second = id_of(&f.send(f.lead, "second"));
        assert_eq!(f.state_of(first), "pending");
        assert!(typed.try_recv().is_err(), "nothing typed while it works");

        f.change_state(f.lead, AgentState::Idle);
        assert_eq!(
            typed_line(&mut typed).await,
            "[herdr] 2 messages, oldest first: \
             [from worker] first (report, just now, herdr message show 1) \
             [from worker] second (report, just now, herdr message show 2)",
            "one submission; unbracketed, so joined on one line"
        );
        assert_eq!(f.state_of(first), "delivered");
        assert_eq!(f.state_of(second), "delivered");
        assert_eq!(delivered_ids(&f.events), vec![first, second]);

        let third = id_of(&f.send(f.lead, "third"));
        assert_eq!(f.state_of(third), "pending", "still the same state change");
        assert!(typed.try_recv().is_err());
        f.change_state(f.lead, AgentState::Working);
        f.change_state(f.lead, AgentState::Idle);
        assert_eq!(
            typed_line(&mut typed).await,
            "[from worker] third (report, just now, herdr message show 3)"
        );
    }

    #[tokio::test]
    async fn a_pane_with_bracketed_paste_gets_the_block_a_line_per_message() {
        let mut f = fixture();
        f.agent(f.worker, Agent::Claude, AgentState::Working, "worker");
        f.change_state(f.lead, AgentState::Working);
        let mut typed = f.runtime(f.lead, &format!("\x1b[?2004h{EMPTY}"));
        f.send(f.lead, "first");
        f.send(f.lead, "second");

        f.change_state(f.lead, AgentState::Idle);

        assert_eq!(
            typed_line(&mut typed).await,
            "\x1b[200~[herdr] 2 messages, oldest first:\n\
             [from worker] first (report, just now, herdr message show 1)\n\
             [from worker] second (report, just now, herdr message show 2)\x1b[201~"
        );
    }

    #[tokio::test]
    async fn a_withdraw_takes_its_message_out_of_the_block_or_is_refused_after_it() {
        let mut f = fixture();
        f.agent(f.worker, Agent::Claude, AgentState::Working, "worker");
        f.change_state(f.lead, AgentState::Working);
        let mut typed = f.runtime(f.lead, EMPTY);
        let kept = id_of(&f.send(f.lead, "kept"));
        let withdrawn = id_of(&f.send(f.lead, "withdrawn"));
        let late = id_of(&f.send(f.lead, "late"));

        let taken = f.call(Method::MessageWithdraw(MessageTarget {
            message_id: withdrawn,
        }));
        assert_eq!(taken["result"]["message"]["summary"], "withdrawn");
        f.change_state(f.lead, AgentState::Idle);

        let block = typed_line(&mut typed).await;
        assert!(block.starts_with("[herdr] 2 messages"), "{block}");
        assert!(block.contains("kept") && block.contains("late"), "{block}");
        assert!(!block.contains("withdrawn"), "{block}");
        let refused = f.call(Method::MessageWithdraw(MessageTarget { message_id: late }));
        assert_eq!(refused["error"]["code"], "message_delivered");
        assert_eq!(delivered_ids(&f.events), vec![kept, late]);
    }

    #[tokio::test]
    async fn past_the_limit_the_backlog_is_one_pointer_line_and_all_of_it_is_delivered() {
        let mut f = fixture();
        f.agent(f.worker, Agent::Claude, AgentState::Working, "worker");
        f.change_state(f.lead, AgentState::Working);
        let mut typed = f.runtime(f.lead, EMPTY);
        let ids: Vec<u64> = (0..=crate::app::messages::DELIVERY_BLOCK_LIMIT)
            .map(|index| id_of(&f.send(f.lead, &format!("m{index}"))))
            .collect();
        f.send_from(f.lead, "from the user", None);

        f.change_state(f.lead, AgentState::Idle);

        let to = f.call(Method::MessageGet(MessageTarget { message_id: ids[0] }))["result"]
            ["message"]["to_agent_id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            typed_line(&mut typed).await,
            format!(
                "[herdr] 7 messages from worker, user, the oldest sent just now: \
                 `loom inbox` lists them, or `herdr message list --to {to}`"
            )
        );
        let pending = f.call(Method::MessageList(MessageListParams {
            pending: true,
            ..Default::default()
        }));
        assert_eq!(pending["result"]["messages"], serde_json::json!([]));
        assert_eq!(delivered_ids(&f.events).len(), 7);
    }

    #[tokio::test]
    async fn an_agent_awaiting_background_work_takes_messages_like_an_idle_one() {
        let mut f = fixture();
        f.agent(f.worker, Agent::Claude, AgentState::Working, "worker");
        f.change_state(f.lead, AgentState::Awaiting);
        let mut typed = f.runtime(f.lead, EMPTY);

        let first = f.send(f.lead, "first")["result"]["message"]["message_id"]
            .as_u64()
            .unwrap();
        assert!(typed_line(&mut typed).await.contains("first"));
        assert_eq!(f.state_of(first), "delivered");

        f.change_state(f.lead, AgentState::Working);
        let second = f.send(f.lead, "second")["result"]["message"]["message_id"]
            .as_u64()
            .unwrap();
        assert_eq!(f.state_of(second), "pending", "held while it works");
        f.change_state(f.lead, AgentState::Awaiting);
        assert!(typed_line(&mut typed).await.contains("second"));
        assert_eq!(f.state_of(second), "delivered");
    }

    #[tokio::test]
    async fn a_draft_in_the_composer_holds_the_message_until_the_next_state_change() {
        let mut f = fixture();
        let mut typed = f.runtime(f.lead, "\u{276f} half typed");

        let id = f.send(f.lead, "wait for me")["result"]["message"]["message_id"]
            .as_u64()
            .unwrap();
        assert_eq!(f.state_of(id), "pending");
        assert!(typed.try_recv().is_err());

        let mut typed = f.runtime(f.lead, EMPTY);
        f.app.deliver_pending_messages();
        assert!(typed.try_recv().is_err(), "no retry without a state change");

        f.change_state(f.lead, AgentState::Working);
        assert!(typed.try_recv().is_err(), "a working agent waits");
        f.change_state(f.lead, AgentState::Idle);
        assert!(typed_line(&mut typed).await.contains("wait for me"));
    }

    #[tokio::test]
    async fn a_blocked_agent_takes_a_message_only_on_a_question_with_an_empty_composer() {
        for (reason, composer, delivered) in [
            (BlockedReason::Question, EMPTY, true),
            (BlockedReason::Question, "\u{276f} my answer", false),
            (BlockedReason::Permission, EMPTY, false),
            (BlockedReason::Form, EMPTY, false),
        ] {
            let mut f = fixture();
            f.agent(f.lead, Agent::Claude, AgentState::Idle, "lead");
            f.terminal(f.lead).set_detailed_hook_authority_at(
                "herdr:claude".into(),
                "claude".into(),
                AgentState::Blocked,
                crate::terminal::HookReportDetail {
                    blocked_reason: Some(reason),
                    background: false,
                },
                None,
                crate::agent_resume::AgentSessionRef::id("claude-session"),
                Some(1),
                std::time::Instant::now(),
            );
            let mut typed = f.runtime(f.lead, composer);

            let sent = f.send(f.lead, "an answer");

            let state = &sent["result"]["message"]["state"];
            if delivered {
                assert_eq!(state, "delivered", "{reason:?} {composer:?}");
                assert!(typed_line(&mut typed).await.contains("an answer"));
            } else {
                assert_eq!(state, "pending", "{reason:?} {composer:?}");
                assert!(typed.try_recv().is_err());
            }
        }
    }

    #[tokio::test]
    async fn a_shell_pane_gets_an_id_and_keeps_its_messages_queued() {
        let mut f = fixture();
        let _typed = f.runtime(f.worker, EMPTY);
        let from = Some(f.public(f.lead));

        let sent = f.send_from(f.worker, "hello shell", from);

        let message = &sent["result"]["message"];
        assert_eq!(message["state"], "pending");
        assert_eq!(message["from_name"], "lead");
        let shell_id = message["to_agent_id"].as_str().unwrap().to_owned();
        let pane = f.call(Method::PaneGet(PaneTarget {
            pane_id: f.public(f.worker),
        }));
        assert_eq!(pane["result"]["pane"]["agent_id"], shell_id.as_str());
    }

    #[tokio::test]
    async fn list_filters_and_withdraw_take_back_only_pending_messages() {
        let mut f = fixture();
        f.agent(f.worker, Agent::Claude, AgentState::Working, "worker");
        let _lead_typed = f.runtime(f.lead, EMPTY);
        let delivered = f.send(f.lead, "to lead")["result"]["message"]["message_id"]
            .as_u64()
            .unwrap();
        let pending = f.send_from(f.worker, "to worker", None)["result"]["message"]["message_id"]
            .as_u64()
            .unwrap();

        let all = f.call(Method::MessageList(MessageListParams::default()));
        assert_eq!(all["result"]["messages"].as_array().unwrap().len(), 2);
        let to_worker = f.call(Method::MessageList(MessageListParams {
            to: Some("worker".into()),
            ..Default::default()
        }));
        let to_worker = to_worker["result"]["messages"].as_array().unwrap();
        assert_eq!(to_worker.len(), 1);
        assert_eq!(to_worker[0]["from_name"], "user");
        assert!(to_worker[0].get("from_agent_id").is_none());
        let from_worker = f.call(Method::MessageList(MessageListParams {
            from: Some("worker".into()),
            ..Default::default()
        }));
        assert_eq!(
            from_worker["result"]["messages"][0]["message_id"],
            delivered
        );
        let only_pending = f.call(Method::MessageList(MessageListParams {
            pending: true,
            ..Default::default()
        }));
        assert_eq!(
            only_pending["result"]["messages"].as_array().unwrap().len(),
            1
        );

        let refused = f.call(Method::MessageWithdraw(MessageTarget {
            message_id: delivered,
        }));
        assert_eq!(refused["error"]["code"], "message_delivered");
        let withdrawn = f.call(Method::MessageWithdraw(MessageTarget {
            message_id: pending,
        }));
        assert_eq!(withdrawn["result"]["message"]["summary"], "to worker");
        let gone = f.call(Method::MessageGet(MessageTarget {
            message_id: pending,
        }));
        assert_eq!(gone["error"]["code"], "message_not_found");
    }

    #[test]
    fn send_refuses_an_empty_summary_an_unknown_agent_and_an_unknown_reply() {
        let mut f = fixture();
        let empty = f.send(f.lead, "   ");
        assert_eq!(empty["error"]["code"], "empty_message");

        let unknown = f.call(Method::MessageSend(MessageSendParams {
            to: "99999999".into(),
            summary: "hi".into(),
            kind: MessageKind::Tell,
            body: None,
            from: None,
            reply_to: None,
        }));
        assert_eq!(unknown["error"]["code"], "agent_not_found");

        let lead = f.public(f.lead);
        let reply = f.call(Method::MessageSend(MessageSendParams {
            to: lead,
            summary: "hi".into(),
            kind: MessageKind::Reply,
            body: None,
            from: None,
            reply_to: Some(42),
        }));
        assert_eq!(reply["error"]["code"], "message_not_found");
        assert!(f.app.state.messages.is_empty());
    }

    #[test]
    fn closing_the_recipient_pane_drops_its_messages() {
        let mut f = fixture();
        f.agent(f.worker, Agent::Claude, AgentState::Working, "worker");
        f.change_state(f.lead, AgentState::Working);
        f.send(f.lead, "never read");
        f.send_from(f.worker, "kept", None);

        let pane_id = f.public(f.lead);
        f.call(Method::PaneClose(PaneTarget { pane_id }));
        f.app.sync_agent_identities();

        let left: Vec<_> = f
            .app
            .state
            .messages
            .iter()
            .map(|m| m.summary.clone())
            .collect();
        assert_eq!(left, vec!["kept".to_string()]);
    }

    #[tokio::test]
    async fn a_delivered_event_carries_the_message() {
        let mut f = fixture();
        let _typed = f.runtime(f.lead, EMPTY);
        f.send(f.lead, "evented");
        let delivered = f
            .events
            .events_after(0)
            .into_iter()
            .find_map(|(_, event)| match event.data {
                EventData::MessageDelivered { message } => Some(message),
                _ => None,
            })
            .expect("a delivered event");
        assert_eq!(delivered.state, MessageState::Delivered);
        assert_eq!(delivered.summary, "evented");
        assert!(delivered.delivered_at_ms.is_some());
    }
}
