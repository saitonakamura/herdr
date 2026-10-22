//! Delivers the session pane to opted-in shell clients and routes their input and
//! slot sizes back. Its output redraws only its own slot: it never joins a pane
//! surface render.

use std::time::Instant;

use tracing::warn;

use super::{
    apply_client_terminal_input_events, client_pane_input_has_interaction, ClientShellInputTarget,
    HeadlessServer,
};
use crate::protocol::session_pane::{
    SessionPaneInput, SessionPaneSlot, SessionPaneUpdate, SessionPaneView,
};
use crate::protocol::{self, ServerMessage};
use crate::server::clients::SessionPaneSentState;

impl HeadlessServer {
    pub(super) fn any_client_receives_session_pane(&self) -> bool {
        self.clients
            .values()
            .any(|client| client.session_pane.is_some() && client.writer.is_some())
    }

    pub(super) fn run_session_pane_tasks(&mut self, now: Instant) {
        self.app.run_session_pane_restart_if_due(now);
        if let Some(message) = self.app.take_session_pane_notice() {
            self.send_to_client_shells(ServerMessage::SemanticNotification(
                protocol::SemanticNotification {
                    kind: protocol::SemanticNotificationKind::Custom,
                    title: "sidebar pane stopped".to_string(),
                    body: Some(message),
                    sound: None,
                    agent: None,
                    workspace_id: None,
                    tab_id: None,
                    pane_id: None,
                    position: None,
                },
            ));
        }
    }

    pub(super) fn handle_session_pane_slot(
        &mut self,
        client_id: u64,
        slot: SessionPaneSlot,
    ) -> bool {
        let Some(delivery) = self
            .clients
            .get_mut(&client_id)
            .and_then(|client| client.session_pane.as_mut())
        else {
            return false;
        };
        delivery.slot = Some((slot.cols, slot.rows));
        self.sync_session_pane_size()
    }

    /// Sizes the program to the foreground client's slot, else to any client's.
    pub(super) fn sync_session_pane_size(&mut self) -> bool {
        let slot_of = |client: &crate::server::clients::ClientConnection| {
            client
                .session_pane
                .as_ref()
                .and_then(|delivery| delivery.slot)
                .filter(|(cols, rows)| *cols > 0 && *rows > 0)
                .map(|slot| (slot, client.cell_size))
        };
        let chosen = self
            .foreground_client_id
            .and_then(|id| self.clients.get(&id))
            .and_then(slot_of)
            .or_else(|| self.clients.values().find_map(slot_of));
        let Some(((cols, rows), cell_size)) = chosen else {
            return false;
        };
        self.app.resize_session_pane(cols, rows, cell_size)
    }

    pub(super) fn handle_session_pane_input(
        &mut self,
        client_id: u64,
        input: SessionPaneInput,
    ) -> bool {
        if self.handoff_in_progress
            || !self.clients.get(&client_id).is_some_and(|client| {
                client.is_active_shell_client() && client.session_pane.is_some()
            })
        {
            return false;
        }
        let Some((terminal_id, pane_id)) = self
            .app
            .state
            .session_pane
            .process
            .as_ref()
            .map(|process| (process.terminal_id.clone(), process.pane_id))
        else {
            return false;
        };
        if terminal_id.as_str() != input.terminal_id {
            return false;
        }
        let pixel_mouse = self.clients.get(&client_id).is_some_and(|client| {
            client.pixel_mouse && client.host_sgr_pixels_active == Some(true)
        });
        let mut events = input.events;
        let Some(runtime) = self.app.terminal_runtimes.get(&terminal_id) else {
            return false;
        };
        crate::server::pane_input::downgrade_ineligible_pixel_mouse(
            &mut events,
            pixel_mouse,
            runtime.current_size(),
            runtime.pixel_size(),
        );
        let interaction = client_pane_input_has_interaction(&events);
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.track_shell_input(
                ClientShellInputTarget::SessionPane(input.terminal_id.clone()),
                &events,
            );
        }
        let foreground_changed = interaction && self.promote_client_to_foreground(client_id);
        let resized = foreground_changed && self.sync_session_pane_size();
        let Some(runtime) = self.app.terminal_runtimes.get(&terminal_id) else {
            return foreground_changed;
        };
        let mut keyboard_forwarded = false;
        let result =
            apply_client_terminal_input_events(runtime, &events, false, &mut keyboard_forwarded);
        if keyboard_forwarded {
            self.note_interactive_input(pane_id, Instant::now());
        }
        if let Err(err) = result {
            warn!(client_id, terminal_id = %input.terminal_id, err = %err, "session pane input failed");
        }
        foreground_changed || resized
    }

    fn session_pane_sent_state(&self) -> Option<SessionPaneSentState> {
        let pane = &self.app.state.session_pane;
        let process = pane.process.as_ref()?;
        let runtime = self.app.terminal_runtimes.get(&process.terminal_id)?;
        let (rows, cols) = runtime.current_size();
        Some(SessionPaneSentState {
            terminal_id: process.terminal_id.to_string(),
            content_seq: runtime.content_seq(),
            size: (cols, rows),
            focused: pane.focused,
            widths: (pane.config.width, pane.config.collapsed_width),
            mouse: (
                runtime.mouse_reporting_enabled(),
                runtime.sgr_pixel_mouse_enabled(),
            ),
        })
    }

    fn render_session_pane_view(&self, state: &SessionPaneSentState) -> Option<SessionPaneView> {
        let cell_size = self
            .foreground_client_id
            .and_then(|id| self.clients.get(&id))
            .map(|client| client.cell_size)
            .filter(|cell_size| cell_size.is_known());
        crate::server::client_shell::render_session_pane_view(&self.app, state.size, cell_size)
    }

    /// Sends the pane to each opted-in client whose copy is stale. Rendering happens
    /// at most once per call, and only when some client needs it.
    pub(super) fn stream_session_pane(&mut self) {
        if !self.any_client_receives_session_pane() {
            return;
        }
        self.sync_session_pane_size();
        if self
            .app
            .session_pane_runtime()
            .is_some_and(|runtime| runtime.synchronized_output_state().0)
        {
            // The program is mid-frame; its end of synchronized output wakes us again.
            return;
        }
        let state = self.session_pane_sent_state();
        let stale: Vec<u64> = self
            .clients
            .iter()
            .filter(|(_, client)| client.writer.is_some())
            .filter_map(|(&id, client)| {
                let delivery = client.session_pane.as_ref()?;
                (delivery.last_sent != state).then_some(id)
            })
            .collect();
        if stale.is_empty() {
            return;
        }
        let view = match &state {
            Some(state) => match self.render_session_pane_view(state) {
                Some(view) => Some(view),
                None => return,
            },
            None => None,
        };
        let mut broken = Vec::new();
        for client_id in stale {
            let Some(client) = self.clients.get_mut(&client_id) else {
                continue;
            };
            let Some(delivery) = client.session_pane.as_mut() else {
                continue;
            };
            delivery.revision = delivery.revision.saturating_add(1);
            let update = SessionPaneUpdate {
                boot_id: self.client_shell_boot_id.clone(),
                revision: delivery.revision,
                pane: view.clone(),
            };
            let framed =
                crate::protocol::session_pane::update_message(&update).and_then(|message| {
                    Self::frame_server_message(&message).map_err(|err| err.to_string())
                });
            let framed = match framed {
                Ok(framed) => framed,
                Err(err) => {
                    warn!(client_id, err = %err, "failed to encode session pane");
                    continue;
                }
            };
            let Some(writer) = client.writer.as_ref() else {
                continue;
            };
            if writer.control.send(framed).is_err() {
                broken.push(client_id);
                continue;
            }
            delivery.last_sent = state.clone();
        }
        for client_id in broken {
            self.remove_client_and_resize_if_needed(client_id);
        }
    }
}
