use super::*;
use crate::protocol::session_pane::{SessionPaneUpdate, SessionPaneView};

fn view(focused: bool) -> SessionPaneView {
    SessionPaneView {
        terminal_id: "terminal-side".into(),
        focused,
        width: 20,
        collapsed_width: 6,
        mouse_reporting: true,
        sgr_pixel_mouse: false,
        pixel_width: 0,
        pixel_height: 0,
        frame: FrameData::from_ratatui_buffer_with_hyperlinks(
            &Buffer::with_lines(["SIDE-LIVE", "row two"]),
            Some(crate::protocol::CursorState {
                x: 4,
                y: 1,
                visible: true,
                shape: 1,
            }),
            &[],
        ),
    }
}

fn shell_with_session_pane(focused: bool) -> ClientShellState {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let (changed, _) = state.set_endpoint_session_pane(
        &ClientEndpointId::Local,
        None,
        SessionPaneUpdate {
            boot_id: "boot-1".into(),
            revision: 1,
            pane: Some(view(focused)),
        },
        (106, 20),
    );
    assert!(changed);
    state.set_pane_surface(surface());
    state
}

fn frame_text(frame: &FrameData) -> String {
    frame
        .cells
        .chunks(frame.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn session_pane_events(requests: &[ClientMessage]) -> Vec<ClientPaneInputEvent> {
    requests
        .iter()
        .filter_map(|request| match request {
            ClientMessage::EndpointControl { kind, data }
                if kind == crate::protocol::session_pane::INPUT_KIND =>
            {
                let input = crate::protocol::session_pane::decode_input(data).unwrap();
                assert_eq!(input.terminal_id, "terminal-side");
                Some(input.events)
            }
            _ => None,
        })
        .flatten()
        .collect()
}

#[test]
fn layout_gives_the_slot_to_the_session_pane() {
    let mut config = ClientShellConfig::from_config(&Config::default());
    let width = |config: &ClientShellConfig, collapsed, pane| {
        config.layout(120, 40, collapsed, 1, 30, pane).sidebar.width
    };
    // The server's width wins over the client's own dragged width.
    assert_eq!(width(&config, false, Some((22, 6))), 22);
    assert_eq!(width(&config, true, Some((22, 6))), 6);
    assert_eq!(width(&config, false, None), 30);
    assert_eq!(width(&config, true, None), 4);

    config.sidebar_collapsed_mode = SidebarCollapsedModeConfig::Hidden;
    assert_eq!(width(&config, true, Some((22, 6))), 0);
    assert_eq!(width(&config, false, Some((22, 6))), 22);

    // Mobile stays native, so it has no slot at all.
    let mobile = config.layout(60, 40, false, 1, 30, Some((22, 6)));
    assert_eq!(mobile.sidebar.width, 0);
    assert!(!mobile.mobile_header.is_empty());
}

#[test]
fn session_pane_replaces_the_native_sidebar_and_disables_its_divider() {
    let mut state = shell_with_session_pane(false);
    let frame = state.compose(106, 20).expect("frame");
    let text = frame_text(&frame);
    assert!(text.contains("SIDE-LIVE"));
    assert!(
        !text.contains("client-shell"),
        "native sidebar drawn:\n{text}"
    );
    let hit = state.hits.session_pane.as_ref().expect("session pane hit");
    assert_eq!(hit.rect, Rect::new(0, 0, 20, 20));
    assert_eq!(state.hits.sidebar_divider, Rect::default());
    assert!(state.hits.workspaces.is_empty());
    // Unfocused, the cursor stays the focused pane's.
    assert_ne!(
        frame.cursor.as_ref().map(|cursor| (cursor.x, cursor.y)),
        Some((4, 1))
    );

    // Without a pane the native sidebar comes back.
    let (changed, _) = state.set_endpoint_session_pane(
        &ClientEndpointId::Local,
        None,
        SessionPaneUpdate {
            boot_id: "boot-1".into(),
            revision: 2,
            pane: None,
        },
        (106, 20),
    );
    assert!(changed);
    state.set_pane_surface(surface());
    let text = frame_text(&state.compose(106, 20).expect("frame"));
    assert!(!text.contains("SIDE-LIVE"));
    assert!(text.contains("client-shell"));
    assert!(state.hits.session_pane.is_none());
}

#[test]
fn mouse_in_the_slot_goes_to_the_session_pane_and_a_pane_click_takes_focus_back() {
    let mut state = shell_with_session_pane(true);
    state.compose(106, 20).expect("frame");
    let mouse = |state: &mut ClientShellState, column, row| {
        state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::empty(),
        })])
    };

    let input = mouse(&mut state, 3, 1);
    assert!(matches!(
        &session_pane_events(&input.requests)[..],
        [ClientPaneInputEvent::Mouse {
            position: ClientMousePosition::Cell { column: 3, row: 1 },
            ..
        }]
    ));
    assert!(state.pane_mouse_gesture.is_some());
    state.pane_mouse_gesture = None;

    let pane = state.hits.panes[0].inner_rect;
    let input = mouse(&mut state, pane.x + 1, pane.y + 1);
    assert!(session_pane_events(&input.requests).is_empty());
    assert!(input.actions.iter().any(|action| matches!(
        action,
        ClientShellAction::Endpoint { request, .. }
            if matches!(request.method, crate::api::schema::Method::SessionPaneUnfocus(_))
    )));
}

#[test]
fn a_focused_session_pane_takes_typing_but_not_herdr_bindings() {
    let mut state = shell_with_session_pane(true);
    let frame = state.compose(106, 20).expect("frame");
    assert_eq!(
        frame.cursor.as_ref().map(|cursor| (cursor.x, cursor.y)),
        Some((4, 1))
    );

    for bytes in [b"j".as_slice(), b"\x1b".as_slice()] {
        let input = state.handle_input_bytes(bytes);
        assert_eq!(session_pane_events(&input.requests).len(), 1, "{bytes:?}");
        assert!(!input
            .requests
            .iter()
            .any(|request| matches!(request, ClientMessage::ClientShellPaneInput { .. })));
    }
    let paste = state.handle_raw_events(vec![RawInputEvent::Paste("p".into())]);
    assert!(matches!(
        &session_pane_events(&paste.requests)[..],
        [ClientPaneInputEvent::Paste(value)] if value == "p"
    ));

    // The prefix and its bindings still belong to herdr.
    let collapsed = state.sidebar_collapsed;
    let prefix = state.handle_input_bytes(b"\x02");
    assert!(session_pane_events(&prefix.requests).is_empty());
    let toggle = state.handle_input_bytes(b"b");
    assert!(session_pane_events(&toggle.requests).is_empty());
    assert_ne!(state.sidebar_collapsed, collapsed);
}

#[test]
fn unfocused_typing_stays_with_the_focused_pane() {
    let mut state = shell_with_session_pane(false);
    state.compose(106, 20).expect("frame");
    let input = state.handle_input_bytes(b"j");
    assert!(session_pane_events(&input.requests).is_empty());
    assert!(matches!(
        &input.requests[..],
        [ClientMessage::ClientShellPaneInput { pane_id, .. }] if pane_id == "pane_1"
    ));
}

#[test]
fn focus_sidebar_toggles_focus_through_the_endpoint() {
    let method_of = |input: &ClientShellInput| {
        input.actions.iter().find_map(|action| match action {
            ClientShellAction::Endpoint { request, .. } => Some(request.method.clone()),
            _ => None,
        })
    };
    let binding = || crate::input::KeybindMatch::Action(crate::input::KeybindAction::FocusSidebar);

    let mut state = shell_with_session_pane(false);
    state.compose(106, 20).expect("frame");
    let mut input = ClientShellInput::default();
    state.record_binding(binding(), &mut input);
    assert!(matches!(
        method_of(&input),
        Some(crate::api::schema::Method::SessionPaneFocus(_))
    ));

    let mut state = shell_with_session_pane(true);
    state.compose(106, 20).expect("frame");
    let mut input = ClientShellInput::default();
    state.record_binding(binding(), &mut input);
    assert!(matches!(
        method_of(&input),
        Some(crate::api::schema::Method::SessionPaneUnfocus(_))
    ));

    // Without a session pane the action does nothing.
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.compose(106, 20).expect("frame");
    let mut input = ClientShellInput::default();
    state.record_binding(binding(), &mut input);
    assert!(method_of(&input).is_none());
}

#[test]
fn slot_size_is_reported_once_per_change_and_only_with_a_pane() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    assert!(state.take_session_pane_slot_message(106, 20).is_none());

    let mut state = shell_with_session_pane(false);
    let slot = |message: Option<ClientMessage>| match message {
        Some(ClientMessage::EndpointControl { kind, data })
            if kind == crate::protocol::session_pane::SLOT_KIND =>
        {
            let slot = crate::protocol::session_pane::decode_slot(&data).unwrap();
            Some((slot.cols, slot.rows))
        }
        Some(other) => panic!("unexpected {other:?}"),
        None => None,
    };
    assert_eq!(
        slot(state.take_session_pane_slot_message(106, 20)),
        Some((20, 20))
    );
    assert_eq!(slot(state.take_session_pane_slot_message(106, 20)), None);
    state.sidebar_collapsed = true;
    assert_eq!(
        slot(state.take_session_pane_slot_message(106, 20)),
        Some((6, 20))
    );
}

#[test]
fn a_pane_that_keeps_the_surface_size_does_not_strand_the_surface() {
    // Same width as the native sidebar: no resize is sent, so the surface must survive.
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let native = state.layout(106, 20).sidebar.width;
    let mut pane = view(false);
    pane.width = native;
    let (changed, resized) = state.set_endpoint_session_pane(
        &ClientEndpointId::Local,
        None,
        SessionPaneUpdate {
            boot_id: "boot-1".into(),
            revision: 1,
            pane: Some(pane),
        },
        (106, 20),
    );
    assert!(changed);
    assert!(!resized);
    let text = frame_text(&state.compose(106, 20).expect("surface kept"));
    assert!(text.contains("SIDE-LIVE"));
}

#[test]
fn a_pane_from_another_connection_or_server_is_dropped_with_its_focus() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snapshot()));
    state.set_pane_surface(surface());
    let update = |boot: &str| SessionPaneUpdate {
        boot_id: boot.into(),
        revision: 1,
        pane: Some(view(true)),
    };
    state.set_endpoint_session_pane(
        &ClientEndpointId::Local,
        Some(1),
        update("boot-1"),
        (106, 20),
    );
    state.set_pane_surface(surface());
    state.compose(106, 20).expect("frame");
    assert!(state.take_session_pane_slot_message(106, 20).is_some());
    assert_eq!(
        session_pane_events(&state.handle_input_bytes(b"j").requests).len(),
        1
    );

    // A reconnect may land on a server that never sends `pane: null`, such as stock herdr.
    state.set_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 2, Box::new(snapshot()));
    state.set_pane_surface(surface());
    let frame = state.compose(106, 20).expect("frame");
    assert!(state.session_pane_view().is_none());
    assert!(!frame_text(&frame).contains("SIDE-LIVE"));
    let input = state.handle_input_bytes(b"j");
    assert!(session_pane_events(&input.requests).is_empty());
    assert!(matches!(
        &input.requests[..],
        [ClientMessage::ClientShellPaneInput { pane_id, .. }] if pane_id == "pane_1"
    ));
    assert!(state.take_session_pane_slot_message(106, 20).is_none());

    // The new connection's pane hears the slot again.
    state.set_endpoint_session_pane(
        &ClientEndpointId::Local,
        Some(2),
        update("boot-1"),
        (106, 20),
    );
    assert!(state.take_session_pane_slot_message(106, 20).is_some());

    // A new server boot on the same connection drops it too.
    let mut rebooted = snapshot();
    rebooted.boot_id = "boot-2".into();
    state.set_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 2, Box::new(rebooted));
    assert!(state.session_pane_view().is_none());
}
