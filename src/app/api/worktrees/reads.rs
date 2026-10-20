use std::path::PathBuf;

use crate::api::schema::{Method, Request};
use crate::app::App;
use crate::events::{AppEvent, WorktreeReadData, WorktreeReadResult};

use super::super::responses::encode_error;
use super::{
    absolute_user_path, find_worktree_entry, not_git_worktree, ApiFailure, WorktreeRepo,
    WorktreeSource,
};

// Capture only what is known without I/O. Filesystem discovery and Git run on
// the worker; the target workspace is found again by its id on completion.
struct SourceInput {
    workspace_id: Option<String>,
    repo: Option<WorktreeRepo>,
    cwd: Option<PathBuf>,
    from_workspace: bool,
}

impl SourceInput {
    fn resolve(self, trust_repository: bool) -> Result<WorktreeSource, ApiFailure> {
        let repo = self
            .repo
            .or_else(|| {
                self.cwd
                    .as_deref()
                    .and_then(crate::workspace::git_space_metadata)
                    .map(WorktreeRepo::Space)
            })
            .ok_or_else(|| not_git_worktree(self.from_workspace))?;
        Ok(repo.into_source(trust_repository))
    }
}

impl App {
    fn capture_worktree_read_source(
        &self,
        workspace_id: &Option<String>,
        cwd: &Option<String>,
    ) -> Result<SourceInput, ApiFailure> {
        let target = self.worktree_target_workspace(workspace_id.as_deref())?;
        let target_id = target.map(|idx| self.state.workspaces[idx].id.clone());
        if let Some(cwd) = cwd {
            return Ok(SourceInput {
                workspace_id: target_id,
                repo: None,
                cwd: Some(absolute_user_path(cwd)?),
                from_workspace: false,
            });
        }
        let Some(ws_idx) = target else {
            return Err(ApiFailure::new(
                "invalid_request",
                "workspace_id or cwd is required when no workspace is active",
            ));
        };
        let ws = &self.state.workspaces[ws_idx];
        let membership = ws.active_tab().and_then(|tab| tab.worktree.clone());
        let repo = membership.map(WorktreeRepo::Membership).or_else(|| {
            ws.focused_pane_git_space(&self.state.terminals)
                .cloned()
                .map(WorktreeRepo::Space)
        });
        let cwd = if repo.is_none() {
            ws.focused_pane_cwd_from(&self.state.terminals, &self.terminal_runtimes)
        } else {
            None
        };
        Ok(SourceInput {
            workspace_id: target_id,
            repo,
            cwd,
            from_workspace: true,
        })
    }

    pub(super) fn start_api_worktree_read(
        &mut self,
        request: Request,
        respond_to: std::sync::mpsc::Sender<String>,
        client_local: bool,
    ) {
        let (workspace_id, cwd, trust_repository) = match &request.method {
            Method::WorktreeList(params) => {
                (&params.workspace_id, &params.cwd, params.trust_repository)
            }
            Method::WorktreeOpen(params) => {
                if params.path.is_some() == params.branch.is_some() {
                    let _ = respond_to.send(encode_error(
                        request.id,
                        "invalid_request",
                        "exactly one of path or branch is required",
                    ));
                    return;
                }
                (&params.workspace_id, &params.cwd, params.trust_repository)
            }
            // Create finds its repo here too, then starts `git worktree add`.
            Method::WorktreeCreate(params) => {
                (&params.workspace_id, &params.cwd, params.trust_repository)
            }
            _ => unreachable!("only worktree list/open/create use background discovery"),
        };
        let input = match self.capture_worktree_read_source(workspace_id, cwd) {
            Ok(input) => input,
            Err(err) => {
                let _ = respond_to.send(encode_error(request.id, err.code, err.message));
                return;
            }
        };
        let Ok(permit) = self.worktree_read_slots.clone().try_acquire_owned() else {
            let _ = respond_to.send(encode_error(
                request.id,
                "worktree_busy",
                "too many worktree checks are pending; retry shortly",
            ));
            return;
        };
        let event_tx = self.event_tx.clone();
        let spawn_error_response = respond_to.clone();
        let request_id = request.id.clone();
        let spawned = std::thread::Builder::new()
            .name("worktree-read".into())
            .spawn(move || {
                let source_workspace_id = input.workspace_id.clone();
                let result = input
                    .resolve(trust_repository)
                    .and_then(|source| {
                        let mut entries = if matches!(request.method, Method::WorktreeCreate(_)) {
                            Vec::new()
                        } else {
                            crate::worktree::list_existing_worktrees(
                                &source.source_repo_root,
                                trust_repository,
                            )
                            .map_err(|err| ApiFailure::new("worktree_list_failed", err))?
                        };
                        if let Method::WorktreeOpen(params) = &request.method {
                            entries = vec![find_worktree_entry(
                                entries,
                                params.path.clone(),
                                params.branch.clone(),
                            )?];
                        }
                        Ok(WorktreeReadData {
                            source_checkout_path: source.source_checkout_path,
                            source_repo_root: source.source_repo_root,
                            repo_key: source.repo_key,
                            repo_name: source.repo_name,
                            entries,
                        })
                    })
                    .map_err(|err| (err.code.to_string(), err.message));
                let _ = event_tx.blocking_send(AppEvent::WorktreeReadFinished(Box::new(
                    WorktreeReadResult {
                        _permit: permit,
                        request,
                        client_local,
                        source_workspace_id,
                        result,
                        respond_to,
                    },
                )));
            });
        if let Err(err) = spawned {
            let _ = spawn_error_response.send(encode_error(
                request_id,
                "worktree_list_failed",
                format!("could not start worktree discovery: {err}"),
            ));
        }
    }

    pub(crate) fn handle_api_worktree_read_finished(&mut self, result: WorktreeReadResult) {
        if let (Method::WorktreeCreate(params), Ok(data)) = (&result.request.method, &result.result)
        {
            let source = WorktreeSource {
                workspace_idx: self
                    .worktree_target_workspace_by_id(result.source_workspace_id.as_deref()),
                source_checkout_path: data.source_checkout_path.clone(),
                source_repo_root: data.source_repo_root.clone(),
                repo_key: data.repo_key.clone(),
                repo_name: data.repo_name.clone(),
            };
            self.start_api_worktree_add(
                result.request.id.clone(),
                params.clone(),
                source,
                result.respond_to.clone(),
            );
            return;
        }
        let response = match result.result {
            Err((code, message)) => encode_error(result.request.id, &code, message),
            Ok(data) => {
                let source = WorktreeSource {
                    // An index captured before Git ran could now name a different workspace.
                    workspace_idx: self
                        .worktree_target_workspace_by_id(result.source_workspace_id.as_deref()),
                    source_checkout_path: data.source_checkout_path,
                    source_repo_root: data.source_repo_root,
                    repo_key: data.repo_key,
                    repo_name: data.repo_name,
                };
                match result.request.method {
                    Method::WorktreeList(_) => {
                        self.finish_worktree_list(result.request.id, source, data.entries)
                    }
                    Method::WorktreeOpen(params) => self.finish_worktree_open(
                        result.request.id,
                        params,
                        source,
                        data.entries
                            .into_iter()
                            .next()
                            .expect("open discovery selects one worktree"),
                    ),
                    _ => unreachable!("create completes above"),
                }
            }
        };
        let _ = result.respond_to.send(response);
    }
}
