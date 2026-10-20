use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::api::schema::{
    Request, ResponseResult, TabTarget, WorktreeCreateParams, WorktreeRemoveTabParams,
};
use crate::app::App;
use crate::events::{ApiWorktreeAddRequest, ApiWorktreeRemoveRequest, AppEvent};

use super::super::responses::{encode_error, encode_success};
use super::{absolute_user_path, ApiFailure, WorktreeSource};

impl App {
    pub(crate) fn handle_deferred_worktree_api_request(
        &mut self,
        request: Request,
        respond_to: std::sync::mpsc::Sender<String>,
        client_local: bool,
    ) -> bool {
        match request.method {
            crate::api::schema::Method::WorktreeList(_)
            | crate::api::schema::Method::WorktreeOpen(_) => {
                self.start_api_worktree_read(request, respond_to, client_local);
                true
            }
            crate::api::schema::Method::WorktreeCreate(ref params) => {
                if let Err(err) = validate_worktree_create(params) {
                    Self::send_api_response(
                        respond_to,
                        encode_error(request.id, err.code, err.message),
                    );
                } else {
                    self.start_api_worktree_read(request, respond_to, client_local);
                }
                true
            }
            // The workspace-addressed form: a worktree is a tab now, so this
            // removes the workspace's one worktree tab.
            crate::api::schema::Method::WorktreeRemove(params) => {
                self.start_api_worktree_remove(
                    request.id,
                    WorktreeRemoveTabParams {
                        tab_id: None,
                        workspace_id: Some(params.workspace_id),
                        force: params.force,
                        trust_repository: params.trust_repository,
                    },
                    respond_to,
                );
                true
            }
            crate::api::schema::Method::WorktreeRemoveTab(params) => {
                self.start_api_worktree_remove(request.id, params, respond_to);
                true
            }
            _ => false,
        }
    }

    fn send_api_response(respond_to: std::sync::mpsc::Sender<String>, response: String) {
        let _ = respond_to.send(response);
    }

    fn next_api_worktree_operation_id(&mut self) -> u64 {
        let id = self.next_api_worktree_operation_id;
        self.next_api_worktree_operation_id = self.next_api_worktree_operation_id.saturating_add(1);
        id
    }

    /// Starts `git worktree add` once background discovery has found the repo.
    pub(super) fn start_api_worktree_add(
        &mut self,
        id: String,
        params: WorktreeCreateParams,
        source: WorktreeSource,
        respond_to: std::sync::mpsc::Sender<String>,
    ) {
        let branch = params
            .branch
            .unwrap_or_else(|| {
                let seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|duration| duration.as_micros().min(u128::from(u64::MAX)) as u64)
                    .unwrap_or(0);
                crate::worktree::generated_branch_slug(seed)
            })
            .trim()
            .to_string();
        let base = params.base.unwrap_or_else(|| "HEAD".into());
        let checkout_path = match params.path {
            Some(path) => match absolute_user_path(&path) {
                Ok(path) => path,
                Err(err) => {
                    Self::send_api_response(respond_to, encode_error(id, err.code, err.message));
                    return;
                }
            },
            None => crate::worktree::default_checkout_path(
                &self.state.worktree_directory,
                &source.repo_name,
                &branch,
            ),
        };
        let checkout_key = crate::worktree::canonical_or_original(&checkout_path);
        if self
            .pending_api_worktree_creates
            .contains_key(&checkout_key)
            || self
                .pending_api_worktree_remove_paths
                .contains_key(&checkout_key)
        {
            Self::send_api_response(
                respond_to,
                encode_error(
                    id,
                    "worktree_operation_in_progress",
                    "worktree operation is already in progress for this checkout",
                ),
            );
            return;
        }
        let operation_id = self.next_api_worktree_operation_id();
        self.pending_api_worktree_creates
            .insert(checkout_key.clone(), operation_id);

        let parent_dir = checkout_path.parent().map(Path::to_path_buf);
        let target_workspace_id = source
            .workspace_idx
            .and_then(|idx| self.state.workspaces.get(idx).map(|ws| ws.id.clone()));
        let api_request = ApiWorktreeAddRequest {
            id,
            operation_id,
            checkout_key,
            target_workspace_id,
            source_checkout_path: source.source_checkout_path,
            source_repo_root: source.source_repo_root,
            repo_key: source.repo_key,
            repo_name: source.repo_name,
            label: params.label,
            focus: params.focus,
            respond_to,
        };
        let source_checkout_path = api_request.source_checkout_path.clone();
        let event_tx = self.event_tx.clone();
        let finished = crate::events::WorktreeAddResult {
            path: checkout_path,
            api_request: Some(api_request),
            result: Ok(()),
            post_create: Ok(()),
        };
        let post_create = self.state.worktree_post_create.clone();
        let spawned = crate::thread_spawn::spawn_named_with(
            "herdr-worktree-add",
            finished,
            move |mut finished| {
                finished.result = if let Some(parent_dir) = parent_dir {
                    std::fs::create_dir_all(&parent_dir).map_err(|err| err.to_string())
                } else {
                    Ok(())
                }
                .and_then(|()| {
                    crate::worktree::run_worktree_add_command(
                        &source_checkout_path,
                        &finished.path,
                        &branch,
                        &base,
                        params.trust_repository,
                    )
                });
                if finished.result.is_ok() {
                    finished.post_create = crate::worktree::run_post_create_hook(
                        &post_create,
                        &finished.path,
                        &source_checkout_path,
                        &branch,
                    );
                }
                let _ = event_tx.blocking_send(AppEvent::WorktreeAddFinished(Box::new(finished)));
            },
        );
        if let Err((mut finished, err)) = spawned {
            finished.result = Err(format!("could not start worktree creation: {err}"));
            self.queue_worktree_spawn_failure(AppEvent::WorktreeAddFinished(Box::new(finished)));
        }
    }

    /// Reports a worker that never started through the normal completion path,
    /// so pending-operation and runtime cleanup stay in one place.
    fn queue_worktree_spawn_failure(&self, finished: AppEvent) {
        tracing::warn!("failed to spawn worktree operation thread");
        match self.event_tx.try_send(finished) {
            Ok(()) => {}
            // The event loop drains this channel, so wait for room on a task
            // instead of dropping the only completion for this request.
            Err(tokio::sync::mpsc::error::TrySendError::Full(finished)) => {
                let event_tx = self.event_tx.clone();
                tokio::spawn(async move {
                    let _ = event_tx.send(finished).await;
                });
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
        }
    }

    /// The worktree tab a remove targets: the tab named, or the workspace's one
    /// worktree tab (its active tab when that is one).
    pub(super) fn worktree_remove_target(
        &self,
        params: &WorktreeRemoveTabParams,
    ) -> Result<(usize, usize), ApiFailure> {
        match (&params.tab_id, &params.workspace_id) {
            (Some(tab_id), None) => self
                .parse_tab_id(tab_id)
                .ok_or_else(|| ApiFailure::new("tab_not_found", format!("tab {tab_id} not found"))),
            (None, Some(workspace_id)) => {
                let ws_idx = self.parse_workspace_id(workspace_id).ok_or_else(|| {
                    ApiFailure::new(
                        "workspace_not_found",
                        format!("workspace {workspace_id} not found"),
                    )
                })?;
                let ws = &self.state.workspaces[ws_idx];
                let is_linked = |tab_idx: &usize| {
                    ws.tabs[*tab_idx]
                        .worktree
                        .as_ref()
                        .is_some_and(|space| space.is_linked_worktree)
                };
                if is_linked(&ws.active_tab) {
                    return Ok((ws_idx, ws.active_tab));
                }
                let linked = (0..ws.tabs.len()).filter(is_linked).collect::<Vec<_>>();
                match linked.as_slice() {
                    [tab_idx] => Ok((ws_idx, *tab_idx)),
                    [] => Err(ApiFailure::new(
                        "not_linked_worktree",
                        "workspace has no worktree tab",
                    )),
                    _ => Err(ApiFailure::new(
                        "ambiguous_worktree_tab",
                        "workspace has several worktree tabs; name one with tab_id",
                    )),
                }
            }
            _ => Err(ApiFailure::new(
                "invalid_request",
                "exactly one of tab_id or workspace_id is required",
            )),
        }
    }

    fn start_api_worktree_remove(
        &mut self,
        id: String,
        params: WorktreeRemoveTabParams,
        respond_to: std::sync::mpsc::Sender<String>,
    ) {
        let (ws_idx, tab_idx) = match self.worktree_remove_target(&params) {
            Ok(target) => target,
            Err(err) => {
                Self::send_api_response(respond_to, encode_error(id, err.code, err.message));
                return;
            }
        };
        let Some(space) = self.state.workspaces[ws_idx].tabs[tab_idx].worktree.clone() else {
            Self::send_api_response(
                respond_to,
                encode_error(
                    id,
                    "not_linked_worktree",
                    "tab is not a Herdr-managed worktree checkout",
                ),
            );
            return;
        };
        if !space.is_linked_worktree {
            Self::send_api_response(
                respond_to,
                encode_error(
                    id,
                    "not_linked_worktree",
                    "tab is not a linked worktree checkout",
                ),
            );
            return;
        }

        let checkout_key = crate::worktree::canonical_or_original(&space.checkout_path);
        let tabs = self.tabs_for_checkout(&space.checkout_path);
        let pane_ids = tabs
            .iter()
            .flat_map(|&(ws_idx, tab_idx)| {
                self.state.workspaces[ws_idx].tabs[tab_idx]
                    .layout
                    .pane_ids()
            })
            .collect::<Vec<_>>();
        if self
            .pending_api_worktree_remove_paths
            .contains_key(&checkout_key)
            || self
                .pending_api_worktree_creates
                .contains_key(&checkout_key)
            || pane_ids.iter().any(|pane_id| {
                self.pending_worktree_remove_runtime_restores
                    .contains_key(pane_id)
            })
        {
            Self::send_api_response(
                respond_to,
                encode_error(
                    id,
                    "worktree_operation_in_progress",
                    "worktree operation is already in progress for this checkout",
                ),
            );
            return;
        }

        // After the in-progress check: a duplicate must not run `git status` beside the removal.
        #[cfg(windows)]
        {
            if !params.force
                && crate::worktree::checkout_has_dirty_files(
                    &space.checkout_path,
                    params.trust_repository,
                )
                .unwrap_or(false)
            {
                Self::send_api_response(
                    respond_to,
                    encode_error(
                        id,
                        "dirty_worktree_requires_force",
                        crate::worktree::worktree_dirty_remove_message(&space.checkout_path),
                    ),
                );
                return;
            }
        }

        let shutdown_panes =
            if Self::should_shutdown_workspace_terminal_runtimes_for_worktree_remove(params.force) {
                self.shutdown_terminal_runtimes_for_worktree_remove(&pane_ids)
            } else {
                Vec::new()
            };

        let operation_id = self.next_api_worktree_operation_id();
        self.pending_api_worktree_remove_paths
            .insert(checkout_key.clone(), operation_id);
        let workspace_snapshot = self.workspace_info(ws_idx);
        let worktree = self.worktree_info_for_membership(&space, None);
        let command = crate::worktree::build_worktree_remove_command(
            &space.repo_root,
            &space.checkout_path,
            params.force,
            params.trust_repository,
        );
        let api_request = ApiWorktreeRemoveRequest {
            id,
            operation_id,
            checkout_key,
            shutdown_panes,
            respond_to,
        };
        let repo_root = space.repo_root.clone();
        let trust_repository = params.trust_repository;
        let event_tx = self.event_tx.clone();
        let finished = crate::events::WorktreeRemoveResult {
            workspace_id: self.state.workspaces[ws_idx].id.clone(),
            path: space.checkout_path.clone(),
            workspace: Some(Box::new(workspace_snapshot)),
            worktree: Some(Box::new(worktree)),
            forced: params.force,
            api_request: Some(api_request),
            result: Ok(()),
        };
        let spawned = crate::thread_spawn::spawn_named_with(
            "herdr-worktree-remove",
            finished,
            move |mut finished| {
                // A saved tab can outlive its checkout; never remove a directory
                // that has since become some other repo's.
                finished.result =
                    if crate::persist::restored_worktree_space_membership(Some(space)).is_none() {
                        Err("worktree checkout no longer belongs to this repository".into())
                    } else {
                        crate::worktree::run_worktree_remove_command_with_recovery(
                            &command,
                            &repo_root,
                            &finished.path,
                            finished.forced,
                            trust_repository,
                        )
                    };
                let _ =
                    event_tx.blocking_send(AppEvent::WorktreeRemoveFinished(Box::new(finished)));
            },
        );
        if let Err((mut finished, err)) = spawned {
            finished.result = Err(format!("could not start worktree removal: {err}"));
            self.queue_worktree_spawn_failure(AppEvent::WorktreeRemoveFinished(Box::new(finished)));
        }
    }

    pub(crate) fn handle_api_worktree_add_finished(
        &mut self,
        mut result: crate::events::WorktreeAddResult,
    ) {
        let Some(api) = result.api_request.take() else {
            return;
        };
        let checkout_key = api.checkout_key.clone();
        let operation_matches = self
            .pending_api_worktree_creates
            .get(&checkout_key)
            .is_some_and(|operation_id| *operation_id == api.operation_id);
        if !operation_matches {
            Self::send_api_response(
                api.respond_to,
                encode_error(
                    api.id,
                    "stale_worktree_operation",
                    "worktree create completed after the operation was superseded",
                ),
            );
            return;
        }
        self.pending_api_worktree_creates.remove(&checkout_key);

        if let Err(err) = result.result {
            Self::send_api_response(
                api.respond_to,
                encode_error(api.id, "worktree_create_failed", err),
            );
            return;
        }
        // The checkout stays on disk but is not opened: an unprepared tree is
        // what the hook exists to prevent.
        if let Err(err) = result.post_create {
            Self::send_api_response(
                api.respond_to,
                encode_error(api.id, "worktree_post_create_failed", err),
            );
            return;
        }

        let source = WorktreeSource {
            workspace_idx: self.worktree_target_workspace_by_id(api.target_workspace_id.as_deref()),
            source_checkout_path: api.source_checkout_path,
            source_repo_root: api.source_repo_root,
            repo_key: api.repo_key,
            repo_name: api.repo_name,
        };
        let opened = match self.open_worktree_tab(
            &source,
            result.path.clone(),
            true,
            api.label,
            api.focus,
        ) {
            Ok(opened) => opened,
            Err(err) => {
                Self::send_api_response(
                    api.respond_to,
                    encode_error(
                        api.id,
                        err.code,
                        format!(
                            "created worktree but failed to open its tab: {}",
                            err.message
                        ),
                    ),
                );
                return;
            }
        };
        let Some(membership) = self.state.workspaces[opened.ws_idx].tabs[opened.tab_idx]
            .worktree
            .clone()
        else {
            Self::send_api_response(
                api.respond_to,
                encode_error(
                    api.id,
                    "worktree_open_failed",
                    "created worktree but failed to record its tab",
                ),
            );
            return;
        };
        let worktree =
            self.worktree_info_for_membership(&membership, Some((opened.ws_idx, opened.tab_idx)));
        self.emit_worktree_created_event(opened.ws_idx, worktree.clone());
        let response = encode_success(
            api.id,
            ResponseResult::WorktreeCreated {
                workspace: self.workspace_info(opened.ws_idx),
                tab: self
                    .tab_info(opened.ws_idx, opened.tab_idx)
                    .expect("created worktree tab should exist"),
                root_pane: self
                    .root_pane_info(opened.ws_idx, opened.tab_idx)
                    .expect("created worktree tab should have a root pane"),
                worktree,
            },
        );
        Self::send_api_response(api.respond_to, response);
    }

    pub(crate) fn handle_api_worktree_remove_finished(
        &mut self,
        mut result: crate::events::WorktreeRemoveResult,
    ) -> Vec<crate::app::actions::PaneStateUpdate> {
        let Some(api) = result.api_request.take() else {
            return Vec::new();
        };
        let operation_matches = self
            .pending_api_worktree_remove_paths
            .get(&api.checkout_key)
            .is_some_and(|operation_id| *operation_id == api.operation_id);
        if !operation_matches {
            Self::send_api_response(
                api.respond_to,
                encode_error(
                    api.id,
                    "stale_worktree_operation",
                    "worktree remove completed after the operation was superseded",
                ),
            );
            return Vec::new();
        }
        self.pending_api_worktree_remove_paths
            .remove(&api.checkout_key);

        if let Err(message) = result.result {
            let pane_updates = self.restore_shutdown_worktree_panes(
                &api.shutdown_panes,
                api.operation_id,
                &result.path,
            );
            let code =
                if !result.forced && crate::worktree::is_dirty_worktree_remove_error(&message) {
                    "dirty_worktree_requires_force"
                } else {
                    "worktree_remove_failed"
                };
            Self::send_api_response(api.respond_to, encode_error(api.id, code, message));
            return pane_updates;
        }

        let workspace_snapshot = result.workspace.as_deref().cloned();
        let workspace_id = self
            .state
            .workspaces
            .iter()
            .position(|ws| ws.id == result.workspace_id)
            .map(|ws_idx| self.public_workspace_id(ws_idx))
            .or_else(|| {
                workspace_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.workspace_id.clone())
            })
            .unwrap_or_else(|| result.workspace_id.clone());
        self.close_removed_worktree_tabs(&result.path);
        let pane_updates = self.restore_shutdown_worktree_panes(
            &api.shutdown_panes,
            api.operation_id,
            &result.path,
        );

        let Some(worktree) = result.worktree.as_deref().cloned() else {
            Self::send_api_response(
                api.respond_to,
                encode_error(
                    api.id,
                    "worktree_remove_failed",
                    "removed worktree but lost worktree snapshot",
                ),
            );
            return pane_updates;
        };
        self.emit_worktree_removed_event(
            workspace_id.clone(),
            workspace_snapshot,
            worktree,
            result.forced,
        );
        let response = encode_success(
            api.id,
            ResponseResult::WorktreeRemoved {
                workspace_id,
                path: result.path.display().to_string(),
                forced: result.forced,
            },
        );
        Self::send_api_response(api.respond_to, response);
        pane_updates
    }

    /// Closes every tab opened for a removed checkout, the way `tab.close` does.
    pub(super) fn close_removed_worktree_tabs(&mut self, checkout_path: &Path) {
        // Close from the back so earlier indices stay valid.
        for (ws_idx, tab_idx) in self.tabs_for_checkout(checkout_path).into_iter().rev() {
            if let Some(tab_id) = self.public_tab_id(ws_idx, tab_idx) {
                let _ = self.handle_tab_close(String::new(), TabTarget { tab_id });
            }
        }
    }

    fn restore_shutdown_worktree_panes(
        &mut self,
        shutdown_panes: &[crate::layout::PaneId],
        operation_id: u64,
        removed_checkout: &std::path::Path,
    ) -> Vec<crate::app::actions::PaneStateUpdate> {
        let mut pane_updates = Vec::new();
        let removed_checkout = crate::worktree::canonical_or_original(removed_checkout);
        for &pane_id in shutdown_panes {
            let Some((ws_idx, terminal_id)) = self
                .find_pane(pane_id)
                .map(|(ws_idx, pane)| (ws_idx, pane.attached_terminal_id.clone()))
            else {
                self.pending_worktree_remove_runtime_exits.remove(&pane_id);
                self.pending_worktree_remove_runtime_restores
                    .remove(&pane_id);
                continue;
            };
            let runtime_missing = self.terminal_runtimes.get(&terminal_id).is_none();
            if runtime_missing {
                let workspace = &self.state.workspaces[ws_idx];
                let current_checkout = workspace
                    .find_tab_index_for_pane(pane_id)
                    .and_then(|tab_idx| workspace.tabs[tab_idx].worktree.as_ref())
                    .map(|space| space.checkout_path.clone())
                    .or_else(|| {
                        workspace
                            .first_pane_cwd_from(&self.state.terminals, &self.terminal_runtimes)
                    });
                if let Some(current_checkout) = current_checkout.filter(|checkout| {
                    crate::worktree::canonical_or_original(checkout) != removed_checkout
                }) {
                    if let Some(terminal) = self.state.terminals.get_mut(&terminal_id) {
                        terminal.cwd = current_checkout;
                    }
                }
                if self
                    .pending_worktree_remove_runtime_exits
                    .contains_key(&pane_id)
                {
                    if self
                        .pending_worktree_remove_runtime_restores
                        .insert(pane_id, operation_id)
                        .is_none()
                    {
                        let event_tx = self.event_tx.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            let _ = event_tx
                                .send(AppEvent::WorktreeRuntimeRestoreFailed {
                                    pane_id,
                                    operation_id,
                                })
                                .await;
                        });
                    }
                } else {
                    pane_updates.extend(self.publish_worktree_runtime_agent_release(pane_id));
                    if !self.respawn_shell_for_launch_pane(pane_id, false) {
                        self.pending_worktree_remove_runtime_restores
                            .insert(pane_id, operation_id);
                        self.queue_worktree_runtime_restore_failed(pane_id, operation_id);
                    }
                }
            }
        }
        pane_updates
    }
}

/// The checks that need no repo, answered before any discovery runs.
fn validate_worktree_create(params: &WorktreeCreateParams) -> Result<(), ApiFailure> {
    if params
        .branch
        .as_deref()
        .is_some_and(|branch| branch.trim().is_empty())
    {
        return Err(ApiFailure::new("invalid_request", "branch is required"));
    }
    if let Some(path) = params.path.as_deref() {
        absolute_user_path(path)?;
    }
    Ok(())
}
