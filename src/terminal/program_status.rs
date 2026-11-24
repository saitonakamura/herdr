//! The lifecycle a pane's program reports about itself over OSC 7501.
//!
//! It comes down the pane's own PTY, so it ranks above hook reports and screen
//! rules. Hooks stay a layer on top for what the protocol cannot say: how a
//! turn ended (a question, background work), a dialog the agent's host raised,
//! and whether a question is a form.

use std::time::Instant;

use crate::detect::{AgentState, BlockedReason};

use super::{HookAuthority, RestoredAgentStatus, TerminalState, TerminalStateMutation};

/// What the root OSC 7501 record says, in Herdr's terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramLifecycle {
    Working,
    Blocked(Option<BlockedReason>),
    Done,
    Idle,
    /// Herdr has no error state: the turn is over, and the message says why.
    Error,
}

impl ProgramLifecycle {
    fn phase(self) -> Phase {
        match self {
            Self::Working => Phase::Working,
            Self::Blocked(_) => Phase::Blocked,
            Self::Done | Self::Idle | Self::Error => Phase::AtPrompt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Working,
    Blocked,
    AtPrompt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramStatusReport {
    pub lifecycle: ProgramLifecycle,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProgramStatusAuthority {
    report: ProgramStatusReport,
    /// When the record last moved between working, blocked and at the prompt.
    changed_at: Instant,
    /// When the program last left its prompt; `None` if it has only reported from it.
    turn_started_at: Option<Instant>,
}

/// Which source decided the pane's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LifecycleSource {
    Restored,
    ProgramStatus,
    /// The program's record, refined by what a hook said on top of it.
    ProgramStatusWithHook,
    Hook,
    Screen,
}

impl LifecycleSource {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Restored => "restored",
            Self::ProgramStatus => "program_status",
            Self::ProgramStatusWithHook => "program_status+hook",
            Self::Hook => "hook",
            Self::Screen => "screen",
        }
    }
}

impl HookAuthority {
    /// A report of how a turn ended, which the program's own record cannot tell apart
    /// from work it left running: idle, a question in prose, or background work.
    fn ends_a_turn(&self) -> bool {
        match self.state {
            AgentState::Idle => true,
            AgentState::Blocked => self.detail.blocked_reason == Some(BlockedReason::Question),
            AgentState::Working => self.detail.background,
            AgentState::Unknown | AgentState::Awaiting => false,
        }
    }
}

impl TerminalState {
    /// Takes the pane's root OSC 7501 record as its program now reports it. `None` is
    /// a cleared record, or one whose process is gone: the next source decides.
    pub fn set_program_status_at(
        &mut self,
        report: Option<ProgramStatusReport>,
        now: Instant,
    ) -> Option<TerminalStateMutation> {
        let previous = self.program_status.take();
        let Some(report) = report else {
            previous.as_ref()?;
            return self.program_status_mutation(now, |_| {});
        };
        let phase = report.lifecycle.phase();
        let previous_phase = previous
            .as_ref()
            .map(|previous| previous.report.lifecycle.phase());
        let changed_at = previous
            .as_ref()
            .filter(|_| previous_phase == Some(phase))
            .map_or(now, |previous| previous.changed_at);
        let turn_started_at = match (previous_phase, phase) {
            (None | Some(Phase::AtPrompt), Phase::Working | Phase::Blocked) => Some(now),
            (None, Phase::AtPrompt) => None,
            _ => previous
                .as_ref()
                .and_then(|previous| previous.turn_started_at),
        };
        let authority = ProgramStatusAuthority {
            report,
            changed_at,
            turn_started_at,
        };
        if previous.as_ref() == Some(&authority) {
            self.program_status = previous;
            return None;
        }
        self.program_status_mutation(now, |terminal| {
            // The agent left its prompt, so what a restart saved no longer stands,
            // unless the record names the very block that was saved.
            let leaves_prompt = match authority.report.lifecycle {
                ProgramLifecycle::Working => true,
                ProgramLifecycle::Blocked(reason) => {
                    !terminal.block_confirms_restored_status(reason)
                }
                ProgramLifecycle::Done | ProgramLifecycle::Idle | ProgramLifecycle::Error => false,
            };
            if leaves_prompt {
                terminal.restored_status = None;
            }
            terminal.program_status = Some(authority);
        })
    }

    fn program_status_mutation(
        &mut self,
        now: Instant,
        apply: impl FnOnce(&mut Self),
    ) -> Option<TerminalStateMutation> {
        let previous_agent_label = self.effective_agent_label().map(str::to_string);
        let previous_known_agent = self.effective_known_agent();
        let previous_state = self.state;
        let previous_presentation = self.effective_presentation_for_state_at(previous_state, now);
        apply(self);
        Some(TerminalStateMutation {
            effective_state_change: self.recompute_effective_state(
                previous_agent_label,
                previous_known_agent,
                previous_state,
                previous_presentation,
                now,
            ),
            ..TerminalStateMutation::default()
        })
    }

    /// A blocked report naming the block a restart saved confirms it instead of replacing it.
    pub(super) fn block_confirms_restored_status(&self, reason: Option<BlockedReason>) -> bool {
        reason.is_some()
            && match self.restored_status {
                // Standing it again stays the session report's, since that also wakes delivery.
                Some(RestoredAgentStatus::UnconfirmedQuestion) => {
                    reason == Some(BlockedReason::Question)
                }
                restored => restored == Some(RestoredAgentStatus::Blocked(reason)),
            }
    }

    /// The program's record while an agent holds the pane; a record nobody owns decides nothing.
    fn effective_program_status(&self) -> Option<&ProgramStatusAuthority> {
        self.program_status
            .as_ref()
            .filter(|_| self.effective_agent_label().is_some())
    }

    pub(crate) fn program_status_message(&self) -> Option<&str> {
        self.effective_program_status()
            .and_then(|status| status.report.message.as_deref())
    }

    /// The state the program's record stands for, with the hook layer applied.
    pub(super) fn program_status_state_and_blocked_reason(
        &self,
    ) -> Option<(AgentState, Option<BlockedReason>, LifecycleSource)> {
        let status = self.effective_program_status()?;
        let hook = self
            .hook_authority
            .as_ref()
            .filter(|authority| self.hook_authority_is_effective(authority));
        let since_turn_start = |hook: &&HookAuthority| {
            status
                .turn_started_at
                .is_none_or(|started| hook.reported_at >= started)
        };
        let since_change = |hook: &&HookAuthority| hook.reported_at >= status.changed_at;
        let hook_blocked = |hook: &&HookAuthority| hook.state == AgentState::Blocked;
        let from_hook = |hook: &HookAuthority| {
            let (state, reason) = hook.reported_state_and_blocked_reason();
            (state, reason, LifecycleSource::ProgramStatusWithHook)
        };
        let own = |state, reason| (state, reason, LifecycleSource::ProgramStatus);

        Some(match status.report.lifecycle {
            ProgramLifecycle::Blocked(reason) => {
                let hook = hook.filter(hook_blocked).filter(since_turn_start);
                let hook_reason = hook.and_then(|hook| hook.detail.blocked_reason);
                let screen_reason = self
                    .fallback_blocked_reason
                    .filter(|_| self.fallback_state == AgentState::Blocked);
                match reason {
                    // The protocol has one kind for a question in prose and for a dialog
                    // that asks; a hook or the screen naming a dialog makes it a form.
                    Some(BlockedReason::Question)
                        if matches!(
                            hook_reason,
                            Some(BlockedReason::Form | BlockedReason::Permission)
                        ) || screen_reason == Some(BlockedReason::Form) =>
                    {
                        (
                            AgentState::Blocked,
                            Some(BlockedReason::Form),
                            LifecycleSource::ProgramStatusWithHook,
                        )
                    }
                    Some(_) => own(AgentState::Blocked, reason),
                    None if hook_reason.is_some() => (
                        AgentState::Blocked,
                        hook_reason,
                        LifecycleSource::ProgramStatusWithHook,
                    ),
                    None => own(AgentState::Blocked, screen_reason),
                }
            }
            ProgramLifecycle::Working => {
                // A dialog the agent's host raised mid-turn, which the agent does not report.
                if let Some(hook) = hook.filter(hook_blocked).filter(since_change) {
                    from_hook(hook)
                // A record that stays working past the end of a turn is work left running.
                } else if let Some(hook) = hook
                    .filter(|hook| hook.ends_a_turn())
                    .filter(since_turn_start)
                {
                    from_hook(hook)
                } else {
                    own(AgentState::Working, None)
                }
            }
            ProgramLifecycle::Done | ProgramLifecycle::Idle | ProgramLifecycle::Error => {
                let refines = |hook: &&HookAuthority| {
                    if hook.ends_a_turn() {
                        hook.state != AgentState::Idle && since_turn_start(hook)
                    } else {
                        hook_blocked(hook) && since_change(hook)
                    }
                };
                match hook.filter(refines) {
                    Some(hook) => from_hook(hook),
                    None => own(AgentState::Idle, None),
                }
            }
        })
    }

    /// Which source decides the pane's state right now.
    pub(crate) fn lifecycle_source(&self) -> LifecycleSource {
        if self.restored_status.is_some() {
            return LifecycleSource::Restored;
        }
        if let Some((_, _, source)) = self.program_status_state_and_blocked_reason() {
            return source;
        }
        let hook_decides = self
            .hook_authority
            .as_ref()
            .is_some_and(|authority| self.hook_authority_is_effective(authority));
        if hook_decides {
            LifecycleSource::Hook
        } else {
            LifecycleSource::Screen
        }
    }

    #[cfg(test)]
    pub(crate) fn program_status_lifecycle(&self) -> Option<ProgramLifecycle> {
        self.effective_program_status()
            .map(|status| status.report.lifecycle)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::detect::Agent;
    use crate::terminal::{HookReportDetail, TerminalId};

    fn agent_terminal(agent: Agent, now: Instant) -> TerminalState {
        let mut terminal = TerminalState::new(TerminalId::alloc(), "/tmp".into());
        screen(&mut terminal, agent, AgentState::Idle, None, now);
        terminal
    }

    /// A pi pane whose full-lifecycle hook reports are accepted.
    fn pi_terminal(now: Instant) -> TerminalState {
        let mut terminal = agent_terminal(Agent::Pi, now);
        terminal.set_persisted_agent_session(crate::agent_resume::PersistedAgentSession {
            source: "herdr:pi".into(),
            agent: "pi".into(),
            session_ref: crate::agent_resume::AgentSessionRef::id("pi-root").unwrap(),
        });
        terminal
    }

    fn after(now: Instant, millis: u64) -> Instant {
        now + Duration::from_millis(millis)
    }

    fn screen(
        terminal: &mut TerminalState,
        agent: Agent,
        state: AgentState,
        reason: Option<BlockedReason>,
        at: Instant,
    ) {
        terminal.set_detected_state_with_blocked_reason_at(
            Some(agent),
            state,
            state == AgentState::Blocked,
            reason,
            state == AgentState::Working,
            false,
            at,
        );
    }

    fn program(terminal: &mut TerminalState, lifecycle: ProgramLifecycle, at: Instant) {
        terminal.set_program_status_at(
            Some(ProgramStatusReport {
                lifecycle,
                message: None,
            }),
            at,
        );
    }

    fn hook(
        terminal: &mut TerminalState,
        source: &str,
        agent: &str,
        state: AgentState,
        detail: HookReportDetail,
        at: Instant,
    ) {
        // A full-lifecycle report is only accepted for the session it is anchored to.
        let session_ref = (source == "herdr:pi")
            .then(|| crate::agent_resume::AgentSessionRef::id("pi-root").unwrap());
        terminal.set_detailed_hook_authority_at(
            source.into(),
            agent.into(),
            state,
            detail,
            None,
            session_ref,
            None,
            at,
        );
    }

    fn claude_hook(
        terminal: &mut TerminalState,
        state: AgentState,
        detail: HookReportDetail,
        at: Instant,
    ) {
        hook(terminal, "herdr:claude", "claude", state, detail, at);
    }

    fn blocked_on(reason: BlockedReason) -> HookReportDetail {
        HookReportDetail {
            blocked_reason: Some(reason),
            ..HookReportDetail::default()
        }
    }

    fn background() -> HookReportDetail {
        HookReportDetail {
            background: true,
            ..HookReportDetail::default()
        }
    }

    fn state(terminal: &TerminalState) -> (AgentState, Option<BlockedReason>) {
        (terminal.state, terminal.blocked_reason())
    }

    #[test]
    fn every_record_state_maps_to_a_pane_state() {
        let now = Instant::now();
        for (lifecycle, expected) in [
            (ProgramLifecycle::Working, (AgentState::Working, None)),
            (
                ProgramLifecycle::Blocked(Some(BlockedReason::Permission)),
                (AgentState::Blocked, Some(BlockedReason::Permission)),
            ),
            (
                ProgramLifecycle::Blocked(Some(BlockedReason::Question)),
                (AgentState::Blocked, Some(BlockedReason::Question)),
            ),
            (
                ProgramLifecycle::Blocked(Some(BlockedReason::Auth)),
                (AgentState::Blocked, Some(BlockedReason::Auth)),
            ),
            (ProgramLifecycle::Blocked(None), (AgentState::Blocked, None)),
            (ProgramLifecycle::Done, (AgentState::Idle, None)),
            (ProgramLifecycle::Idle, (AgentState::Idle, None)),
            (ProgramLifecycle::Error, (AgentState::Idle, None)),
        ] {
            let mut terminal = agent_terminal(Agent::Codex, now);
            // The screen says the opposite, so the record is what decides.
            let opposite = if expected.0 == AgentState::Idle {
                AgentState::Working
            } else {
                AgentState::Idle
            };
            screen(&mut terminal, Agent::Codex, opposite, None, after(now, 10));
            program(&mut terminal, lifecycle, after(now, 20));
            assert_eq!(state(&terminal), expected, "{lifecycle:?}");
            assert_eq!(
                terminal.lifecycle_source(),
                LifecycleSource::ProgramStatus,
                "{lifecycle:?}"
            );
        }
    }

    #[test]
    fn an_error_keeps_its_message() {
        let now = Instant::now();
        let mut terminal = agent_terminal(Agent::Codex, now);
        terminal.set_program_status_at(
            Some(ProgramStatusReport {
                lifecycle: ProgramLifecycle::Error,
                message: Some("rate limited".into()),
            }),
            after(now, 10),
        );
        assert_eq!(terminal.state, AgentState::Idle);
        assert_eq!(terminal.program_status_message(), Some("rate limited"));
    }

    #[test]
    fn the_record_outranks_later_screen_matches() {
        let now = Instant::now();
        let mut terminal = agent_terminal(Agent::Codex, now);
        program(&mut terminal, ProgramLifecycle::Working, after(now, 10));
        screen(
            &mut terminal,
            Agent::Codex,
            AgentState::Idle,
            None,
            after(now, 20),
        );
        assert_eq!(terminal.state, AgentState::Working);
        screen(
            &mut terminal,
            Agent::Codex,
            AgentState::Blocked,
            Some(BlockedReason::Permission),
            after(now, 30),
        );
        assert_eq!(terminal.state, AgentState::Working);

        program(&mut terminal, ProgramLifecycle::Idle, after(now, 40));
        screen(
            &mut terminal,
            Agent::Codex,
            AgentState::Working,
            None,
            after(now, 50),
        );
        assert_eq!(terminal.state, AgentState::Idle);
    }

    #[test]
    fn the_record_outranks_hook_working_and_idle() {
        let now = Instant::now();
        let mut terminal = agent_terminal(Agent::Claude, now);
        claude_hook(
            &mut terminal,
            AgentState::Working,
            HookReportDetail::default(),
            after(now, 10),
        );
        program(&mut terminal, ProgramLifecycle::Working, after(now, 20));
        // An interrupt: no stop hook, the record goes idle at once.
        program(&mut terminal, ProgramLifecycle::Idle, after(now, 30));
        assert_eq!(terminal.state, AgentState::Idle);
        assert_eq!(terminal.lifecycle_source(), LifecycleSource::ProgramStatus);

        // A stale idle hook from the last turn does not hold a new turn at idle.
        claude_hook(
            &mut terminal,
            AgentState::Idle,
            HookReportDetail::default(),
            after(now, 40),
        );
        program(&mut terminal, ProgramLifecycle::Working, after(now, 50));
        assert_eq!(terminal.state, AgentState::Working);
        // Nor does a full-lifecycle integration's.
        let mut terminal = pi_terminal(now);
        hook(
            &mut terminal,
            "herdr:pi",
            "pi",
            AgentState::Idle,
            HookReportDetail::default(),
            after(now, 10),
        );
        assert_eq!(terminal.lifecycle_source(), LifecycleSource::Hook);
        program(&mut terminal, ProgramLifecycle::Working, after(now, 20));
        assert_eq!(terminal.state, AgentState::Working);
        hook(
            &mut terminal,
            "herdr:pi",
            "pi",
            AgentState::Working,
            HookReportDetail::default(),
            after(now, 30),
        );
        program(&mut terminal, ProgramLifecycle::Done, after(now, 40));
        assert_eq!(terminal.state, AgentState::Idle);
    }

    #[test]
    fn a_turn_ending_sentinel_refines_done_and_idle() {
        let now = Instant::now();
        for at_prompt in [ProgramLifecycle::Done, ProgramLifecycle::Idle] {
            // The stop hook runs before the record goes done.
            let mut terminal = agent_terminal(Agent::Claude, now);
            program(&mut terminal, ProgramLifecycle::Working, after(now, 10));
            claude_hook(
                &mut terminal,
                AgentState::Blocked,
                blocked_on(BlockedReason::Question),
                after(now, 20),
            );
            program(&mut terminal, at_prompt, after(now, 30));
            assert_eq!(
                state(&terminal),
                (AgentState::Blocked, Some(BlockedReason::Question))
            );
            assert_eq!(
                terminal.lifecycle_source(),
                LifecycleSource::ProgramStatusWithHook
            );

            // Or after it.
            let mut terminal = terminal_with_turn(now);
            program(&mut terminal, at_prompt, after(now, 20));
            claude_hook(
                &mut terminal,
                AgentState::Working,
                background(),
                after(now, 30),
            );
            assert_eq!(state(&terminal), (AgentState::Awaiting, None));
        }
    }

    fn terminal_with_turn(now: Instant) -> TerminalState {
        let mut terminal = agent_terminal(Agent::Claude, now);
        program(&mut terminal, ProgramLifecycle::Working, after(now, 10));
        terminal
    }

    #[test]
    fn a_sentinel_from_an_earlier_turn_does_not_refine_this_one() {
        let now = Instant::now();
        let mut terminal = terminal_with_turn(now);
        claude_hook(
            &mut terminal,
            AgentState::Blocked,
            blocked_on(BlockedReason::Question),
            after(now, 20),
        );
        program(&mut terminal, ProgramLifecycle::Done, after(now, 30));
        // The answer starts a turn whose prompt hook was lost, and it is interrupted.
        program(&mut terminal, ProgramLifecycle::Working, after(now, 40));
        assert_eq!(terminal.state, AgentState::Working);
        program(&mut terminal, ProgramLifecycle::Idle, after(now, 50));
        assert_eq!(state(&terminal), (AgentState::Idle, None));
    }

    #[test]
    fn a_stop_hook_stands_over_a_record_left_working() {
        let now = Instant::now();
        // The turn ends with a shell still running: the record stays working.
        let mut terminal = terminal_with_turn(now);
        claude_hook(
            &mut terminal,
            AgentState::Idle,
            HookReportDetail::default(),
            after(now, 20),
        );
        program(&mut terminal, ProgramLifecycle::Working, after(now, 30));
        assert_eq!(state(&terminal), (AgentState::Idle, None));
        assert_eq!(
            terminal.lifecycle_source(),
            LifecycleSource::ProgramStatusWithHook
        );

        claude_hook(
            &mut terminal,
            AgentState::Working,
            background(),
            after(now, 40),
        );
        assert_eq!(state(&terminal), (AgentState::Awaiting, None));

        // The next prompt's hook says working again.
        claude_hook(
            &mut terminal,
            AgentState::Working,
            HookReportDetail::default(),
            after(now, 50),
        );
        assert_eq!(state(&terminal), (AgentState::Working, None));

        // A dialog raised for the work left running still shows.
        claude_hook(
            &mut terminal,
            AgentState::Idle,
            HookReportDetail::default(),
            after(now, 60),
        );
        program(
            &mut terminal,
            ProgramLifecycle::Blocked(Some(BlockedReason::Permission)),
            after(now, 70),
        );
        assert_eq!(
            state(&terminal),
            (AgentState::Blocked, Some(BlockedReason::Permission))
        );
    }

    #[test]
    fn a_hook_block_refines_a_working_record_until_the_record_moves() {
        let now = Instant::now();
        let mut terminal = pi_terminal(now);
        program(&mut terminal, ProgramLifecycle::Working, after(now, 10));
        // A gate approval: the agent's host asks, the agent itself keeps saying working.
        hook(
            &mut terminal,
            "herdr:pi",
            "pi",
            AgentState::Blocked,
            blocked_on(BlockedReason::Permission),
            after(now, 20),
        );
        assert_eq!(
            state(&terminal),
            (AgentState::Blocked, Some(BlockedReason::Permission))
        );
        hook(
            &mut terminal,
            "herdr:pi",
            "pi",
            AgentState::Working,
            HookReportDetail::default(),
            after(now, 30),
        );
        assert_eq!(state(&terminal), (AgentState::Working, None));

        // A dialog the record itself reported and saw answered is over, whatever the hook last said.
        let mut terminal = terminal_with_turn(now);
        claude_hook(
            &mut terminal,
            AgentState::Blocked,
            blocked_on(BlockedReason::Permission),
            after(now, 20),
        );
        program(
            &mut terminal,
            ProgramLifecycle::Blocked(None),
            after(now, 30),
        );
        assert_eq!(
            state(&terminal),
            (AgentState::Blocked, Some(BlockedReason::Permission))
        );
        program(&mut terminal, ProgramLifecycle::Working, after(now, 40));
        assert_eq!(state(&terminal), (AgentState::Working, None));
        // Escaping out of a dialog ends the turn with no stop hook.
        program(
            &mut terminal,
            ProgramLifecycle::Blocked(None),
            after(now, 50),
        );
        program(&mut terminal, ProgramLifecycle::Idle, after(now, 60));
        assert_eq!(state(&terminal), (AgentState::Idle, None));
    }

    #[test]
    fn a_question_is_a_form_when_a_hook_or_the_screen_names_a_dialog() {
        let now = Instant::now();
        let question = ProgramLifecycle::Blocked(Some(BlockedReason::Question));

        let mut terminal = terminal_with_turn(now);
        claude_hook(
            &mut terminal,
            AgentState::Blocked,
            blocked_on(BlockedReason::Form),
            after(now, 20),
        );
        program(&mut terminal, question, after(now, 30));
        assert_eq!(
            state(&terminal),
            (AgentState::Blocked, Some(BlockedReason::Form))
        );
        // The late permission notification for the same dialog does not make it a prose question.
        claude_hook(
            &mut terminal,
            AgentState::Blocked,
            blocked_on(BlockedReason::Permission),
            after(now, 40),
        );
        assert_eq!(
            state(&terminal),
            (AgentState::Blocked, Some(BlockedReason::Form))
        );

        let mut terminal = agent_terminal(Agent::Codex, now);
        program(&mut terminal, question, after(now, 10));
        assert_eq!(
            state(&terminal),
            (AgentState::Blocked, Some(BlockedReason::Question))
        );
        screen(
            &mut terminal,
            Agent::Codex,
            AgentState::Blocked,
            Some(BlockedReason::Form),
            after(now, 20),
        );
        assert_eq!(
            state(&terminal),
            (AgentState::Blocked, Some(BlockedReason::Form))
        );
    }

    #[test]
    fn a_cleared_record_falls_back_to_the_next_source() {
        let now = Instant::now();
        let mut terminal = agent_terminal(Agent::Claude, now);
        claude_hook(
            &mut terminal,
            AgentState::Working,
            HookReportDetail::default(),
            after(now, 10),
        );
        program(&mut terminal, ProgramLifecycle::Idle, after(now, 20));
        assert_eq!(terminal.state, AgentState::Idle);
        terminal.set_program_status_at(None, after(now, 30));
        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(terminal.lifecycle_source(), LifecycleSource::Hook);

        let mut terminal = agent_terminal(Agent::Codex, now);
        program(&mut terminal, ProgramLifecycle::Working, after(now, 10));
        terminal.set_program_status_at(None, after(now, 20));
        assert_eq!(terminal.state, AgentState::Idle);
        assert_eq!(terminal.lifecycle_source(), LifecycleSource::Screen);
        assert!(terminal
            .set_program_status_at(None, after(now, 30))
            .is_none());
    }

    #[test]
    fn a_record_does_not_outlive_its_agent() {
        let now = Instant::now();
        let mut terminal = agent_terminal(Agent::Codex, now);
        program(&mut terminal, ProgramLifecycle::Working, after(now, 10));
        terminal.set_detected_state_with_blocked_reason_at(
            Some(Agent::Codex),
            AgentState::Idle,
            false,
            None,
            false,
            true,
            after(now, 20),
        );
        assert_eq!(terminal.program_status_lifecycle(), None);

        // The next agent in the pane starts from its own sources.
        screen(
            &mut terminal,
            Agent::Claude,
            AgentState::Working,
            None,
            after(now, 30),
        );
        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(terminal.lifecycle_source(), LifecycleSource::Screen);
    }

    #[test]
    fn a_record_without_an_agent_decides_nothing() {
        let now = Instant::now();
        let mut terminal = TerminalState::new(TerminalId::alloc(), "/tmp".into());
        program(&mut terminal, ProgramLifecycle::Working, now);
        assert_eq!(terminal.state, AgentState::Unknown);
        // The agent is identified after its first report, which then counts.
        screen(
            &mut terminal,
            Agent::Codex,
            AgentState::Idle,
            None,
            after(now, 10),
        );
        assert_eq!(terminal.state, AgentState::Working);
    }

    #[test]
    fn a_declared_wait_still_reads_an_idle_record_as_awaiting() {
        let now = Instant::now();
        let mut terminal = agent_terminal(Agent::Codex, now);
        program(&mut terminal, ProgramLifecycle::Done, after(now, 10));
        terminal.set_declared_wait(true, after(now, 20));
        assert_eq!(terminal.state, AgentState::Awaiting);
    }

    #[test]
    fn a_restored_status_stands_until_the_record_leaves_the_prompt() {
        let now = Instant::now();
        let mut terminal = agent_terminal(Agent::Codex, now);
        terminal.restore_agent_status(
            RestoredAgentStatus::Blocked(Some(BlockedReason::Permission)),
            now,
        );
        program(&mut terminal, ProgramLifecycle::Idle, after(now, 10));
        assert_eq!(terminal.lifecycle_source(), LifecycleSource::Restored);
        assert_eq!(terminal.state, AgentState::Blocked);
        // Naming the saved block confirms it.
        program(
            &mut terminal,
            ProgramLifecycle::Blocked(Some(BlockedReason::Permission)),
            after(now, 20),
        );
        assert_eq!(terminal.lifecycle_source(), LifecycleSource::Restored);
        program(&mut terminal, ProgramLifecycle::Working, after(now, 30));
        assert_eq!(state(&terminal), (AgentState::Working, None));
        assert_eq!(terminal.lifecycle_source(), LifecycleSource::ProgramStatus);
    }

    #[test]
    fn an_unchanged_report_is_not_a_mutation() {
        let now = Instant::now();
        let mut terminal = agent_terminal(Agent::Codex, now);
        program(&mut terminal, ProgramLifecycle::Working, after(now, 10));
        let repeated = terminal.set_program_status_at(
            Some(ProgramStatusReport {
                lifecycle: ProgramLifecycle::Working,
                message: None,
            }),
            after(now, 20),
        );
        assert!(repeated.is_none());
    }
}
