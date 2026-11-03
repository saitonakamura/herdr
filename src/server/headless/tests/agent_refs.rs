use super::*;

/// Fork 061: a retained patch decorates the rows it sends exactly as the full
/// renderer would, so a client's committed surface never depends on which path drew it.
#[tokio::test]
async fn retained_rows_carry_the_same_agent_reference_decoration_as_a_full_render() {
    let (mut server, _control_rx, render_rx, pane_id) =
        retained_test_server_with_control(b"start\r\n");
    server.app.state.ensure_test_terminals();
    let terminal_id = server.app.state.workspaces[0].tabs[0].panes[&pane_id]
        .attached_terminal_id
        .clone();
    let terminal = server.app.state.terminals.get_mut(&terminal_id).unwrap();
    terminal.agent_id = crate::terminal::AgentId::parse("8c1ee73d");
    terminal.task_status = Some(crate::api::schema::TaskStatus::NeedsUser);
    server.render_and_stream();
    let _ = render_rx.recv_timeout(Duration::from_secs(1)).unwrap();

    write_shared_test_pane(&mut server, pane_id, b"the child (8c1ee73d) needs you\r\n");
    assert!(server.render_retained_pane_surface_and_stream(&HashSet::from([pane_id])));
    assert!(matches!(
        read_server_message(render_rx.recv_timeout(Duration::from_secs(1)).unwrap()),
        ServerMessage::PaneSurfacePatch(_)
    ));
    let retained = server.clients[&1]
        .render_state
        .last_pane_surface()
        .expect("retained surface")
        .frame
        .clone();
    let text = frame_text(&retained);
    assert!(
        text.contains("the child (8c1ee73d) needs you  asking · w1:p1"),
        "{text}"
    );
    let retained = retained.cells;

    server.render_and_stream();
    let full = &server.clients[&1]
        .render_state
        .last_pane_surface()
        .expect("full surface")
        .frame
        .cells;
    assert_eq!(&retained, full);
    shutdown_test_runtimes(&mut server);
}

/// Fork 061: loom changes a node's task status through the API, and every pane
/// naming that node must redraw, so the request counts as a UI change.
#[tokio::test]
async fn a_task_status_change_redraws_while_agent_references_are_on() {
    let (mut server, _control_rx, _render_rx, pane_id) =
        retained_test_server_with_control(b"start\r\n");
    let target = server.app.public_pane_id(0, pane_id).unwrap();
    let set = |server: &mut HeadlessServer, status| {
        let (respond_to, _response_rx) = std::sync::mpsc::channel();
        server.handle_api_request_with_shutdown_check(api::ApiRequestMessage {
            request: api::schema::Request {
                id: "task".into(),
                method: api::schema::Method::AgentSetTaskStatus(
                    api::schema::AgentSetTaskStatusParams {
                        target: target.clone(),
                        status,
                    },
                ),
            },
            respond_to,
            response_write_complete: None,
        })
    };
    assert!(set(&mut server, api::schema::TaskStatus::NeedsUser));
    server.app.state.agent_references.enabled = false;
    assert!(!set(&mut server, api::schema::TaskStatus::Running));
    shutdown_test_runtimes(&mut server);
}
