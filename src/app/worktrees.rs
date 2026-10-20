use super::App;

impl App {
    pub(crate) fn should_shutdown_workspace_terminal_runtimes_for_worktree_remove(
        force: bool,
    ) -> bool {
        force || cfg!(windows)
    }

    pub(crate) fn shutdown_terminal_runtimes_for_worktree_remove(
        &mut self,
        pane_ids: &[crate::layout::PaneId],
    ) -> Vec<crate::layout::PaneId> {
        let mut shutdown_panes = Vec::new();
        for &pane_id in pane_ids {
            let Some((ws_idx, _)) = self.find_pane(pane_id) else {
                continue;
            };
            let Some(terminal_id) = self.state.terminal_id_for_pane(ws_idx, pane_id) else {
                continue;
            };
            if self.terminal_runtimes.get(&terminal_id).is_none() {
                continue;
            }
            tracing::debug!(
                workspace_index = ws_idx,
                terminal_id = %terminal_id,
                "shutting down terminal runtime before worktree removal"
            );
            *self
                .pending_worktree_remove_runtime_exits
                .entry(pane_id)
                .or_default() += 1;
            shutdown_panes.push(pane_id);
            self.shutdown_terminal_runtime(terminal_id);
        }
        shutdown_panes
    }
}

#[cfg(test)]
mod tests {
    use super::App;

    #[test]
    fn worktree_remove_runtime_shutdown_policy_preserves_windows_safe_remove() {
        assert_eq!(
            App::should_shutdown_workspace_terminal_runtimes_for_worktree_remove(false),
            cfg!(windows)
        );
        assert!(App::should_shutdown_workspace_terminal_runtimes_for_worktree_remove(true));
    }
}
