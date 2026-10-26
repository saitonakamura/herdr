//! The endpoint's session pane, drawn in the sidebar slot in place of the native
//! sidebar. Mobile layouts and multi-endpoint shells keep the native sidebar.

use super::*;
use crate::protocol::session_pane::{SessionPaneSlot, SessionPaneUpdate, SessionPaneView};

const SET_WIDTH_METHOD: &str = "session_pane.set_width";

impl ClientShellState {
    /// The session pane this shell draws, if any.
    pub(super) fn session_pane_view(&self) -> Option<&SessionPaneView> {
        session_pane_view_in(&self.endpoints)
    }

    /// Expanded and collapsed widths of the session pane, for the layout.
    pub(super) fn session_pane_widths(&self) -> Option<(u16, u16)> {
        self.session_pane_view().map(|view| {
            let expanded = self.session_pane_width_preview.unwrap_or(view.width);
            (expanded.max(1), view.collapsed_width.max(1))
        })
    }

    /// Whether the endpoint lets the divider set the pane's width. A server without
    /// the method keeps the divider still and shows no notice.
    pub(super) fn session_pane_resizable(&self) -> bool {
        self.session_pane_view().is_some()
            && self
                .endpoints
                .iter()
                .find(|endpoint| endpoint.endpoint_id == self.active_endpoint_id)
                .and_then(|endpoint| endpoint.methods.as_ref())
                .is_some_and(|methods| methods.iter().any(|method| method == SET_WIDTH_METHOD))
    }

    /// Starts a divider drag, or on a double-click goes back to the configured width.
    pub(super) fn press_session_pane_divider(
        &mut self,
        column: u16,
        double_click: bool,
        outcome: &mut ClientShellInput,
    ) {
        if double_click {
            self.preview_session_pane_width(self.config.sidebar_width, outcome);
            self.push_endpoint_method_with_kind(
                crate::api::schema::Method::SessionPaneSetWidth(
                    crate::api::schema::SessionPaneSetWidthParams { width: None },
                ),
                PendingEndpointKind::SessionPaneWidth,
                outcome,
            );
        } else {
            self.chrome_drag = Some(ClientChromeDrag::SessionPaneWidth);
            self.drag_session_pane_divider(column, outcome);
        }
    }

    pub(super) fn drag_session_pane_divider(
        &mut self,
        column: u16,
        outcome: &mut ClientShellInput,
    ) {
        self.preview_session_pane_width(column.saturating_add(1), outcome);
    }

    /// Asks the endpoint for the dragged width, which every client then follows.
    pub(super) fn release_session_pane_divider(&mut self, outcome: &mut ClientShellInput) {
        let Some(width) = self.session_pane_width_preview else {
            return;
        };
        if self
            .session_pane_view()
            .is_some_and(|view| view.width == width)
        {
            self.session_pane_width_preview = None;
            return;
        }
        self.push_endpoint_method_with_kind(
            crate::api::schema::Method::SessionPaneSetWidth(
                crate::api::schema::SessionPaneSetWidthParams { width: Some(width) },
            ),
            PendingEndpointKind::SessionPaneWidth,
            outcome,
        );
    }

    fn preview_session_pane_width(&mut self, width: u16, outcome: &mut ClientShellInput) {
        let (min, max) = crate::config::validated_sidebar_bounds(
            self.config.sidebar_min_width,
            self.config.sidebar_max_width,
        )
        .unwrap_or((18, 36));
        let width = width.clamp(min, max);
        let Some((current, _)) = self.session_pane_widths() else {
            return;
        };
        self.session_pane_width_preview = Some(width);
        if current != width {
            self.invalidate_pane_surface();
            outcome.repaint = true;
            outcome.resize = true;
        }
    }

    /// A drag or its request still owns the width, so updates must not replace it.
    fn session_pane_width_held(&self) -> bool {
        matches!(self.chrome_drag, Some(ClientChromeDrag::SessionPaneWidth))
            || self
                .pending_requests
                .values()
                .any(|pending| matches!(pending.kind, PendingEndpointKind::SessionPaneWidth))
    }

    /// Whether the session pane holds keyboard focus and this shell shows it.
    pub(super) fn session_pane_focused(&self) -> bool {
        self.session_pane_view().is_some_and(|view| view.focused) && self.session_pane_shown()
    }

    pub(super) fn session_pane_input_target(&self) -> Option<ClientInputTarget> {
        self.session_pane_focused()
            .then(|| self.session_pane_view())
            .flatten()
            .map(|view| ClientInputTarget::SessionPane(view.terminal_id.clone()))
    }

    /// Gives the session pane keyboard focus, or takes it back. Focus is the
    /// endpoint's to grant, so this asks it and waits for the next update.
    pub(super) fn toggle_session_pane_focus(&mut self, outcome: &mut ClientShellInput) {
        let Some(focused) = self.session_pane_view().map(|view| view.focused) else {
            return;
        };
        if !focused && !self.session_pane_shown() {
            return;
        }
        let params = crate::api::schema::EmptyParams::default();
        let method = if focused {
            crate::api::schema::Method::SessionPaneUnfocus(params)
        } else {
            crate::api::schema::Method::SessionPaneFocus(params)
        };
        self.push_endpoint_method(method, outcome);
    }

    fn session_pane_shown(&self) -> bool {
        self.last_composed_size
            .is_some_and(|(cols, rows)| self.layout(cols, rows).sidebar.width > 0)
    }

    /// Drops a pane that came from another connection or server boot. A server
    /// without the feature never says the pane is gone, and focus on a pane nobody
    /// runs would swallow every key.
    pub(super) fn forget_stale_session_pane(
        &mut self,
        index: usize,
        generation: Option<u64>,
        boot_id: &str,
    ) {
        let Some(endpoint) = self.endpoints.get_mut(index) else {
            return;
        };
        if endpoint
            .session_pane_origin
            .as_ref()
            .is_none_or(|(origin_generation, origin_boot)| {
                *origin_generation == generation && origin_boot == boot_id
            })
        {
            return;
        }
        endpoint.session_pane = None;
        endpoint.session_pane_origin = None;
        if endpoint.endpoint_id == self.active_endpoint_id {
            self.session_pane_slot_sent = None;
            self.session_pane_width_preview = None;
        }
    }

    /// Stores an endpoint's session pane update for a `cols` x `rows` terminal.
    /// Returns whether anything changed and whether the pane surface changed size.
    pub(crate) fn set_endpoint_session_pane(
        &mut self,
        endpoint_id: &ClientEndpointId,
        generation: Option<u64>,
        update: SessionPaneUpdate,
        (cols, rows): (u16, u16),
    ) -> (bool, bool) {
        let previous_size = self.surface_size(cols, rows);
        let active = endpoint_id == &self.active_endpoint_id;
        // Once no drag or request holds it, the endpoint's width replaces the preview.
        let preview_done = active
            && update.pane.is_some()
            && self.session_pane_width_preview.is_some()
            && !self.session_pane_width_held();
        let Some(endpoint) = self
            .endpoints
            .iter_mut()
            .find(|endpoint| &endpoint.endpoint_id == endpoint_id)
        else {
            return (false, false);
        };
        let origin = (generation, update.boot_id);
        if endpoint.session_pane_origin.as_ref() != Some(&origin) {
            // A new connection or server has not heard this client's slot yet.
            endpoint.session_pane_origin = Some(origin);
            if active {
                self.session_pane_slot_sent = None;
            }
        }
        if endpoint.session_pane == update.pane && !preview_done {
            return (false, false);
        }
        endpoint.session_pane = update.pane;
        if preview_done || self.session_pane_view().is_none() {
            self.session_pane_width_preview = None;
        }
        // Only a new size makes the endpoint send a new surface to replace a stale one.
        let resized = self.surface_size(cols, rows) != previous_size;
        if resized {
            self.invalidate_pane_surface();
        }
        (true, resized)
    }

    /// The slot size to report to the endpoint, when it changed since the last report.
    pub(crate) fn take_session_pane_slot_message(
        &mut self,
        cols: u16,
        rows: u16,
    ) -> Option<crate::protocol::ClientMessage> {
        self.session_pane_view()?;
        let slot = session_pane_slot(self.layout(cols, rows).sidebar, self.sidebar_collapsed);
        let size = (slot.width, slot.height);
        if self.session_pane_slot_sent == Some(size) {
            return None;
        }
        self.session_pane_slot_sent = Some(size);
        crate::protocol::session_pane::slot_message(SessionPaneSlot {
            cols: size.0,
            rows: size.1,
        })
        .ok()
    }
}

/// The part of the sidebar slot the pane gets. Expanded, the last column is the
/// divider, as in the native sidebar.
pub(super) fn session_pane_slot(sidebar: Rect, collapsed: bool) -> Rect {
    if collapsed || sidebar.width < 2 {
        return sidebar;
    }
    Rect {
        width: sidebar.width - 1,
        ..sidebar
    }
}

/// The divider column beside an expanded session pane, if it has one.
pub(super) fn session_pane_divider(sidebar: Rect, collapsed: bool) -> Rect {
    let slot = session_pane_slot(sidebar, collapsed);
    if slot == sidebar {
        return Rect::default();
    }
    Rect::new(slot.right(), sidebar.y, 1, sidebar.height)
}

/// The session pane a shell with these endpoints draws, if any.
pub(super) fn session_pane_view_in(endpoints: &[ClientShellEndpoint]) -> Option<&SessionPaneView> {
    let [endpoint] = endpoints else {
        return None;
    };
    if endpoint.status != ClientEndpointStatus::Online {
        return None;
    }
    endpoint.session_pane.as_ref()
}

/// Draws the session pane into its slot and returns its hit region. While the pane
/// holds focus, also returns its cursor in frame coordinates (`Some(None)` hides it).
pub(super) fn compose_session_pane(
    frame: &mut FrameData,
    view: Option<&SessionPaneView>,
    slot: Rect,
) -> (
    Option<PaneHit>,
    Option<Option<crate::protocol::CursorState>>,
) {
    let Some(view) = view.filter(|_| slot.width > 0 && slot.height > 0) else {
        return (None, None);
    };
    let graphics = std::mem::take(&mut frame.graphics);
    let cursor = frame.cursor.take();
    blit_pane_surface(frame, &view.frame, slot);
    let session_cursor = frame.cursor.take();
    frame.cursor = cursor;
    frame.graphics = graphics;
    let hit = PaneHit {
        rect: slot,
        inner_rect: slot,
        scrollbar_rect: None,
        scroll: None,
        pane_id: view.terminal_id.clone(),
        popup: false,
        session_pane: true,
        mouse_reporting: view.mouse_reporting,
        sgr_pixel_mouse: view.sgr_pixel_mouse,
        pixel_width: view.pixel_width,
        pixel_height: view.pixel_height,
    };
    (Some(hit), view.focused.then_some(session_cursor))
}
