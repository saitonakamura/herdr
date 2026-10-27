use super::*;

const BACKGROUND_TAB: &str = "tab_2";
const AGENT_PANE: &str = "pane_2";

fn agent(status: AgentStatus) -> ClientShellAgent {
    ClientShellAgent {
        repo: None,
        pane_id: AGENT_PANE.into(),
        workspace_id: "ws_1".into(),
        tab_id: BACKGROUND_TAB.into(),
        name: Some("agent".into()),
        display_agent: None,
        agent: Some("agent".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: status,
        state_change_seq: 1,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused: false,
    }
}

/// `pane_1` is focused in `tab_1`; the agent in `pane_2` sits in a background tab.
fn background_agent_snapshot(status: AgentStatus) -> ClientShellSnapshot {
    let mut snapshot = snapshot();
    let mut pane = snapshot.panes[0].clone();
    pane.pane_id = AGENT_PANE.into();
    pane.tab_id = BACKGROUND_TAB.into();
    pane.focused = false;
    snapshot.panes.push(pane);
    snapshot.agents.push(agent(status));
    snapshot
}

fn system_state(status: AgentStatus) -> ClientShellState {
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.toast_delivery = crate::config::ToastDelivery::System;
    config.toast_delay_seconds = 0;
    let mut state = ClientShellState::new(config);
    state.outer_focused = Some(true);
    state.set_snapshot(Box::new(background_agent_snapshot(status)));
    state
}

fn event(kind: SemanticNotificationKind) -> SemanticNotification {
    SemanticNotification {
        kind,
        title: "agent".into(),
        body: None,
        sound: None,
        agent: Some("agent".into()),
        workspace_id: Some("ws_1".into()),
        tab_id: Some(BACKGROUND_TAB.into()),
        pane_id: Some(AGENT_PANE.into()),
        position: None,
    }
}

fn shown_target(
    state: &mut ClientShellState,
    kind: SemanticNotificationKind,
    now: std::time::Instant,
) -> ClientSystemNotificationTarget {
    let (effects, _) = state.receive_notification(&ClientEndpointId::Local, event(kind), now);
    let [ClientShellNotificationEffect::System {
        target: Some(target),
        ..
    }] = effects.as_slice()
    else {
        panic!(
            "a pane notification must carry its target, got {} effects",
            effects.len()
        );
    };
    target.clone()
}

fn update_snapshot(state: &mut ClientShellState, edit: impl FnOnce(&mut ClientShellSnapshot)) {
    let mut snapshot = state
        .endpoints
        .iter()
        .find(|endpoint| endpoint.endpoint_id == ClientEndpointId::Local)
        .and_then(|endpoint| endpoint.snapshot.as_deref())
        .cloned()
        .expect("local snapshot");
    snapshot.revision = snapshot.revision.saturating_add(1);
    edit(&mut snapshot);
    state.set_snapshot(Box::new(snapshot));
}

fn dismissed(effects: &[ClientShellNotificationEffect]) -> Vec<&ClientSystemNotificationTarget> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            ClientShellNotificationEffect::DismissSystem { target } => Some(target),
            _ => None,
        })
        .collect()
}

fn set_status(state: &mut ClientShellState, status: AgentStatus) {
    update_snapshot(state, |snapshot| {
        snapshot.agents[0].agent_status = status;
        snapshot.agents[0].state_change_seq += 1;
    });
}

#[test]
fn system_notifications_carry_the_endpoint_boot_and_pane() {
    let mut state = system_state(AgentStatus::Blocked);

    let target = shown_target(
        &mut state,
        SemanticNotificationKind::NeedsAttention,
        std::time::Instant::now(),
    );

    assert_eq!(
        target,
        ClientSystemNotificationTarget {
            endpoint_id: ClientEndpointId::Local,
            boot_id: "boot-1".into(),
            pane_id: AGENT_PANE.into(),
        }
    );
    assert!(
        !state
            .activate_system_notification(target)
            .actions
            .is_empty(),
        "a click on a current target focuses its pane"
    );
}

#[test]
fn leaving_the_notified_state_dismisses_the_notification() {
    for (kind, notified, next) in [
        (
            SemanticNotificationKind::NeedsAttention,
            AgentStatus::Blocked,
            AgentStatus::Working,
        ),
        (
            SemanticNotificationKind::Finished,
            AgentStatus::Done,
            AgentStatus::Idle,
        ),
    ] {
        let mut state = system_state(notified);
        let now = std::time::Instant::now();
        let target = shown_target(&mut state, kind, now);
        let (effects, _) = state.tick_notifications(now);
        assert!(dismissed(&effects).is_empty(), "{kind:?} still current");

        set_status(&mut state, next);
        let (effects, _) = state.tick_notifications(now);

        assert_eq!(dismissed(&effects), [&target], "{kind:?} -> {next:?}");
        let (effects, _) = state.tick_notifications(now);
        assert!(dismissed(&effects).is_empty(), "dismissed once");
    }
}

#[test]
fn seeing_the_pane_dismisses_the_notification() {
    let mut state = system_state(AgentStatus::Blocked);
    let now = std::time::Instant::now();
    let target = shown_target(&mut state, SemanticNotificationKind::NeedsAttention, now);

    update_snapshot(&mut state, |snapshot| {
        snapshot.focused_tab_id = Some(BACKGROUND_TAB.into());
        snapshot.focused_pane_id = Some(AGENT_PANE.into());
    });
    state.outer_focused = Some(false);
    let (effects, _) = state.tick_notifications(now);
    assert!(
        dismissed(&effects).is_empty(),
        "a focused tab behind another app is not seen"
    );

    state.outer_focused = Some(true);
    let (effects, _) = state.tick_notifications(now);

    assert_eq!(dismissed(&effects), [&target]);
}

#[test]
fn closing_the_pane_or_restarting_the_server_dismisses_the_notification() {
    for edit in [
        (|snapshot: &mut ClientShellSnapshot| {
            snapshot.panes.retain(|pane| pane.pane_id != AGENT_PANE);
            snapshot.agents.clear();
        }) as fn(&mut ClientShellSnapshot),
        |snapshot| snapshot.boot_id = "boot-2".into(),
    ] {
        let mut state = system_state(AgentStatus::Blocked);
        let now = std::time::Instant::now();
        let target = shown_target(&mut state, SemanticNotificationKind::Custom, now);

        update_snapshot(&mut state, edit);
        let (effects, _) = state.tick_notifications(now);

        assert_eq!(dismissed(&effects), [&target]);
    }
}

#[test]
fn a_state_the_snapshot_has_not_shown_yet_gets_a_grace_period() {
    let mut state = system_state(AgentStatus::Working);
    let now = std::time::Instant::now();
    let target = shown_target(&mut state, SemanticNotificationKind::NeedsAttention, now);

    let (effects, _) = state.tick_notifications(now);
    assert!(
        dismissed(&effects).is_empty(),
        "the blocked snapshot may still be on its way"
    );

    let (effects, _) = state.tick_notifications(now + std::time::Duration::from_secs(1));
    assert_eq!(
        dismissed(&effects),
        [&target],
        "never confirmed means it already passed"
    );
}

#[test]
fn custom_notifications_wait_for_the_pane_to_be_seen() {
    let mut state = system_state(AgentStatus::Working);
    let now = std::time::Instant::now();
    shown_target(&mut state, SemanticNotificationKind::Custom, now);

    set_status(&mut state, AgentStatus::Idle);
    let (effects, _) = state.tick_notifications(now + std::time::Duration::from_secs(5));

    assert!(dismissed(&effects).is_empty());
}

#[test]
fn a_new_notification_for_the_pane_replaces_the_tracked_one() {
    let mut state = system_state(AgentStatus::Blocked);
    let now = std::time::Instant::now();
    shown_target(&mut state, SemanticNotificationKind::NeedsAttention, now);
    let (effects, _) = state.receive_notification(
        &ClientEndpointId::Local,
        event(SemanticNotificationKind::NeedsAttention),
        now,
    );
    assert!(
        dismissed(&effects).is_empty(),
        "the same target replaces natively"
    );
    assert_eq!(state.shown_system_notifications.len(), 1);

    update_snapshot(&mut state, |snapshot| snapshot.boot_id = "boot-2".into());
    let (effects, _) = state.receive_notification(
        &ClientEndpointId::Local,
        event(SemanticNotificationKind::NeedsAttention),
        now,
    );

    let [ClientShellNotificationEffect::DismissSystem { target: old }, ClientShellNotificationEffect::System {
        target: Some(new), ..
    }] = effects.as_slice()
    else {
        panic!(
            "an older boot's notification is removed before the new one, got {} effects",
            effects.len()
        );
    };
    assert_eq!(old.boot_id, "boot-1");
    assert_eq!(new.boot_id, "boot-2");
    assert_eq!(state.shown_system_notifications.len(), 1);
}

#[test]
fn a_missing_snapshot_keeps_the_notification() {
    let mut state = system_state(AgentStatus::Blocked);
    let now = std::time::Instant::now();
    shown_target(&mut state, SemanticNotificationKind::NeedsAttention, now);

    state.endpoints[0].snapshot = None;
    let (effects, _) = state.tick_notifications(now + std::time::Duration::from_secs(5));

    assert!(dismissed(&effects).is_empty());
}
