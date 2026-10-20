use crate::api::schema::AgentStatus;
use crate::protocol::{ClientShellAgent, ClientShellSnapshot, PaneSurfaceFrame};

/// Done is the server's: it clears on one acknowledgement there (a pane focused in a
/// client whose outer terminal has focus) for every client. So the client neither
/// projects done from completions nor acknowledges what it presents.
#[derive(Clone, Debug, Default)]
pub(super) struct EndpointAgentPresentation;

impl EndpointAgentPresentation {
    pub(super) fn receive_completions(
        &mut self,
        _generation: Option<u64>,
        _completions: crate::protocol::endpoint::EndpointAgentCompletions,
    ) {
    }

    #[cfg(test)]
    pub(super) fn project_snapshot(&mut self, snapshot: &mut ClientShellSnapshot) {
        self.project_snapshot_for_generation(snapshot, None);
    }

    pub(super) fn project_snapshot_for_generation(
        &mut self,
        _snapshot: &mut ClientShellSnapshot,
        _generation: Option<u64>,
    ) {
    }

    pub(super) fn acknowledge_surface(
        &mut self,
        _snapshot: &mut ClientShellSnapshot,
        _surface: &PaneSurfaceFrame,
        _outer_focused: Option<bool>,
    ) -> bool {
        false
    }

    pub(super) fn seen(&self, agent: &ClientShellAgent) -> bool {
        agent.agent_status != AgentStatus::Done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{FrameData, PaneSurfacePane, SurfaceRect};

    fn agent(status: AgentStatus, sequence: u64) -> ClientShellAgent {
        ClientShellAgent {
            repo: None,
            pane_id: "agent-pane".into(),
            workspace_id: "workspace".into(),
            tab_id: "tab".into(),
            name: None,
            display_agent: None,
            agent: None,
            title: None,
            terminal_title: None,
            terminal_title_stripped: None,
            agent_status: status,
            state_change_seq: sequence,
            state_labels: Vec::new(),
            tokens: Vec::new(),
            focused: true,
        }
    }

    fn snapshot(status: AgentStatus, sequence: u64, revision: u64) -> ClientShellSnapshot {
        let mut snapshot = crate::client::shell::tests::snapshot();
        snapshot.boot_id = "endpoint-boot".into();
        snapshot.revision = revision;
        snapshot.agents = vec![agent(status, sequence)];
        snapshot
    }

    fn surface(revision: u64) -> PaneSurfaceFrame {
        let rect = SurfaceRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        PaneSurfaceFrame {
            boot_id: "endpoint-boot".into(),
            projection_revision: revision,
            surface_revision: 1,
            frame: FrameData {
                cells: Vec::new(),
                width: 0,
                height: 0,
                cursor: None,
                hyperlinks: Vec::new(),
                graphics: Vec::new(),
            },
            panes: vec![PaneSurfacePane {
                pane_id: "agent-pane".into(),
                content_revision: 1,
                rect,
                inner_rect: rect,
                scrollbar_rect: None,
                scroll: None,
                focused: true,
                mouse_reporting: false,
                sgr_pixel_mouse: false,
                alternate_screen_active: false,
                pixel_width: 0,
                pixel_height: 0,
            }],
            splits: Vec::new(),
            popup: None,
            graphics: Default::default(),
        }
    }

    #[test]
    fn server_done_survives_projection_and_a_presented_surface() {
        let mut presentation = EndpointAgentPresentation;
        let mut working = snapshot(AgentStatus::Working, 4, 1);
        presentation.project_snapshot(&mut working);
        let mut completed = snapshot(AgentStatus::Done, 5, 2);

        presentation.project_snapshot(&mut completed);
        assert!(!presentation.acknowledge_surface(&mut completed, &surface(2), Some(true)));

        assert_eq!(completed.agents[0].agent_status, AgentStatus::Done);
        assert!(!presentation.seen(&completed.agents[0]));
    }

    #[test]
    fn server_idle_is_not_projected_to_done_from_observed_work() {
        let mut presentation = EndpointAgentPresentation;
        let mut working = snapshot(AgentStatus::Working, 4, 1);
        presentation.project_snapshot(&mut working);
        let mut acknowledged = snapshot(AgentStatus::Idle, 5, 2);

        presentation.project_snapshot(&mut acknowledged);

        assert_eq!(acknowledged.agents[0].agent_status, AgentStatus::Idle);
        assert!(presentation.seen(&acknowledged.agents[0]));
    }
}
