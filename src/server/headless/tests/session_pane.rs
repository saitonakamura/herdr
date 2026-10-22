use super::*;
use crate::protocol::session_pane::{SessionPaneInput, SessionPaneSlot, SessionPaneUpdate};

fn connect(
    server: &mut HeadlessServer,
    client_id: u64,
    session_pane: bool,
) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (writer, control_rx, render_rx) = test_client_writer();
    // The writer forwards in order, so an undrained render lane would hold controls back.
    std::thread::spawn(move || while render_rx.recv().is_ok() {});
    server.handle_server_event(ServerEvent::ClientShellConnected {
        client_id,
        surface_cols: 80,
        surface_rows: 24,
        cell_width_px: 0,
        cell_height_px: 0,
        pixel_mouse: false,
        direct_graphics: false,
        endpoint_keybindings: false,
        mouse_capture: false,
        surface_active: true,
        surface_reuse: false,
        surface_delta: false,
        surface_scroll: false,
        session_pane,
        writer,
    });
    control_rx
}

fn session_pane_updates(control_rx: &std::sync::mpsc::Receiver<Vec<u8>>) -> Vec<SessionPaneUpdate> {
    std::iter::from_fn(|| {
        control_rx
            .recv_timeout(std::time::Duration::from_millis(200))
            .ok()
    })
    .filter_map(|bytes| match read_server_message(bytes) {
        ServerMessage::EndpointControl { kind, data }
            if kind == crate::protocol::session_pane::MESSAGE_KIND =>
        {
            Some(crate::protocol::session_pane::decode_update(&data).expect("update"))
        }
        _ => None,
    })
    .collect()
}

fn server_with_session_pane(bytes: &[u8]) -> (HeadlessServer, crate::terminal::TerminalId) {
    let mut server = test_headless_server();
    server.app.state.workspaces = vec![crate::workspace::Workspace::test_new("session-pane")];
    server.app.state.ensure_test_terminals();
    server.app.state.active = Some(0);
    let terminal_id = server.app.install_test_session_pane(
        crate::terminal::TerminalRuntime::test_with_screen_bytes(12, 4, bytes),
    );
    (server, terminal_id)
}

#[tokio::test]
async fn only_opted_in_clients_receive_the_session_pane_and_only_when_it_changes() {
    let (mut server, terminal_id) = server_with_session_pane(b"SIDE");
    let opted_in = connect(&mut server, 1, true);
    let legacy = connect(&mut server, 2, false);

    server.stream_session_pane();
    let updates = session_pane_updates(&opted_in);
    let [update] = updates.as_slice() else {
        panic!("expected one update, got {updates:?}");
    };
    let view = update.pane.as_ref().expect("running pane");
    assert_eq!(view.terminal_id, terminal_id.to_string());
    assert_eq!((view.width, view.collapsed_width), (26, 8));
    assert!(!view.focused);
    assert!(frame_text(&view.frame).starts_with("SIDE"));
    assert!(session_pane_updates(&legacy).is_empty());

    server.stream_session_pane();
    assert!(
        session_pane_updates(&opted_in).is_empty(),
        "unchanged pane was resent"
    );

    assert!(server.app.set_session_pane_focus(true));
    server.stream_session_pane();
    let updates = session_pane_updates(&opted_in);
    assert!(
        matches!(&updates[..], [update] if update.pane.as_ref().is_some_and(|view| view.focused)),
        "{:?}",
        updates
            .iter()
            .map(|update| update.pane.as_ref().map(|view| view.focused))
            .collect::<Vec<_>>()
    );

    // While the program is down, clients fall back to their native sidebar.
    server.app.state.session_pane.process = None;
    server.app.state.session_pane.status = crate::app::session_pane::SessionPaneStatus::Restarting;
    server.stream_session_pane();
    let updates = session_pane_updates(&opted_in);
    assert!(matches!(&updates[..], [update] if update.pane.is_none()));
    assert!(session_pane_updates(&legacy).is_empty());
    shutdown_test_runtimes(&mut server);
}

#[tokio::test]
async fn session_pane_input_reaches_the_program_and_slots_size_it() {
    let mut server = test_headless_server();
    server.app.state.workspaces = vec![crate::workspace::Workspace::test_new("session-pane")];
    server.app.state.ensure_test_terminals();
    server.app.state.active = Some(0);
    let (runtime, mut pty_rx) = crate::terminal::TerminalRuntime::test_with_channel(12, 4);
    let terminal_id = server.app.install_test_session_pane(runtime);
    let _control = connect(&mut server, 1, true);
    let _legacy = connect(&mut server, 2, false);

    let input = |terminal_id: String| SessionPaneInput {
        terminal_id,
        events: vec![crate::protocol::ClientPaneInputEvent::TextCommit(
            "j".into(),
        )],
    };
    server.handle_server_event(ServerEvent::ClientSessionPaneInput {
        client_id: 1,
        input: input("term_other".into()),
    });
    assert!(
        pty_rx.try_recv().is_err(),
        "input for another terminal reached the program"
    );
    server.handle_server_event(ServerEvent::ClientSessionPaneInput {
        client_id: 2,
        input: input(terminal_id.to_string()),
    });
    assert!(
        pty_rx.try_recv().is_err(),
        "a client that did not opt in sent input"
    );
    server.handle_server_event(ServerEvent::ClientSessionPaneInput {
        client_id: 1,
        input: input(terminal_id.to_string()),
    });
    assert_eq!(pty_rx.try_recv().expect("program input").as_ref(), b"j");

    server.handle_server_event(ServerEvent::ClientSessionPaneSlot {
        client_id: 1,
        slot: SessionPaneSlot { cols: 30, rows: 22 },
    });
    assert_eq!(server.app.state.session_pane.size, (30, 22));
    // A hidden sidebar reports no slot; the program keeps its size.
    server.handle_server_event(ServerEvent::ClientSessionPaneSlot {
        client_id: 1,
        slot: SessionPaneSlot { cols: 0, rows: 22 },
    });
    assert_eq!(server.app.state.session_pane.size, (30, 22));
    shutdown_test_runtimes(&mut server);
}

#[tokio::test]
async fn listings_and_snapshots_leave_the_session_pane_out() {
    let (mut server, terminal_id) = server_with_session_pane(b"SIDE");
    let panes = headless_pane_list(&mut server);
    assert_eq!(panes.len(), 1);
    assert!(panes
        .iter()
        .all(|pane| pane.terminal_id != terminal_id.to_string()));
    let snapshot = server.app.session_snapshot();
    assert_eq!(snapshot.panes.len(), 1);
    assert!(snapshot.agents.is_empty());
    let persisted = crate::persist::capture(
        &server.app.state.workspaces,
        &server.app.state.terminals,
        &server.app.terminal_runtimes,
        server.app.state.active,
        server.app.state.selected,
    );
    let persisted = serde_json::to_string(&persisted).expect("persisted snapshot");
    assert!(!persisted.contains(terminal_id.as_str()));
    assert!(!persisted.contains("/session-pane"));
    server.app.state.assert_invariants_for_test();
    shutdown_test_runtimes(&mut server);
}

#[tokio::test]
async fn focusing_a_pane_takes_focus_from_the_session_pane() {
    let (mut server, _) = server_with_session_pane(b"SIDE");
    assert!(server.app.set_session_pane_focus(true));
    let pane_id = headless_pane_list(&mut server)[0].pane_id.clone();
    let response = server.app.handle_api_request(api::schema::Request {
        id: "focus".into(),
        method: api::schema::Method::PaneFocus(api::schema::PaneTarget { pane_id }),
    });
    assert!(response.contains("\"result\""), "{response}");
    assert!(!server.app.state.session_pane.focused);
    shutdown_test_runtimes(&mut server);
}

#[tokio::test]
async fn an_opted_in_client_hears_once_that_no_pane_runs() {
    let mut server = test_headless_server();
    server.app.state.workspaces = vec![crate::workspace::Workspace::test_new("session-pane")];
    server.app.state.ensure_test_terminals();
    server.app.state.active = Some(0);
    let opted_in = connect(&mut server, 1, true);
    let legacy = connect(&mut server, 2, false);
    server.stream_session_pane();
    let updates = session_pane_updates(&opted_in);
    assert!(matches!(&updates[..], [update] if update.pane.is_none()));
    assert!(session_pane_updates(&legacy).is_empty());
    server.stream_session_pane();
    assert!(session_pane_updates(&opted_in).is_empty());
    shutdown_test_runtimes(&mut server);
}

#[tokio::test]
async fn only_a_client_showing_the_pane_routes_its_keyboard_mode_there() {
    let (mut server, _) = server_with_session_pane(b"SIDE");
    let _shown = connect(&mut server, 1, true);
    let _hidden = connect(&mut server, 2, true);
    let _unreported = connect(&mut server, 3, true);
    server.handle_server_event(ServerEvent::ClientSessionPaneSlot {
        client_id: 1,
        slot: SessionPaneSlot { cols: 26, rows: 24 },
    });
    server.handle_server_event(ServerEvent::ClientSessionPaneSlot {
        client_id: 2,
        slot: SessionPaneSlot { cols: 0, rows: 24 },
    });
    assert!(server.app.set_session_pane_focus(true));
    let session_pane = server.app.state.session_pane.pane_id();
    let focused = |server: &HeadlessServer, client_id| {
        server
            .shell_focused_runtime(client_id)
            .map(|(_, pane_id)| pane_id)
    };
    assert_eq!(focused(&server, 1), session_pane);
    assert_ne!(focused(&server, 2), session_pane);
    assert_ne!(focused(&server, 3), session_pane);
    shutdown_test_runtimes(&mut server);
}

#[tokio::test]
async fn synchronized_output_holds_the_pane_back_only_for_a_while() {
    let (mut server, _) = server_with_session_pane(b"\x1b[?2026hHALF");
    let control = connect(&mut server, 1, true);
    server.stream_session_pane();
    assert!(session_pane_updates(&control).is_empty());
    server.session_pane_sync_since = Some(Instant::now() - std::time::Duration::from_secs(2));
    server.run_session_pane_tasks(Instant::now());
    let updates = session_pane_updates(&control);
    let texts = updates
        .iter()
        .map(|update| update.pane.as_ref().map(|view| frame_text(&view.frame)))
        .collect::<Vec<_>>();
    // Like any pane mid-frame, it shows the last finished frame rather than nothing.
    assert!(matches!(&texts[..], [Some(_)]), "{texts:?}");
    shutdown_test_runtimes(&mut server);
}
