use std::path::{Path, PathBuf};

use crate::api::schema::{
    EventData, EventEnvelope, EventKind, ResponseResult, WorktreeInfo, WorktreeOpenParams,
    WorktreeSourceInfo,
};
use crate::app::App;

use super::responses::{encode_error, encode_success};

mod deferred;
mod reads;

struct ApiFailure {
    code: &'static str,
    message: String,
}

impl ApiFailure {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

fn absolute_user_path(path: &str) -> Result<PathBuf, ApiFailure> {
    let path = crate::worktree::expand_tilde_path(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(ApiFailure::new(
            "invalid_request",
            "worktree path must be absolute",
        ))
    }
}

/// The repo a worktree action works on, and where its tab opens.
struct WorktreeSource {
    /// The workspace the worktree's tab opens in; `None` opens a new workspace.
    workspace_idx: Option<usize>,
    /// The repo's main checkout, even when the action started in a linked one.
    source_checkout_path: PathBuf,
    source_repo_root: PathBuf,
    repo_key: String,
    repo_name: String,
}

/// A worktree tab that was opened, or found already open.
struct OpenedWorktreeTab {
    ws_idx: usize,
    tab_idx: usize,
    already_open: bool,
}

impl App {
    fn finish_worktree_list(
        &self,
        id: String,
        source: WorktreeSource,
        entries: Vec<crate::worktree::ExistingWorktree>,
    ) -> String {
        let worktrees = entries
            .into_iter()
            .map(|entry| self.worktree_info_for_entry(&source, entry))
            .collect();

        encode_success(
            id,
            ResponseResult::WorktreeList {
                source: self.worktree_source_info(&source),
                worktrees,
            },
        )
    }

    fn finish_worktree_open(
        &mut self,
        id: String,
        params: WorktreeOpenParams,
        source: WorktreeSource,
        entry: crate::worktree::ExistingWorktree,
    ) -> String {
        if entry.is_bare || entry.is_prunable {
            return encode_error(id, "worktree_not_found", "worktree cannot be opened");
        }
        let canonical_path = crate::worktree::canonical_or_original(&entry.path);
        let target_matches =
            crate::workspace::git_space_metadata(&entry.path).is_some_and(|space| {
                space.key == source.repo_key
                    && space.checkout_key == canonical_path.display().to_string()
            });
        if !target_matches {
            return encode_error(
                id,
                "worktree_not_found",
                "worktree checkout changed during discovery; retry the request",
            );
        }
        let is_linked_worktree =
            canonical_path != crate::worktree::canonical_or_original(&source.source_repo_root);
        let opened = match self.open_worktree_tab(
            &source,
            entry.path.clone(),
            is_linked_worktree,
            params.label,
            params.focus,
        ) {
            Ok(opened) => opened,
            Err(err) => return encode_error(id, err.code, err.message),
        };

        let worktree = self.worktree_info_for_entry(&source, entry);
        self.emit_worktree_opened_event(opened.ws_idx, worktree.clone(), opened.already_open);
        encode_success(
            id,
            ResponseResult::WorktreeOpened {
                workspace: self.workspace_info(opened.ws_idx),
                tab: self
                    .tab_info(opened.ws_idx, opened.tab_idx)
                    .expect("opened worktree tab should exist"),
                root_pane: self
                    .root_pane_info(opened.ws_idx, opened.tab_idx)
                    .expect("opened worktree tab should have a root pane"),
                worktree,
                already_open: opened.already_open,
            },
        )
    }

    /// Focuses the tab already open on `checkout_path`, or opens one in the
    /// source's workspace (a new workspace when there is none) and marks it.
    fn open_worktree_tab(
        &mut self,
        source: &WorktreeSource,
        checkout_path: PathBuf,
        is_linked_worktree: bool,
        label: Option<String>,
        focus: bool,
    ) -> Result<OpenedWorktreeTab, ApiFailure> {
        if let Some((ws_idx, tab_idx)) = self.open_tab_for_checkout(&checkout_path) {
            if let Some(label) = label {
                self.rename_worktree_tab(ws_idx, tab_idx, label);
            }
            if focus {
                self.state.switch_workspace_tab(ws_idx, tab_idx);
            }
            return Ok(OpenedWorktreeTab {
                ws_idx,
                tab_idx,
                already_open: true,
            });
        }

        let membership = worktree_membership(source, checkout_path.clone(), is_linked_worktree);
        let (ws_idx, tab_idx) = if let Some(ws_idx) = source.workspace_idx {
            let tab_idx = self
                .spawn_tab(ws_idx, checkout_path, label, Vec::new(), focus)
                .map_err(|err| ApiFailure::new("worktree_open_failed", err.to_string()))?;
            self.state.workspaces[ws_idx].tabs[tab_idx].worktree = Some(membership);
            self.emit_tab_created_events(ws_idx, tab_idx);
            (ws_idx, tab_idx)
        } else {
            let ws_idx = self
                .create_workspace_with_options(checkout_path, focus)
                .map_err(|err| ApiFailure::new("worktree_open_failed", err.to_string()))?;
            let tab = &mut self.state.workspaces[ws_idx].tabs[0];
            tab.worktree = Some(membership);
            if let Some(label) = label {
                tab.set_custom_name(label);
            }
            self.emit_workspace_open_events(ws_idx);
            (ws_idx, 0)
        };
        self.state.mark_session_dirty();
        Ok(OpenedWorktreeTab {
            ws_idx,
            tab_idx,
            already_open: false,
        })
    }

    fn rename_worktree_tab(&mut self, ws_idx: usize, tab_idx: usize, label: String) {
        let Some(tab) = self
            .state
            .workspaces
            .get_mut(ws_idx)
            .and_then(|ws| ws.tabs.get_mut(tab_idx))
        else {
            return;
        };
        tab.set_custom_name(label);
        self.state.mark_session_dirty();
        let Some(tab) = self.tab_info(ws_idx, tab_idx) else {
            return;
        };
        crate::logging::tab_renamed(&tab.workspace_id, &tab.tab_id);
        self.emit_event(EventEnvelope {
            event: EventKind::TabRenamed,
            data: EventData::TabRenamed {
                tab_id: tab.tab_id,
                workspace_id: tab.workspace_id,
                label: tab.label,
            },
        });
    }

    /// The workspace a worktree's tab opens in: the one named, else the active one.
    fn worktree_target_workspace(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Option<usize>, ApiFailure> {
        if let Some(workspace_id) = workspace_id {
            return self
                .parse_workspace_id(workspace_id)
                .map(Some)
                .ok_or_else(|| {
                    ApiFailure::new(
                        "workspace_not_found",
                        format!("workspace {workspace_id} not found"),
                    )
                });
        }
        Ok(self.state.active.or_else(|| {
            self.state
                .workspaces
                .get(self.state.selected)
                .map(|_| self.state.selected)
        }))
    }

    /// Re-finds a target workspace by its stable id after background work;
    /// falls back to the active one when it has closed since.
    fn worktree_target_workspace_by_id(&self, workspace_id: Option<&str>) -> Option<usize> {
        workspace_id
            .and_then(|id| self.state.workspaces.iter().position(|ws| ws.id == id))
            .or_else(|| self.worktree_target_workspace(None).ok().flatten())
    }

    fn worktree_source_info(&self, source: &WorktreeSource) -> WorktreeSourceInfo {
        WorktreeSourceInfo {
            repo_key: source.repo_key.clone(),
            repo_name: source.repo_name.clone(),
            repo_root: source.source_repo_root.display().to_string(),
            source_checkout_path: source.source_checkout_path.display().to_string(),
            source_workspace_id: source
                .workspace_idx
                .map(|idx| self.public_workspace_id(idx)),
        }
    }

    fn worktree_info_for_entry(
        &self,
        source: &WorktreeSource,
        entry: crate::worktree::ExistingWorktree,
    ) -> WorktreeInfo {
        let canonical_path = crate::worktree::canonical_or_original(&entry.path);
        let repo_root = crate::worktree::canonical_or_original(&source.source_repo_root);
        let open_tab = self.open_tab_for_checkout(&canonical_path);
        WorktreeInfo {
            path: entry.path.display().to_string(),
            branch: entry.branch,
            is_bare: entry.is_bare,
            is_detached: entry.is_detached,
            is_prunable: entry.is_prunable,
            is_linked_worktree: canonical_path != repo_root,
            open_workspace_id: open_tab.map(|(ws_idx, _)| self.public_workspace_id(ws_idx)),
            open_tab_id: open_tab.and_then(|(ws_idx, tab_idx)| self.public_tab_id(ws_idx, tab_idx)),
            label: source.repo_name.clone(),
        }
    }

    pub(crate) fn worktree_info_for_membership(
        &self,
        membership: &crate::workspace::WorktreeSpaceMembership,
        open_tab: Option<(usize, usize)>,
    ) -> WorktreeInfo {
        let branch = crate::workspace::git_branch(&membership.checkout_path);
        let is_detached = branch.is_none();
        WorktreeInfo {
            path: membership.checkout_path.display().to_string(),
            branch,
            is_bare: false,
            is_detached,
            is_prunable: false,
            is_linked_worktree: membership.is_linked_worktree,
            open_workspace_id: open_tab.map(|(ws_idx, _)| self.public_workspace_id(ws_idx)),
            open_tab_id: open_tab.and_then(|(ws_idx, tab_idx)| self.public_tab_id(ws_idx, tab_idx)),
            label: membership.label.clone(),
        }
    }

    /// Every tab opened for this checkout, as (workspace, tab) indices.
    pub(crate) fn tabs_for_checkout(&self, checkout_path: &Path) -> Vec<(usize, usize)> {
        let canonical_checkout = &crate::worktree::canonical_or_original(checkout_path);
        self.state
            .workspaces
            .iter()
            .enumerate()
            .flat_map(|(ws_idx, ws)| {
                ws.tabs
                    .iter()
                    .enumerate()
                    .filter_map(move |(tab_idx, tab)| {
                        // Only explicit provenance counts: a shell may cd anywhere.
                        tab.worktree
                            .as_ref()
                            .is_some_and(|space| {
                                crate::worktree::canonical_or_original(&space.checkout_path)
                                    == *canonical_checkout
                            })
                            .then_some((ws_idx, tab_idx))
                    })
            })
            .collect()
    }

    pub(crate) fn open_tab_for_checkout(&self, checkout_path: &Path) -> Option<(usize, usize)> {
        self.tabs_for_checkout(checkout_path).into_iter().next()
    }

    fn emit_worktree_created_event(&mut self, ws_idx: usize, worktree: WorktreeInfo) {
        self.emit_event(EventEnvelope {
            event: EventKind::WorktreeCreated,
            data: EventData::WorktreeCreated {
                workspace: self.workspace_info(ws_idx),
                worktree,
            },
        });
    }

    fn emit_worktree_opened_event(
        &mut self,
        ws_idx: usize,
        worktree: WorktreeInfo,
        already_open: bool,
    ) {
        self.emit_event(EventEnvelope {
            event: EventKind::WorktreeOpened,
            data: EventData::WorktreeOpened {
                workspace: self.workspace_info(ws_idx),
                worktree,
                already_open,
            },
        });
    }

    fn emit_worktree_removed_event(
        &mut self,
        workspace_id: String,
        workspace: Option<crate::api::schema::WorkspaceInfo>,
        worktree: WorktreeInfo,
        forced: bool,
    ) {
        self.emit_event(EventEnvelope {
            event: EventKind::WorktreeRemoved,
            data: EventData::WorktreeRemoved {
                workspace_id,
                workspace,
                worktree,
                forced,
            },
        });
    }
}

/// Where a worktree action found its repo, before any Git ran.
#[derive(Clone)]
enum WorktreeRepo {
    /// A tab opened for a checkout records its repo.
    Membership(crate::workspace::WorktreeSpaceMembership),
    Space(crate::workspace::GitSpaceMetadata),
}

impl WorktreeRepo {
    /// The source rooted at the repo's main checkout. A linked checkout's main
    /// one comes from `git worktree list`, so this may run Git.
    fn into_source(self, trust_repository: bool) -> WorktreeSource {
        let (source_checkout_path, repo_key, repo_name) = match self {
            Self::Membership(membership) => {
                (membership.repo_root, membership.key, membership.label)
            }
            Self::Space(space) => (
                main_checkout_path_for_space(&space, trust_repository),
                space.key,
                space.repo_name,
            ),
        };
        WorktreeSource {
            workspace_idx: None,
            source_repo_root: source_checkout_path.clone(),
            source_checkout_path,
            repo_key,
            repo_name,
        }
    }
}

fn not_git_worktree(from_workspace: bool) -> ApiFailure {
    ApiFailure::new(
        "not_git_worktree",
        if from_workspace {
            "Herdr worktree actions require a workspace inside a Git work tree"
        } else {
            "Herdr worktree actions require a path inside a Git work tree"
        },
    )
}

fn main_checkout_path_for_space(
    space: &crate::workspace::GitSpaceMetadata,
    trust_repository: bool,
) -> PathBuf {
    if !space.is_linked_worktree {
        return space.repo_root.clone();
    }

    crate::worktree::list_existing_worktrees(&space.repo_root, trust_repository)
        .ok()
        .and_then(|entries| {
            entries.into_iter().find_map(|entry| {
                let entry_space = crate::workspace::git_space_metadata(&entry.path)?;
                if entry_space.key == space.key && !entry_space.is_linked_worktree {
                    Some(entry_space.repo_root)
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| space.repo_root.clone())
}

fn find_worktree_entry(
    entries: Vec<crate::worktree::ExistingWorktree>,
    path: Option<String>,
    branch: Option<String>,
) -> Result<crate::worktree::ExistingWorktree, ApiFailure> {
    if let Some(path) = path {
        let expected = absolute_user_path(&path)?;
        let expected = crate::worktree::canonical_or_original(&expected);
        entries
            .into_iter()
            .find(|entry| crate::worktree::canonical_or_original(&entry.path) == expected)
            .ok_or_else(|| ApiFailure::new("worktree_not_found", "worktree path not found"))
    } else if let Some(branch) = branch {
        let matches = entries
            .into_iter()
            .filter(|entry| {
                !entry.is_bare
                    && !entry.is_prunable
                    && !entry.is_detached
                    && entry.branch.as_deref() == Some(branch.as_str())
            })
            .collect::<Vec<_>>();
        match matches.len() {
            0 => Err(ApiFailure::new(
                "worktree_not_found",
                "worktree branch not found",
            )),
            1 => Ok(matches.into_iter().next().expect("one match should exist")),
            _ => Err(ApiFailure::new(
                "ambiguous_worktree_branch",
                "multiple worktrees matched branch",
            )),
        }
    } else {
        Err(ApiFailure::new(
            "invalid_request",
            "exactly one of path or branch is required",
        ))
    }
}

fn worktree_membership(
    source: &WorktreeSource,
    checkout_path: PathBuf,
    is_linked_worktree: bool,
) -> crate::workspace::WorktreeSpaceMembership {
    crate::workspace::WorktreeSpaceMembership {
        key: source.repo_key.clone(),
        label: source.repo_name.clone(),
        repo_root: source.source_repo_root.clone(),
        checkout_path,
        is_linked_worktree,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::api::schema::{
        ErrorResponse, Request, SuccessResponse, WorktreeCreateParams, WorktreeListParams,
        WorktreeRemoveParams, WorktreeRemoveTabParams,
    };
    use crate::events::{
        ApiWorktreeAddRequest, ApiWorktreeRemoveRequest, AppEvent, WorktreeAddResult,
        WorktreeRemoveResult,
    };
    use crate::{config::Config, workspace::Workspace};

    fn unique_temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("herdr-{name}-{}-{nanos}", std::process::id()))
    }

    fn run_git(repo: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "git command failed: git -C {} {}",
            repo.display(),
            args.join(" ")
        );
    }

    fn create_committed_repo(name: &str) -> PathBuf {
        let repo = unique_temp_path(name);
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init", "--quiet"]);
        run_git(&repo, &["config", "user.email", "herdr@example.invalid"]);
        run_git(&repo, &["config", "user.name", "Herdr Test"]);
        std::fs::write(repo.join("README.md"), "test\n").unwrap();
        run_git(&repo, &["add", "README.md"]);
        run_git(&repo, &["commit", "--quiet", "-m", "initial"]);
        repo
    }

    fn test_app() -> App {
        test_app_with_event_hub(crate::api::EventHub::default())
    }

    fn test_app_with_event_hub(event_hub: crate::api::EventHub) -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            api_rx,
            event_hub,
        );
        app.state.default_shell = test_shell().into();
        app
    }

    #[cfg(windows)]
    fn test_shell() -> &'static str {
        "C:\\Windows\\System32\\whoami.exe"
    }

    #[cfg(not(windows))]
    fn test_shell() -> &'static str {
        "/usr/bin/true"
    }

    fn app_with_parent(repo: &Path) -> App {
        let mut app = test_app();
        let mut parent = Workspace::test_new("main");
        parent.test_cwd = repo.to_path_buf();
        app.state.workspaces = vec![parent];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app
    }

    fn wait_for_app_event(app: &mut App) -> AppEvent {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Ok(event) = app.event_rx.try_recv() {
                return event;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for app event"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    fn install_event_plugin(app: &mut App, name: &str, event: &str) -> PathBuf {
        let plugin_root = unique_temp_path(name);
        std::fs::create_dir_all(&plugin_root).unwrap();
        let manifest_path = plugin_root.join("herdr-plugin.toml");
        std::fs::write(&manifest_path, format!("id = 'example.{name}'\n")).unwrap();
        app.state.installed_plugins.insert(
            format!("example.{name}"),
            crate::api::schema::InstalledPluginInfo {
                plugin_id: format!("example.{name}"),
                name: name.into(),
                version: "0.1.0".into(),
                min_herdr_version: "0.7.0".into(),
                description: None,
                manifest_path: manifest_path.display().to_string(),
                plugin_root: plugin_root.display().to_string(),
                enabled: true,
                platforms: None,
                build: Vec::new(),
                startup: Vec::new(),
                actions: Vec::new(),
                events: vec![crate::api::schema::PluginManifestEventHook {
                    on: event.into(),
                    platforms: None,
                    command: vec!["sh".into(), "-c".into(), "true".into()],
                }],
                panes: Vec::new(),
                link_handlers: Vec::new(),
                source: crate::api::schema::PluginSourceInfo::default(),
                warnings: Vec::new(),
            },
        );
        plugin_root
    }

    fn response_channel() -> (
        std::sync::mpsc::Sender<String>,
        std::sync::mpsc::Receiver<String>,
    ) {
        std::sync::mpsc::channel()
    }

    fn run_deferred_api_request(app: &mut App, request: Request) -> String {
        let (respond_to, response_rx) = response_channel();
        assert!(app.handle_deferred_worktree_api_request(request, respond_to, false));
        if let Ok(response) = response_rx.recv_timeout(std::time::Duration::from_millis(50)) {
            return response;
        }

        // Create takes two background steps: discovery, then `git worktree add`.
        loop {
            let event = wait_for_app_event(app);
            app.handle_internal_event(event);
            if let Ok(response) = response_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                return response;
            }
        }
    }

    /// Runs queued background completions until `response_rx` answers.
    fn finish_deferred(app: &mut App, response_rx: &std::sync::mpsc::Receiver<String>) -> String {
        loop {
            let event = wait_for_app_event(app);
            app.handle_internal_event(event);
            if let Ok(response) = response_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                return response;
            }
        }
    }

    #[tokio::test]
    async fn api_worktree_create_opens_tab_and_marks_membership() {
        let repo = create_committed_repo("api-worktree-create-repo");
        let worktree_root = unique_temp_path("api-worktree-create-root");
        let event_hub = crate::api::EventHub::default();
        let mut app = test_app_with_event_hub(event_hub.clone());
        let mut parent = Workspace::test_new("main");
        parent.test_cwd = repo.clone();
        app.state.workspaces = vec![parent];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.worktree_directory = worktree_root.clone();
        let workspace_id = app.state.workspaces[0].id.clone();

        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeCreate(WorktreeCreateParams {
                    workspace_id: Some(workspace_id.clone()),
                    branch: Some("worktree/api-create".into()),
                    ..WorktreeCreateParams::default()
                }),
            },
        );

        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::WorktreeCreated {
            workspace,
            tab,
            root_pane,
            worktree,
        } = success.result
        else {
            panic!("expected worktree_created response");
        };
        assert_eq!(workspace.workspace_id, workspace_id);
        assert_eq!(tab.workspace_id, workspace.workspace_id);
        assert_eq!(root_pane.workspace_id, workspace.workspace_id);
        assert_eq!(worktree.branch.as_deref(), Some("worktree/api-create"));
        assert_eq!(worktree.open_tab_id.as_deref(), Some(tab.tab_id.as_str()));
        assert!(Path::new(&worktree.path).starts_with(&worktree_root));
        assert!(Path::new(&worktree.path).join("README.md").exists());
        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(app.state.workspaces[0].tabs.len(), 2);
        assert!(app.state.workspaces[0].tabs[0].worktree.is_none());
        assert!(
            app.state.workspaces[0].tabs[1]
                .worktree
                .as_ref()
                .unwrap()
                .is_linked_worktree
        );
        assert!(workspace.worktree.is_none());
        assert!(tab.worktree.unwrap().is_linked_worktree);
        assert_eq!(
            root_pane
                .cwd
                .as_deref()
                .map(|cwd| crate::worktree::canonical_or_original(Path::new(cwd))),
            Some(crate::worktree::canonical_or_original(Path::new(
                &worktree.path
            )))
        );
        let events = event_hub.events_after(0);
        assert!(events.iter().any(|(_, event)| {
            matches!(
                &event.data,
                EventData::WorktreeCreated {
                    workspace: event_workspace,
                    worktree: event_worktree,
                } if event_workspace.workspace_id == workspace.workspace_id
                    && event_worktree.branch.as_deref() == Some("worktree/api-create")
                    && event_worktree.is_linked_worktree
            )
        }));
        let kinds = events
            .iter()
            .map(|(_, event)| event.event)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds
                .iter()
                .filter(|event| **event == EventKind::WorktreeCreated)
                .count(),
            1
        );
        assert_eq!(
            &kinds[kinds.len() - 4..],
            &[
                EventKind::TabCreated,
                EventKind::PaneCreated,
                EventKind::LayoutUpdated,
                EventKind::WorktreeCreated,
            ]
        );

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
        let remove = crate::worktree::build_worktree_remove_command(
            &repo,
            Path::new(&worktree.path),
            false,
            false,
        );
        crate::worktree::run_worktree_command(&remove).unwrap();
        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[cfg(unix)]
    fn create_worktree_with_post_create(
        name: &str,
        post_create: Vec<String>,
    ) -> (App, PathBuf, PathBuf, String) {
        let repo = create_committed_repo(&format!("{name}-repo"));
        let worktree_root = unique_temp_path(&format!("{name}-root"));
        let mut app = app_with_parent(&repo);
        app.state.worktree_directory = worktree_root.clone();
        app.state.worktree_post_create = post_create;
        let workspace_id = app.state.workspaces[0].id.clone();
        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeCreate(WorktreeCreateParams {
                    workspace_id: Some(workspace_id),
                    branch: Some("worktree/post-create".into()),
                    ..WorktreeCreateParams::default()
                }),
            },
        );
        (app, repo, worktree_root, response)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn api_worktree_create_runs_post_create_in_checkout_before_opening() {
        let (app, repo, worktree_root, response) = create_worktree_with_post_create(
            "api-worktree-post-create",
            vec![
                "sh".into(),
                "-c".into(),
                r#"printf '%s\n' "$PWD" "$HERDR_WORKTREE_PATH" "$HERDR_WORKTREE_SOURCE" "$HERDR_WORKTREE_BRANCH" > seeded"#.into(),
            ],
        );

        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::WorktreeCreated { worktree, .. } = success.result else {
            panic!("expected worktree_created response");
        };
        let checkout = PathBuf::from(&worktree.path);
        let seeded = std::fs::read_to_string(checkout.join("seeded")).unwrap();
        let lines: Vec<&str> = seeded.lines().collect();
        let canonical = |path: &str| crate::worktree::canonical_or_original(Path::new(path));
        assert_eq!(canonical(lines[0]), canonical(&worktree.path));
        assert_eq!(canonical(lines[1]), canonical(&worktree.path));
        assert_eq!(
            canonical(lines[2]),
            crate::worktree::canonical_or_original(&repo)
        );
        assert_eq!(lines[3], "worktree/post-create");
        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(app.state.workspaces[0].tabs.len(), 2);

        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn api_worktree_create_post_create_failure_keeps_checkout_unopened() {
        let (app, repo, worktree_root, response) = create_worktree_with_post_create(
            "api-worktree-post-create-failure",
            vec![
                "sh".into(),
                "-c".into(),
                "echo seeding broke >&2; exit 3".into(),
            ],
        );

        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "worktree_post_create_failed");
        assert!(error.error.message.contains("seeding broke"));
        let checkout = crate::worktree::default_checkout_path(
            &worktree_root,
            &repo.file_name().unwrap().to_string_lossy(),
            "worktree/post-create",
        );
        assert!(error
            .error
            .message
            .contains(&checkout.display().to_string()));
        assert!(checkout.join("README.md").exists());
        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(app.state.workspaces[0].tabs.len(), 1);
        assert!(app.pending_api_worktree_creates.is_empty());

        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn deferred_api_worktree_create_preserves_event_and_plugin_context() {
        let repo = create_committed_repo("api-worktree-create-deferred-repo");
        let worktree_root = unique_temp_path("api-worktree-create-deferred-root");
        let event_hub = crate::api::EventHub::default();
        let mut app = test_app_with_event_hub(event_hub.clone());
        let mut parent = Workspace::test_new("main");
        parent.test_cwd = repo.clone();
        app.state.workspaces = vec![parent];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.worktree_directory = worktree_root.clone();
        let plugin_root = install_event_plugin(&mut app, "deferred-create", "worktree.created");
        let (respond_to, response_rx) = response_channel();

        assert!(app.handle_deferred_worktree_api_request(
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeCreate(WorktreeCreateParams {
                    workspace_id: Some(app.state.workspaces[0].id.clone()),
                    branch: Some("worktree/api-create-deferred".into()),
                    ..WorktreeCreateParams::default()
                }),
            },
            respond_to,
            false,
        ));
        assert!(response_rx.try_recv().is_err());

        let response = finish_deferred(&mut app, &response_rx);
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::WorktreeCreated { tab, worktree, .. } = success.result else {
            panic!("expected worktree_created response");
        };
        let event_kinds = event_hub
            .events_after(0)
            .into_iter()
            .map(|(_, event)| event.event)
            .collect::<Vec<_>>();
        assert_eq!(
            &event_kinds[event_kinds.len() - 4..],
            &[
                EventKind::TabCreated,
                EventKind::PaneCreated,
                EventKind::LayoutUpdated,
                EventKind::WorktreeCreated,
            ]
        );
        assert_eq!(
            tab.worktree
                .as_ref()
                .map(|worktree| worktree.checkout_path.as_str()),
            Some(worktree.path.as_str())
        );
        assert!(app.state.plugin_command_logs.iter().any(|log| {
            log.event.as_deref() == Some("worktree.created")
                && log.status == crate::api::schema::PluginCommandStatus::Running
        }));

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
        let _ = std::fs::remove_dir_all(plugin_root);
    }

    #[tokio::test]
    async fn deferred_api_worktree_create_checks_out_existing_branch() {
        let repo = create_committed_repo("api-worktree-create-existing-branch-repo");
        let worktree_root = unique_temp_path("api-worktree-create-existing-branch-root");
        let branch = "foo";
        run_git(&repo, &["branch", branch]);
        let mut app = test_app();
        let mut parent = Workspace::test_new("main");
        parent.test_cwd = repo.clone();
        let parent_id = parent.id.clone();
        app.state.workspaces = vec![parent];
        app.state.ensure_test_terminals();
        app.state.worktree_directory = worktree_root.clone();
        let request = || Request {
            id: "req".into(),
            method: crate::api::schema::Method::WorktreeCreate(WorktreeCreateParams {
                workspace_id: Some(parent_id.clone()),
                branch: Some(branch.into()),
                ..WorktreeCreateParams::default()
            }),
        };

        let (response_tx, response_rx) = response_channel();
        assert!(app.handle_deferred_worktree_api_request(request(), response_tx, false));
        let response = finish_deferred(&mut app, &response_rx);
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::WorktreeCreated { worktree, .. } = success.result else {
            panic!("expected worktree_created response");
        };
        assert_eq!(worktree.branch.as_deref(), Some(branch));
        let checkout = Path::new(&worktree.path);
        assert!(checkout.join("README.md").exists());
        let branch_name = std::process::Command::new("git")
            .arg("-C")
            .arg(checkout)
            .args(["branch", "--show-current"])
            .output()
            .unwrap();
        assert!(branch_name.status.success());
        assert_eq!(
            String::from_utf8(branch_name.stdout).unwrap().trim(),
            branch
        );
        assert!(app.pending_api_worktree_creates.is_empty());

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn deferred_api_worktree_create_failure_clears_pending_checkout() {
        let repo = create_committed_repo("api-worktree-create-failure-repo");
        let worktree_root = unique_temp_path("api-worktree-create-failure-root");
        let branch_name = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["branch", "--show-current"])
            .output()
            .unwrap();
        assert!(branch_name.status.success());
        let branch = String::from_utf8(branch_name.stdout)
            .unwrap()
            .trim()
            .to_string();
        let mut app = test_app();
        let mut parent = Workspace::test_new("main");
        parent.test_cwd = repo.clone();
        let parent_id = parent.id.clone();
        app.state.workspaces = vec![parent];
        app.state.ensure_test_terminals();
        app.state.worktree_directory = worktree_root.clone();
        let request = || Request {
            id: "req".into(),
            method: crate::api::schema::Method::WorktreeCreate(WorktreeCreateParams {
                workspace_id: Some(parent_id.clone()),
                branch: Some(branch.clone()),
                ..WorktreeCreateParams::default()
            }),
        };

        let (first_tx, first_rx) = response_channel();
        assert!(app.handle_deferred_worktree_api_request(request(), first_tx, false));
        let response = finish_deferred(&mut app, &first_rx);
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "worktree_create_failed");
        assert!(app.pending_api_worktree_creates.is_empty());

        let (second_tx, second_rx) = response_channel();
        assert!(app.handle_deferred_worktree_api_request(request(), second_tx, false));
        assert!(second_rx.try_recv().is_err());
        let response = finish_deferred(&mut app, &second_rx);
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "worktree_create_failed");
        assert_ne!(error.error.code, "worktree_operation_in_progress");

        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn deferred_api_worktree_create_spawn_failure_reports_error_and_clears_pending() {
        let repo = create_committed_repo("api-worktree-create-spawn-failure-repo");
        let worktree_root = unique_temp_path("api-worktree-create-spawn-failure-root");
        let mut app = app_with_parent(&repo);
        app.state.worktree_directory = worktree_root.clone();
        let request = Request {
            id: "req".into(),
            method: crate::api::schema::Method::WorktreeCreate(WorktreeCreateParams {
                workspace_id: Some(app.state.workspaces[0].id.clone()),
                branch: Some("spawn-failure".into()),
                ..WorktreeCreateParams::default()
            }),
        };

        crate::thread_spawn::test_hook::fail_next_spawns(1);
        let response = run_deferred_api_request(&mut app, request);

        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "worktree_create_failed");
        assert!(app.pending_api_worktree_creates.is_empty());
        assert!(!worktree_root.exists());

        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn deferred_api_worktree_spawn_failure_survives_full_event_queue() {
        let repo = create_committed_repo("api-worktree-create-spawn-full-repo");
        let worktree_root = unique_temp_path("api-worktree-create-spawn-full-root");
        let mut app = app_with_parent(&repo);
        app.state.worktree_directory = worktree_root.clone();
        while app
            .event_tx
            .try_send(AppEvent::UpdateReady {
                version: "9.9.9".into(),
                install_command: "herdr update".into(),
            })
            .is_ok()
        {}
        let (respond_to, response_rx) = response_channel();
        let request = Request {
            id: "req".into(),
            method: crate::api::schema::Method::WorktreeCreate(WorktreeCreateParams {
                workspace_id: Some(app.state.workspaces[0].id.clone()),
                branch: Some("spawn-failure".into()),
                ..WorktreeCreateParams::default()
            }),
        };

        crate::thread_spawn::test_hook::fail_next_spawns(1);
        assert!(app.handle_deferred_worktree_api_request(request, respond_to, false));

        let finished = loop {
            let event =
                tokio::time::timeout(std::time::Duration::from_secs(2), app.event_rx.recv())
                    .await
                    .expect("failed completion is delivered once the queue drains")
                    .expect("event channel open");
            match event {
                AppEvent::WorktreeAddFinished(_) => break event,
                // Discovery done: fill the queue again so the add's spawn fails into it.
                AppEvent::WorktreeReadFinished(_) => {
                    while app
                        .event_tx
                        .try_send(AppEvent::UpdateReady {
                            version: "9.9.9".into(),
                            install_command: "herdr update".into(),
                        })
                        .is_ok()
                    {}
                    app.handle_internal_event(event);
                }
                _ => {}
            }
        };
        app.handle_internal_event(finished);

        let response = response_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("request gets a response");
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "worktree_create_failed");
        assert!(app.pending_api_worktree_creates.is_empty());

        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn deferred_api_worktree_create_falls_back_to_active_workspace_when_target_closed() {
        let event_hub = crate::api::EventHub::default();
        let mut app = test_app_with_event_hub(event_hub.clone());
        let repo = create_committed_repo("api-worktree-create-changed-source-repo");
        let checkout = unique_temp_path("api-worktree-create-changed-source-checkout");
        std::fs::create_dir_all(&checkout).unwrap();
        let checkout_key = crate::worktree::canonical_or_original(&checkout);
        let mut active = Workspace::test_new("active");
        active.test_cwd = repo.clone();
        app.state.workspaces = vec![active];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.pending_api_worktree_creates
            .insert(checkout_key.clone(), 9);
        let (respond_to, response_rx) = response_channel();

        app.handle_api_worktree_add_finished(WorktreeAddResult {
            path: checkout.clone(),
            api_request: Some(ApiWorktreeAddRequest {
                id: "req".into(),
                operation_id: 9,
                checkout_key,
                target_workspace_id: Some("closed-workspace".into()),
                source_checkout_path: repo.clone(),
                source_repo_root: repo.clone(),
                repo_key: "repo-key".into(),
                repo_name: "herdr".into(),
                label: None,
                focus: false,
                respond_to,
            }),
            result: Ok(()),
            post_create: Ok(()),
        });

        let response = response_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("changed-source create completion should respond");
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        assert!(matches!(
            success.result,
            ResponseResult::WorktreeCreated { .. }
        ));
        assert!(event_hub
            .events_after(0)
            .into_iter()
            .any(|(_, event)| event.event == EventKind::WorktreeCreated));
        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(app.state.workspaces[0].tabs.len(), 2);
        assert_eq!(
            app.state.workspaces[0].tabs[1]
                .worktree
                .as_ref()
                .map(|membership| membership.checkout_path.clone()),
            Some(checkout.clone())
        );

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
        let _ = std::fs::remove_dir_all(checkout);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn api_worktree_create_from_cwd_without_workspaces_opens_one_for_its_tab() {
        let repo = create_committed_repo("api-worktree-create-cwd-repo");
        let worktree_root = unique_temp_path("api-worktree-create-cwd-root");
        let event_hub = crate::api::EventHub::default();
        let mut app = test_app_with_event_hub(event_hub.clone());
        app.state.worktree_directory = worktree_root.clone();
        app.state.default_shell = test_shell().into();

        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeCreate(WorktreeCreateParams {
                    cwd: Some(repo.display().to_string()),
                    branch: Some("worktree/api-create-cwd".into()),
                    ..WorktreeCreateParams::default()
                }),
            },
        );
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::WorktreeCreated { worktree, .. } = success.result else {
            panic!("expected worktree_created response");
        };

        let created = event_hub
            .events_after(0)
            .into_iter()
            .filter(|(_, event)| event.event == EventKind::WorkspaceCreated)
            .count();
        assert_eq!(created, 1, "only the worktree's own workspace opens");
        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(app.state.workspaces[0].tabs.len(), 1);
        assert!(app.state.workspaces[0].tabs[0]
            .worktree
            .as_ref()
            .is_some_and(|space| space.is_linked_worktree));

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
        let remove = crate::worktree::build_worktree_remove_command(
            &repo,
            Path::new(&worktree.path),
            false,
            false,
        );
        crate::worktree::run_worktree_command(&remove).unwrap();
        let _ = std::fs::remove_dir_all(worktree_root);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn invalid_worktree_create_from_cwd_does_not_create_parent_workspace() {
        let repo = create_committed_repo("api-worktree-create-invalid-cwd-repo");
        let mut app = test_app();

        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeCreate(WorktreeCreateParams {
                    cwd: Some(repo.display().to_string()),
                    branch: Some("   ".into()),
                    ..WorktreeCreateParams::default()
                }),
            },
        );

        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "invalid_request");
        assert!(app.state.workspaces.is_empty());
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn invalid_worktree_open_from_cwd_does_not_create_parent_workspace() {
        let repo = create_committed_repo("api-worktree-open-invalid-cwd-repo");
        let mut app = test_app();

        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeOpen(WorktreeOpenParams {
                    cwd: Some(repo.display().to_string()),
                    path: Some("/tmp/one".into()),
                    branch: Some("worktree/one".into()),
                    ..WorktreeOpenParams::default()
                }),
            },
        );

        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "invalid_request");
        assert!(app.state.workspaces.is_empty());
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn raw_api_worktree_create_rejects_relative_path_override() {
        let repo = create_committed_repo("api-worktree-relative-path-repo");
        let mut app = app_with_parent(&repo);
        let workspace_id = app.state.workspaces[0].id.clone();

        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeCreate(WorktreeCreateParams {
                    workspace_id: Some(workspace_id),
                    branch: Some("worktree/relative".into()),
                    path: Some("relative-checkout".into()),
                    ..WorktreeCreateParams::default()
                }),
            },
        );

        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "invalid_request");
        assert_eq!(app.state.workspaces.len(), 1);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn raw_api_worktree_create_rejects_relative_cwd() {
        let mut app = test_app();

        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeCreate(WorktreeCreateParams {
                    cwd: Some("relative-repo".into()),
                    branch: Some("worktree/relative-cwd".into()),
                    ..WorktreeCreateParams::default()
                }),
            },
        );

        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "invalid_request");
        assert!(app.state.workspaces.is_empty());
    }

    fn add_linked_checkout(repo: &Path, name: &str, branch: &str) -> PathBuf {
        let checkout = unique_temp_path(name);
        run_git(
            repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                branch,
                checkout.to_str().unwrap(),
                "HEAD",
            ],
        );
        // Git's spelling of the path, which create and open record.
        crate::worktree::list_existing_worktrees(repo, false)
            .unwrap()
            .into_iter()
            .find(|entry| entry.branch.as_deref() == Some(branch))
            .unwrap()
            .path
    }

    fn open_worktree(
        app: &mut App,
        workspace_id: &str,
        branch: &str,
        label: Option<&str>,
        focus: bool,
    ) -> (
        crate::api::schema::WorkspaceInfo,
        crate::api::schema::TabInfo,
        bool,
    ) {
        let response = run_deferred_api_request(
            app,
            Request {
                id: "open".into(),
                method: crate::api::schema::Method::WorktreeOpen(WorktreeOpenParams {
                    workspace_id: Some(workspace_id.into()),
                    branch: Some(branch.into()),
                    label: label.map(Into::into),
                    focus,
                    ..WorktreeOpenParams::default()
                }),
            },
        );
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::WorktreeOpened {
            workspace,
            tab,
            already_open,
            ..
        } = success.result
        else {
            panic!("expected worktree_opened response");
        };
        (workspace, tab, already_open)
    }

    #[tokio::test]
    async fn api_worktree_open_reuses_the_tab_already_open_on_the_checkout() {
        let repo = create_committed_repo("api-worktree-open-repo");
        let checkout =
            add_linked_checkout(&repo, "api-worktree-open-checkout", "worktree/api-open");
        let event_hub = crate::api::EventHub::default();
        let mut app = test_app_with_event_hub(event_hub.clone());
        let mut parent = Workspace::test_new("main");
        parent.test_cwd = repo.clone();
        let mut other = Workspace::test_new("other");
        other.test_cwd = repo.clone();
        app.state.workspaces = vec![parent, other];
        app.state.ensure_test_terminals();
        app.state.active = Some(1);
        let parent_id = app.state.workspaces[0].id.clone();
        let other_id = app.state.workspaces[1].id.clone();

        let (workspace, tab, already_open) =
            open_worktree(&mut app, &parent_id, "worktree/api-open", None, false);
        assert!(!already_open);
        assert_eq!(workspace.workspace_id, parent_id);
        assert_eq!(app.state.workspaces[0].tabs.len(), 2);
        assert_eq!(
            app.state.workspaces[0].tabs[1]
                .worktree
                .as_ref()
                .map(|space| (space.checkout_path.clone(), space.is_linked_worktree)),
            Some((checkout.clone(), true))
        );
        assert_eq!(app.state.active, Some(1));

        let (workspace, reopened, already_open) =
            open_worktree(&mut app, &other_id, "worktree/api-open", None, true);
        assert!(already_open);
        assert_eq!(workspace.workspace_id, parent_id);
        assert_eq!(reopened.tab_id, tab.tab_id);
        assert_eq!(app.state.workspaces[1].tabs.len(), 1);
        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.workspaces[0].active_tab, 1);
        assert!(event_hub.events_after(0).iter().any(|(_, event)| {
            matches!(
                &event.data,
                EventData::WorktreeOpened {
                    worktree,
                    already_open: true,
                    ..
                } if worktree.open_tab_id.as_deref() == Some(tab.tab_id.as_str())
                    && worktree.is_linked_worktree
            )
        }));
        app.state.assert_invariants_for_test();

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
        let remove = crate::worktree::build_worktree_remove_command(&repo, &checkout, false, false);
        crate::worktree::run_worktree_command(&remove).unwrap();
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn api_worktree_open_counts_only_tabs_opened_for_the_checkout() {
        let repo = create_committed_repo("api-worktree-membership-repo");
        let checkout = add_linked_checkout(
            &repo,
            "api-worktree-membership-checkout",
            "worktree/membership",
        );
        let mut app = app_with_parent(&repo);
        let parent_id = app.state.workspaces[0].id.clone();
        // A shell that cd'd into the checkout does not make its tab the checkout's.
        let mut wandering = Workspace::test_new("wandering");
        wandering.test_cwd = checkout.clone();
        app.state.workspaces.push(wandering);
        app.state.ensure_test_terminals();
        let terminal_id = app.state.workspaces[1].tabs[0]
            .terminal_id(app.state.workspaces[1].tabs[0].root_pane)
            .unwrap()
            .clone();
        let terminal = app.state.terminals.get_mut(&terminal_id).unwrap();
        terminal.cwd = checkout.clone();
        terminal.repo = Some(crate::workspace::PaneRepo::test_at(&checkout));
        assert_eq!(app.open_tab_for_checkout(&checkout), None);

        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "list".into(),
                method: crate::api::schema::Method::WorktreeList(WorktreeListParams {
                    workspace_id: Some(parent_id.clone()),
                    ..WorktreeListParams::default()
                }),
            },
        );
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::WorktreeList { worktrees, .. } = success.result else {
            panic!("expected worktree_list response");
        };
        let entry = worktrees
            .iter()
            .find(|entry| entry.branch.as_deref() == Some("worktree/membership"))
            .unwrap();
        assert_eq!(entry.open_tab_id, None);
        assert_eq!(entry.open_workspace_id, None);

        let (_, tab, already_open) =
            open_worktree(&mut app, &parent_id, "worktree/membership", None, false);
        assert!(!already_open);
        assert_eq!(tab.workspace_id, parent_id);
        assert_eq!(app.open_tab_for_checkout(&checkout), Some((0, 1)));
        assert!(app.state.workspaces[1].tabs[0].worktree.is_none());
        app.state.assert_invariants_for_test();

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
        let remove = crate::worktree::build_worktree_remove_command(&repo, &checkout, false, false);
        crate::worktree::run_worktree_command(&remove).unwrap();
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn api_worktree_open_label_renames_the_open_tab() {
        let repo = create_committed_repo("api-worktree-open-label-repo");
        let checkout = add_linked_checkout(
            &repo,
            "api-worktree-open-label-checkout",
            "worktree/api-open-label",
        );
        let event_hub = crate::api::EventHub::default();
        let mut app = test_app_with_event_hub(event_hub.clone());
        let mut parent = Workspace::test_new("main");
        parent.test_cwd = repo.clone();
        app.state.workspaces = vec![parent];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        let workspace_id = app.state.workspaces[0].id.clone();

        let (_, tab, _) = open_worktree(
            &mut app,
            &workspace_id,
            "worktree/api-open-label",
            None,
            false,
        );
        let (workspace, renamed, already_open) = open_worktree(
            &mut app,
            &workspace_id,
            "worktree/api-open-label",
            Some("review"),
            false,
        );

        assert!(already_open);
        assert_eq!(renamed.tab_id, tab.tab_id);
        assert_eq!(renamed.label, "review");
        assert_ne!(workspace.label, "review");
        assert!(event_hub.events_after(0).iter().any(|(_, event)| {
            matches!(
                &event.data,
                EventData::TabRenamed { tab_id, label, .. }
                    if tab_id == &tab.tab_id && label == "review"
            )
        }));

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
        let remove = crate::worktree::build_worktree_remove_command(&repo, &checkout, false, false);
        crate::worktree::run_worktree_command(&remove).unwrap();
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn deferred_worktree_open_rejects_removed_or_replaced_target() {
        for replacement in ["missing", "directory", "repository"] {
            let repo = create_committed_repo("api-stale-target-source");
            let checkout = unique_temp_path("api-stale-target");
            run_git(
                &repo,
                &[
                    "worktree",
                    "add",
                    "--quiet",
                    "-b",
                    "stale-target",
                    checkout.to_str().unwrap(),
                ],
            );
            let mut app = app_with_parent(&repo);
            let (respond_to, response_rx) = response_channel();
            let source_id = app.state.workspaces[0].id.clone();
            app.handle_deferred_worktree_api_request(
                Request {
                    id: "stale-target".into(),
                    method: crate::api::schema::Method::WorktreeOpen(WorktreeOpenParams {
                        workspace_id: Some(source_id),
                        branch: Some("stale-target".into()),
                        label: Some("must not rename".into()),
                        focus: true,
                        ..Default::default()
                    }),
                },
                respond_to,
                false,
            );
            let completion = wait_for_app_event(&mut app);
            run_git(&repo, &["worktree", "remove", checkout.to_str().unwrap()]);
            match replacement {
                "directory" => std::fs::create_dir_all(&checkout).unwrap(),
                "repository" => {
                    let other_repo = create_committed_repo("api-stale-target-replacement");
                    std::fs::rename(other_repo, &checkout).unwrap();
                }
                _ => {}
            }
            let mut unrelated = Workspace::test_new("unrelated");
            unrelated.test_cwd = checkout.clone();
            app.state.workspaces.push(unrelated);
            app.state.ensure_test_terminals();
            app.handle_internal_event(completion);
            let response: ErrorResponse = serde_json::from_str(
                &response_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(response.error.code, "worktree_not_found", "{replacement}");
            assert_eq!(app.state.workspaces.len(), 2);
            assert_eq!(app.state.workspaces[0].tabs.len(), 1);
            assert_eq!(app.state.active, Some(0));
            assert!(app
                .state
                .workspaces
                .iter()
                .flat_map(|ws| &ws.tabs)
                .all(|tab| tab.worktree.is_none()));
            app.state.assert_invariants_for_test();
            let _ = std::fs::remove_dir_all(checkout);
            let _ = std::fs::remove_dir_all(repo);
        }
    }

    #[tokio::test]
    async fn deferred_worktree_reads_bound_running_and_queued_discovery() {
        let repo = create_committed_repo("api-read-limit");
        let mut app = app_with_parent(&repo);
        app.worktree_read_slots = std::sync::Arc::new(tokio::sync::Semaphore::new(1));
        let source_id = app.state.workspaces[0].id.clone();
        let request = |open| Request {
            id: "read-limit".into(),
            method: if open {
                crate::api::schema::Method::WorktreeOpen(WorktreeOpenParams {
                    workspace_id: Some(source_id.clone()),
                    path: Some(repo.display().to_string()),
                    ..Default::default()
                })
            } else {
                crate::api::schema::Method::WorktreeList(WorktreeListParams {
                    workspace_id: Some(source_id.clone()),
                    ..Default::default()
                })
            },
        };
        let (entered, release) = crate::worktree::test_list_gate::block(&repo);
        let (respond_to, response_rx) = response_channel();
        app.handle_deferred_worktree_api_request(request(false), respond_to, false);
        entered
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let assert_busy = |app: &mut App| {
            for open in [false, true] {
                let (respond_to, response_rx) = response_channel();
                app.handle_deferred_worktree_api_request(request(open), respond_to, false);
                let response: ErrorResponse = serde_json::from_str(
                    &response_rx
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(response.error.code, "worktree_busy");
            }
        };
        assert_busy(&mut app);
        release.send(()).unwrap();
        let completion = wait_for_app_event(&mut app);
        assert_busy(&mut app);
        app.handle_internal_event(completion);
        let _: SuccessResponse = serde_json::from_str(
            &response_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(app.worktree_read_slots.available_permits(), 1);
        let response = run_deferred_api_request(&mut app, request(false));
        let _: SuccessResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(app.worktree_read_slots.available_permits(), 1);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn deferred_worktree_open_finds_its_workspace_again_by_id() {
        let repo = create_committed_repo("api-worktree-open-pending-source");
        let other_repo = create_committed_repo("api-worktree-open-unrelated-source");
        let checkout =
            add_linked_checkout(&repo, "api-worktree-open-pending-target", "pending-open");

        for source_change in ["reordered", "removed", "opened-meanwhile"] {
            let mut app = app_with_parent(&repo);
            let source_id = app.state.workspaces[0].id.clone();
            let (respond_to, response_rx) = response_channel();
            assert!(app.handle_deferred_worktree_api_request(
                Request {
                    id: "pending-open".into(),
                    method: crate::api::schema::Method::WorktreeOpen(WorktreeOpenParams {
                        workspace_id: Some(source_id.clone()),
                        branch: Some("pending-open".into()),
                        focus: false,
                        ..Default::default()
                    }),
                },
                respond_to,
                false
            ));
            let completed_discovery = wait_for_app_event(&mut app);
            assert!(response_rx.try_recv().is_err());
            assert_eq!(app.state.workspaces[0].tabs.len(), 1);

            let original_source = app.state.workspaces.remove(0);
            let mut unrelated = Workspace::test_new("unrelated");
            unrelated.test_cwd = other_repo.clone();
            let unrelated_id = unrelated.id.clone();
            app.state.workspaces = vec![unrelated];
            if source_change != "removed" {
                app.state.workspaces.push(original_source);
            }
            app.state.terminals.clear();
            app.state.ensure_test_terminals();
            if source_change == "opened-meanwhile" {
                app.state.workspaces[0].tabs[0].worktree =
                    Some(crate::workspace::WorktreeSpaceMembership {
                        key: "repo-key".into(),
                        label: "repo".into(),
                        repo_root: repo.clone(),
                        checkout_path: checkout.clone(),
                        is_linked_worktree: true,
                    });
            }
            app.state.active = Some(0);
            app.state.selected = 0;
            app.handle_internal_event(completed_discovery);

            let response: SuccessResponse = serde_json::from_str(
                &response_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap(),
            )
            .unwrap();
            let ResponseResult::WorktreeOpened {
                workspace,
                already_open,
                ..
            } = response.result
            else {
                panic!("expected opened response");
            };
            let (expected_workspace, expected_already_open) = match source_change {
                // The source closed: the tab opens in the active workspace instead.
                "removed" => (unrelated_id.clone(), false),
                // A tab opened on the checkout during discovery is reused.
                "opened-meanwhile" => (unrelated_id.clone(), true),
                _ => (source_id.clone(), false),
            };
            assert_eq!(
                workspace.workspace_id, expected_workspace,
                "{source_change}"
            );
            assert_eq!(already_open, expected_already_open, "{source_change}");
            assert_eq!(
                app.state.active,
                Some(0),
                "background open must preserve focus"
            );
            let (ws_idx, tab_idx) = app.open_tab_for_checkout(&checkout).unwrap();
            assert_eq!(app.state.workspaces[ws_idx].id, expected_workspace);
            assert_eq!(
                crate::worktree::canonical_or_original(
                    &app.state.workspaces[ws_idx].tabs[tab_idx]
                        .worktree
                        .as_ref()
                        .unwrap()
                        .repo_root
                ),
                crate::worktree::canonical_or_original(&repo)
            );
            app.state.assert_invariants_for_test();
            for (_, runtime) in app.terminal_runtimes.drain() {
                runtime.shutdown();
            }
        }
        let remove = crate::worktree::build_worktree_remove_command(&repo, &checkout, false, false);
        crate::worktree::run_worktree_command(&remove).unwrap();
        let _ = std::fs::remove_dir_all(repo);
        let _ = std::fs::remove_dir_all(other_repo);
    }

    #[tokio::test]
    async fn api_worktree_open_source_checkout_created_by_request_is_not_already_open() {
        let repo = create_committed_repo("api-worktree-open-source-repo");
        let event_hub = crate::api::EventHub::default();
        let mut app = test_app_with_event_hub(event_hub.clone());
        app.state.default_shell = test_shell().into();

        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeOpen(WorktreeOpenParams {
                    cwd: Some(repo.display().to_string()),
                    path: Some(repo.display().to_string()),
                    label: Some("source checkout".into()),
                    ..WorktreeOpenParams::default()
                }),
            },
        );

        let success: SuccessResponse = serde_json::from_str(&response).unwrap_or_else(|err| {
            panic!("expected success response, got {response}: {err}");
        });
        let ResponseResult::WorktreeOpened {
            tab, already_open, ..
        } = success.result
        else {
            panic!("expected worktree_opened response");
        };
        assert!(!already_open);
        assert_eq!(tab.label, "source checkout");
        assert!(tab
            .worktree
            .as_ref()
            .is_some_and(|worktree| !worktree.is_linked_worktree));
        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(
            event_hub
                .events_after(0)
                .iter()
                .filter(|(_, event)| event.event == EventKind::WorkspaceCreated)
                .count(),
            1
        );
        assert!(!event_hub
            .events_after(0)
            .iter()
            .any(|(_, event)| { matches!(&event.data, EventData::WorkspaceRenamed { .. }) }));

        app.state.selected = 0;
        app.state.close_selected_workspace();
        app.shutdown_detached_terminal_runtimes();
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn api_worktree_list_reports_open_tabs() {
        let repo = create_committed_repo("api-worktree-list-repo");
        let checkout = unique_temp_path("api-worktree-list-checkout");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "worktree/api-list",
                checkout.to_str().unwrap(),
                "HEAD",
            ],
        );
        let mut app = app_with_parent(&repo);
        let mut child = Workspace::test_new("child");
        child.test_cwd = checkout.clone();
        child.tabs[0].worktree = Some(crate::workspace::WorktreeSpaceMembership {
            key: crate::workspace::git_space_metadata(&repo).unwrap().key,
            label: "api-worktree-list-repo".into(),
            repo_root: repo.clone(),
            checkout_path: checkout.clone(),
            is_linked_worktree: true,
        });
        app.state.workspaces.push(child);
        app.state.ensure_test_terminals();

        let workspace_id = app.state.workspaces[0].id.clone();
        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeList(WorktreeListParams {
                    workspace_id: Some(workspace_id),
                    cwd: None,
                    trust_repository: false,
                }),
            },
        );

        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::WorktreeList { worktrees, .. } = success.result else {
            panic!("expected worktree_list response");
        };
        let entry = worktrees
            .iter()
            .find(|entry| entry.branch.as_deref() == Some("worktree/api-list"))
            .unwrap();
        assert_eq!(
            entry.open_workspace_id.as_deref(),
            Some(app.state.workspaces[1].id.as_str())
        );
        assert_eq!(entry.open_tab_id, app.public_tab_id(1, 0));
        assert!(entry.is_linked_worktree);

        let remove = crate::worktree::build_worktree_remove_command(&repo, &checkout, false, false);
        crate::worktree::run_worktree_command(&remove).unwrap();
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn api_worktree_list_accepts_linked_checkout_sources() {
        let repo = create_committed_repo("api-worktree-list-linked-repo");
        let checkout = unique_temp_path("api-worktree-list-linked-checkout");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "worktree/api-list-linked",
                checkout.to_str().unwrap(),
                "HEAD",
            ],
        );
        let mut app = app_with_parent(&repo);
        let parent_id = app.state.workspaces[0].id.clone();
        let mut child = Workspace::test_new("child");
        child.test_cwd = checkout.clone();
        let child_id = child.id.clone();
        app.state.workspaces.push(child);
        app.state.ensure_test_terminals();

        for (method, target_id) in [
            (
                crate::api::schema::Method::WorktreeList(WorktreeListParams {
                    workspace_id: Some(child_id.clone()),
                    cwd: None,
                    trust_repository: false,
                }),
                child_id.clone(),
            ),
            (
                crate::api::schema::Method::WorktreeList(WorktreeListParams {
                    workspace_id: None,
                    cwd: Some(checkout.display().to_string()),
                    trust_repository: false,
                }),
                parent_id.clone(),
            ),
        ] {
            let response = run_deferred_api_request(
                &mut app,
                Request {
                    id: "req".into(),
                    method,
                },
            );
            let success: SuccessResponse = serde_json::from_str(&response).unwrap();
            let ResponseResult::WorktreeList { source, worktrees } = success.result else {
                panic!("expected worktree_list response");
            };
            assert_eq!(
                crate::worktree::canonical_or_original(std::path::Path::new(&source.repo_root)),
                crate::worktree::canonical_or_original(&repo)
            );
            assert_eq!(
                source.source_workspace_id.as_deref(),
                Some(target_id.as_str())
            );
            assert!(worktrees.iter().any(|entry| {
                entry.branch.as_deref() == Some("worktree/api-list-linked")
                    && entry.is_linked_worktree
            }));
        }

        let remove = crate::worktree::build_worktree_remove_command(&repo, &checkout, false, false);
        crate::worktree::run_worktree_command(&remove).unwrap();
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn api_worktree_list_preserves_prunable_entries() {
        let repo = create_committed_repo("api-worktree-list-prunable-repo");
        let checkout = unique_temp_path("api-worktree-list-prunable-checkout");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "worktree/api-list-prunable",
                checkout.to_str().unwrap(),
                "HEAD",
            ],
        );
        std::fs::remove_dir_all(&checkout).unwrap();
        let mut app = app_with_parent(&repo);

        let workspace_id = app.state.workspaces[0].id.clone();
        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeList(WorktreeListParams {
                    workspace_id: Some(workspace_id),
                    cwd: None,
                    trust_repository: false,
                }),
            },
        );

        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::WorktreeList { worktrees, .. } = success.result else {
            panic!("expected worktree_list response");
        };
        let entry = worktrees
            .iter()
            .find(|entry| entry.branch.as_deref() == Some("worktree/api-list-prunable"))
            .unwrap();
        assert!(entry.is_prunable);
        assert!(entry.is_linked_worktree);

        run_git(&repo, &["worktree", "prune"]);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn api_worktree_remove_requires_force_for_dirty_checkout() {
        let repo = create_committed_repo("api-worktree-remove-repo");
        let checkout = unique_temp_path("api-worktree-remove-checkout");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "worktree/api-remove",
                checkout.to_str().unwrap(),
                "HEAD",
            ],
        );
        std::fs::write(checkout.join("README.md"), "dirty\n").unwrap();

        let mut app = app_with_parent(&repo);
        let mut child = Workspace::test_new("child");
        child.test_cwd = checkout.clone();
        child.tabs[0].worktree = Some(crate::workspace::WorktreeSpaceMembership {
            key: crate::workspace::git_space_metadata(&repo).unwrap().key,
            label: "api-worktree-remove-repo".into(),
            repo_root: repo.clone(),
            checkout_path: checkout.clone(),
            is_linked_worktree: true,
        });
        let child_id = child.id.clone();
        app.state.workspaces.push(child);
        app.state.ensure_test_terminals();

        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeRemove(WorktreeRemoveParams {
                    workspace_id: child_id.clone(),
                    force: false,
                    trust_repository: false,
                }),
            },
        );
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "dirty_worktree_requires_force");
        assert!(checkout.exists());
        assert_eq!(app.state.workspaces.len(), 2);

        let tab_id = app.public_tab_id(1, 0);
        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeRemoveTab(WorktreeRemoveTabParams {
                    tab_id,
                    force: true,
                    ..WorktreeRemoveTabParams::default()
                }),
            },
        );
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::WorktreeRemoved { forced, path, .. } = success.result else {
            panic!("expected worktree_removed response");
        };
        assert!(forced);
        assert_eq!(path, checkout.display().to_string());
        assert!(!checkout.exists());
        assert_eq!(app.state.workspaces.len(), 1);

        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn api_worktree_remove_emits_close_event_and_drains_runtime_shutdowns() {
        let repo = create_committed_repo("api-worktree-remove-event-repo");
        let checkout = unique_temp_path("api-worktree-remove-event-checkout");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "worktree/api-remove-event",
                checkout.to_str().unwrap(),
                "HEAD",
            ],
        );

        let event_hub = crate::api::EventHub::default();
        let mut app = test_app_with_event_hub(event_hub.clone());
        let mut child = Workspace::test_new("child");
        child.test_cwd = checkout.clone();
        child.tabs[0].worktree = Some(crate::workspace::WorktreeSpaceMembership {
            key: crate::workspace::git_space_metadata(&repo).unwrap().key,
            label: "api-worktree-remove-event-repo".into(),
            repo_root: repo.clone(),
            checkout_path: checkout.clone(),
            is_linked_worktree: true,
        });
        let child_id = child.id.clone();
        app.state.workspaces.push(child);
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;

        let response = run_deferred_api_request(
            &mut app,
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeRemove(WorktreeRemoveParams {
                    workspace_id: child_id.clone(),
                    force: false,
                    trust_repository: false,
                }),
            },
        );
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        assert!(matches!(
            success.result,
            ResponseResult::WorktreeRemoved { .. }
        ));
        assert!(app.state.workspaces.is_empty());
        assert!(app.state.terminal_runtime_shutdowns.is_empty());
        assert!(event_hub.events_after(0).iter().any(|(_, event)| {
            matches!(
                &event.data,
                EventData::TabClosed { workspace_id, .. } if workspace_id == &child_id
            )
        }));
        assert!(event_hub.events_after(0).iter().any(|(_, event)| {
            matches!(
                &event.data,
                EventData::WorkspaceClosed {
                    workspace_id,
                    workspace: Some(_),
                } if workspace_id == &child_id
            )
        }));
        assert!(event_hub.events_after(0).iter().any(|(_, event)| {
            matches!(
                &event.data,
                EventData::WorktreeRemoved {
                    workspace_id,
                    workspace: Some(workspace),
                    worktree,
                    forced,
                } if workspace_id == &child_id
                    && workspace.workspace_id == child_id
                    && worktree.branch.as_deref() == Some("worktree/api-remove-event")
                    && worktree.is_linked_worktree
                    && worktree.open_workspace_id.is_none()
                    && !forced
            )
        }));

        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn deferred_api_worktree_remove_preserves_event_and_plugin_context() {
        let repo = create_committed_repo("api-worktree-remove-deferred-repo");
        let checkout = unique_temp_path("api-worktree-remove-deferred-checkout");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "worktree/api-remove-deferred",
                checkout.to_str().unwrap(),
                "HEAD",
            ],
        );

        let event_hub = crate::api::EventHub::default();
        let mut app = test_app_with_event_hub(event_hub.clone());
        let mut child = Workspace::test_new("child");
        child.test_cwd = checkout.clone();
        child.tabs[0].worktree = Some(crate::workspace::WorktreeSpaceMembership {
            key: crate::workspace::git_space_metadata(&repo).unwrap().key,
            label: "api-worktree-remove-deferred-repo".into(),
            repo_root: repo.clone(),
            checkout_path: checkout.clone(),
            is_linked_worktree: true,
        });
        let child_id = child.id.clone();
        app.state.workspaces.push(child);
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        let plugin_root = install_event_plugin(&mut app, "deferred-remove", "worktree.removed");
        let (respond_to, response_rx) = response_channel();

        assert!(app.handle_deferred_worktree_api_request(
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeRemove(WorktreeRemoveParams {
                    workspace_id: child_id.clone(),
                    force: false,
                    trust_repository: false,
                }),
            },
            respond_to,
            false,
        ));
        assert!(response_rx.try_recv().is_err());

        let event = wait_for_app_event(&mut app);
        app.handle_internal_event(event);
        let response = response_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("deferred worktree remove should respond after completion event");
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        assert!(matches!(
            success.result,
            ResponseResult::WorktreeRemoved { .. }
        ));
        let kinds = event_hub
            .events_after(0)
            .into_iter()
            .map(|(_, event)| event.event)
            .collect::<Vec<_>>();
        assert_eq!(
            &kinds[kinds.len() - 3..],
            &[
                EventKind::TabClosed,
                EventKind::WorkspaceClosed,
                EventKind::WorktreeRemoved
            ]
        );
        assert!(app.state.plugin_command_logs.iter().any(|log| {
            log.event.as_deref() == Some("worktree.removed")
                && log.status == crate::api::schema::PluginCommandStatus::Running
        }));
        assert!(app.state.workspaces.is_empty());

        let _ = std::fs::remove_dir_all(repo);
        let _ = std::fs::remove_dir_all(plugin_root);
    }

    #[test]
    fn deferred_api_worktree_remove_rejects_duplicate_in_flight_request() {
        let repo = create_committed_repo("api-worktree-remove-duplicate-repo");
        let checkout = unique_temp_path("api-worktree-remove-duplicate-checkout");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "worktree/api-remove-duplicate",
                checkout.to_str().unwrap(),
                "HEAD",
            ],
        );

        let mut app = test_app();
        let mut child = Workspace::test_new("child");
        child.test_cwd = checkout.clone();
        child.tabs[0].worktree = Some(crate::workspace::WorktreeSpaceMembership {
            key: crate::workspace::git_space_metadata(&repo).unwrap().key,
            label: "api-worktree-remove-duplicate-repo".into(),
            repo_root: repo.clone(),
            checkout_path: checkout.clone(),
            is_linked_worktree: true,
        });
        let child_id = child.id.clone();
        app.state.workspaces.push(child);
        app.state.ensure_test_terminals();
        let (first_tx, _first_rx) = response_channel();
        let (second_tx, second_rx) = response_channel();

        assert!(app.handle_deferred_worktree_api_request(
            Request {
                id: "first".into(),
                method: crate::api::schema::Method::WorktreeRemove(WorktreeRemoveParams {
                    workspace_id: child_id.clone(),
                    force: false,
                    trust_repository: false,
                }),
            },
            first_tx,
            false,
        ));
        assert!(app.handle_deferred_worktree_api_request(
            Request {
                id: "second".into(),
                method: crate::api::schema::Method::WorktreeRemove(WorktreeRemoveParams {
                    workspace_id: child_id,
                    force: false,
                    trust_repository: false,
                }),
            },
            second_tx,
            false,
        ));
        let response = second_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("duplicate request should respond immediately");
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "worktree_operation_in_progress");

        let event = wait_for_app_event(&mut app);
        app.handle_internal_event(event);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn deferred_api_worktree_remove_rejects_duplicate_checkout_path_in_flight_request() {
        let repo = create_committed_repo("api-worktree-remove-duplicate-path-repo");
        let checkout = unique_temp_path("api-worktree-remove-duplicate-path-checkout");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "worktree/api-remove-duplicate-path",
                checkout.to_str().unwrap(),
                "HEAD",
            ],
        );

        let mut app = test_app();
        let membership = crate::workspace::WorktreeSpaceMembership {
            key: crate::workspace::git_space_metadata(&repo).unwrap().key,
            label: "api-worktree-remove-duplicate-path-repo".into(),
            repo_root: repo.clone(),
            checkout_path: checkout.clone(),
            is_linked_worktree: true,
        };
        let mut first = Workspace::test_new("first");
        first.test_cwd = checkout.clone();
        first.tabs[0].worktree = Some(membership.clone());
        let first_id = first.id.clone();
        let mut second = Workspace::test_new("second");
        second.test_cwd = checkout.clone();
        second.tabs[0].worktree = Some(membership);
        let second_id = second.id.clone();
        app.state.workspaces = vec![first, second];
        app.state.ensure_test_terminals();
        let (first_tx, _first_rx) = response_channel();
        let (second_tx, second_rx) = response_channel();

        assert!(app.handle_deferred_worktree_api_request(
            Request {
                id: "first".into(),
                method: crate::api::schema::Method::WorktreeRemove(WorktreeRemoveParams {
                    workspace_id: first_id,
                    force: true,
                    trust_repository: false,
                }),
            },
            first_tx,
            false,
        ));
        assert!(app.handle_deferred_worktree_api_request(
            Request {
                id: "second".into(),
                method: crate::api::schema::Method::WorktreeRemove(WorktreeRemoveParams {
                    workspace_id: second_id,
                    force: true,
                    trust_repository: false,
                }),
            },
            second_tx,
            false,
        ));
        let response = second_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("duplicate path request should respond immediately");
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "worktree_operation_in_progress");

        let event = wait_for_app_event(&mut app);
        app.handle_internal_event(event);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn deferred_api_worktree_create_rejects_checkout_with_remove_in_flight() {
        let repo = create_committed_repo("api-worktree-create-remove-in-flight-repo");
        let checkout = unique_temp_path("api-worktree-create-remove-in-flight-checkout");
        let mut app = test_app();
        app.pending_api_worktree_remove_paths
            .insert(crate::worktree::canonical_or_original(&checkout), 7);
        let (respond_to, response_rx) = response_channel();

        assert!(app.handle_deferred_worktree_api_request(
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeCreate(WorktreeCreateParams {
                    workspace_id: None,
                    cwd: Some(repo.display().to_string()),
                    branch: Some("worktree/create-remove-in-flight".into()),
                    base: None,
                    path: Some(checkout.display().to_string()),
                    label: None,
                    focus: false,
                    trust_repository: false,
                }),
            },
            respond_to,
            false,
        ));

        let response = finish_deferred(&mut app, &response_rx);
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "worktree_operation_in_progress");
        assert!(app.event_rx.try_recv().is_err());
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn deferred_api_worktree_remove_rejects_checkout_with_create_in_flight() {
        let repo = create_committed_repo("api-worktree-remove-create-in-flight-repo");
        let checkout = unique_temp_path("api-worktree-remove-create-in-flight-checkout");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "worktree/api-remove-create-in-flight",
                checkout.to_str().unwrap(),
                "HEAD",
            ],
        );

        let mut app = test_app();
        let mut child = Workspace::test_new("child");
        child.test_cwd = checkout.clone();
        child.tabs[0].worktree = Some(crate::workspace::WorktreeSpaceMembership {
            key: crate::workspace::git_space_metadata(&repo).unwrap().key,
            label: "api-worktree-remove-create-in-flight-repo".into(),
            repo_root: repo.clone(),
            checkout_path: checkout.clone(),
            is_linked_worktree: true,
        });
        let child_id = child.id.clone();
        app.state.workspaces.push(child);
        app.state.ensure_test_terminals();
        app.pending_api_worktree_creates
            .insert(crate::worktree::canonical_or_original(&checkout), 7);
        let (respond_to, response_rx) = response_channel();

        assert!(app.handle_deferred_worktree_api_request(
            Request {
                id: "req".into(),
                method: crate::api::schema::Method::WorktreeRemove(WorktreeRemoveParams {
                    workspace_id: child_id,
                    force: false,
                    trust_repository: false,
                }),
            },
            respond_to,
            false,
        ));

        let response = response_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("remove should reject checkout with create in flight");
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "worktree_operation_in_progress");
        assert!(app.event_rx.try_recv().is_err());
        let remove = crate::worktree::build_worktree_remove_command(&repo, &checkout, true, false);
        let _ = crate::worktree::run_worktree_command(&remove);
        let _ = std::fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn failed_worktree_remove_preserves_and_restores_shutdown_pane() {
        let checkout = unique_temp_path("api-worktree-remove-failure-checkout");
        std::fs::create_dir_all(&checkout).unwrap();
        let mut app = test_app();
        app.state.default_shell = test_shell().into();
        let mut workspace = Workspace::test_new("worktree");
        workspace.test_cwd = checkout.clone();
        workspace.tabs[0].worktree = Some(crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "repo".into(),
            repo_root: checkout.clone(),
            checkout_path: checkout.clone(),
            is_linked_worktree: true,
        });
        let workspace_id = workspace.id.clone();
        let pane_id = workspace.tabs[0].root_pane;
        let terminal_id = workspace.terminal_id(pane_id).cloned().unwrap();
        let foreground = Workspace::test_new("foreground");
        let foreground_id = foreground.id.clone();
        app.state.workspaces = vec![workspace, foreground];
        app.state.active = Some(1);
        app.state.selected = 1;
        app.state.ensure_test_terminals();
        app.state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .set_detected_state(
                Some(crate::detect::Agent::Codex),
                crate::detect::AgentState::Working,
            );
        app.state.toast_config.delay_seconds = 1;
        app.handle_internal_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(crate::detect::Agent::Codex),
            state: crate::detect::AgentState::Blocked,
            visible_blocker: true,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });
        assert!(app.state.pending_agent_notifications.contains_key(&pane_id));
        let (runtime, _input_rx) = crate::terminal::TerminalRuntime::test_with_channel(80, 24);
        app.terminal_runtimes.insert(terminal_id.clone(), runtime);

        let shutdown_panes = app.shutdown_terminal_runtimes_for_worktree_remove(&[pane_id]);
        assert_eq!(shutdown_panes, vec![pane_id]);

        let checkout_key = crate::worktree::canonical_or_original(&checkout);
        app.pending_api_worktree_remove_paths
            .insert(checkout_key.clone(), 7);
        let workspace_snapshot = app.workspace_info(0);
        let worktree_snapshot = app.worktree_info_for_membership(
            app.state.workspaces[0].tabs[0].worktree.as_ref().unwrap(),
            None,
        );
        let (respond_to, response_rx) = response_channel();
        let pane_updates = app.handle_api_worktree_remove_finished(WorktreeRemoveResult {
            workspace_id,
            path: checkout.clone(),
            workspace: Some(Box::new(workspace_snapshot)),
            worktree: Some(Box::new(worktree_snapshot)),
            forced: false,
            api_request: Some(ApiWorktreeRemoveRequest {
                id: "req".into(),
                operation_id: 7,
                checkout_key,
                shutdown_panes,
                respond_to,
            }),
            result: Err("simulated remove failure".into()),
        });
        assert!(pane_updates.is_empty());

        let response = response_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let error: ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(error.error.code, "worktree_remove_failed");
        assert!(app.find_pane(pane_id).is_some());
        assert!(app.terminal_runtimes.get(&terminal_id).is_none());
        assert!(app
            .pending_worktree_remove_runtime_restores
            .contains_key(&pane_id));

        let pane_updates = app.handle_internal_event_with_pane_updates(AppEvent::PaneDied {
            pane_id,
            exit_reason: crate::platform::ChildExitReason::Exited,
        });
        assert!(matches!(
            pane_updates.as_slice(),
            [update] if update.agent_released && update.suppress_completion
        ));
        assert!(app.pending_worktree_remove_runtime_exits.is_empty());
        assert!(app.pending_worktree_remove_runtime_restores.is_empty());
        assert!(app.state.pending_agent_notifications.is_empty());
        assert!(app.terminal_runtimes.get(&terminal_id).is_some());
        assert_eq!(
            app.state.active.map(|idx| &app.state.workspaces[idx].id),
            Some(&foreground_id)
        );
        assert_eq!(app.state.workspaces[app.state.selected].id, foreground_id);

        crate::app::api::test_support::shutdown_test_runtimes(&mut app);
        let _ = std::fs::remove_dir_all(checkout);
    }

    #[test]
    fn deferred_api_background_worktree_remove_preserves_focused_workspace() {
        let mut app = test_app();
        let checkout = PathBuf::from("/repo/herdr-issue");
        let membership = crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "herdr".into(),
            repo_root: "/repo/herdr".into(),
            checkout_path: checkout.clone(),
            is_linked_worktree: true,
        };
        let parent = Workspace::test_new("parent");
        let mut child = Workspace::test_new("child");
        child.tabs[0].worktree = Some(membership.clone());
        let foreground = Workspace::test_new("foreground");
        let child_id = child.id.clone();
        let child_pane_id = child.tabs[0].root_pane;
        let foreground_id = foreground.id.clone();
        app.state.workspaces = vec![parent, child, foreground];
        app.state.active = Some(2);
        app.state.selected = 2;
        app.pending_worktree_remove_runtime_exits
            .insert(child_pane_id, 1);
        app.pending_worktree_remove_runtime_restores
            .insert(child_pane_id, 7);
        let workspace_snapshot = app.workspace_info(1);
        let worktree_snapshot = app.worktree_info_for_membership(&membership, None);
        app.pending_api_worktree_remove_paths
            .insert(crate::worktree::canonical_or_original(&checkout), 7);
        let (respond_to, response_rx) = response_channel();

        let _ = app.handle_api_worktree_remove_finished(WorktreeRemoveResult {
            workspace_id: child_id,
            path: checkout.clone(),
            workspace: Some(Box::new(workspace_snapshot)),
            worktree: Some(Box::new(worktree_snapshot)),
            forced: false,
            api_request: Some(ApiWorktreeRemoveRequest {
                id: "req".into(),
                operation_id: 7,
                checkout_key: crate::worktree::canonical_or_original(&checkout),
                shutdown_panes: vec![child_pane_id],
                respond_to,
            }),
            result: Ok(()),
        });

        let response = response_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("background worktree remove completion should respond");
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        assert!(matches!(
            success.result,
            ResponseResult::WorktreeRemoved { .. }
        ));
        assert_eq!(app.state.workspaces.len(), 2);
        assert_eq!(
            app.state.active.map(|idx| &app.state.workspaces[idx].id),
            Some(&foreground_id)
        );
        assert_eq!(app.state.workspaces[app.state.selected].id, foreground_id);
        assert!(app.pending_worktree_remove_runtime_exits.is_empty());
        assert!(app.pending_worktree_remove_runtime_restores.is_empty());
    }

    #[tokio::test]
    async fn deferred_api_worktree_remove_keeps_a_tab_whose_checkout_changed_meanwhile() {
        let event_hub = crate::api::EventHub::default();
        let mut app = test_app_with_event_hub(event_hub.clone());
        let checkout = PathBuf::from("/repo/herdr-issue");
        let mut child = Workspace::test_new("child");
        child.tabs[0].worktree = Some(crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "herdr".into(),
            repo_root: "/repo/herdr".into(),
            checkout_path: checkout.clone(),
            is_linked_worktree: true,
        });
        let child_id = child.id.clone();
        let child_pane_id = child.tabs[0].root_pane;
        let child_terminal_id = child.terminal_id(child_pane_id).cloned().unwrap();
        app.state.workspaces.push(child);
        app.state.ensure_test_terminals();
        app.state.terminals.get_mut(&child_terminal_id).unwrap().cwd = checkout.clone();
        let workspace_snapshot = app.workspace_info(0);
        let worktree_snapshot = app.worktree_info_for_membership(
            app.state.workspaces[0].tabs[0].worktree.as_ref().unwrap(),
            None,
        );
        app.pending_api_worktree_remove_paths
            .insert(crate::worktree::canonical_or_original(&checkout), 7);
        app.pending_worktree_remove_runtime_exits
            .insert(child_pane_id, 1);
        app.state.workspaces[0].tabs[0].worktree =
            Some(crate::workspace::WorktreeSpaceMembership {
                key: "repo-key".into(),
                label: "herdr".into(),
                repo_root: "/repo/herdr".into(),
                checkout_path: "/repo/other".into(),
                is_linked_worktree: true,
            });
        let (respond_to, response_rx) = response_channel();

        let _ = app.handle_api_worktree_remove_finished(WorktreeRemoveResult {
            workspace_id: child_id,
            path: checkout.clone(),
            workspace: Some(Box::new(workspace_snapshot)),
            worktree: Some(Box::new(worktree_snapshot)),
            forced: true,
            api_request: Some(ApiWorktreeRemoveRequest {
                id: "req".into(),
                operation_id: 7,
                checkout_key: crate::worktree::canonical_or_original(&checkout),
                shutdown_panes: vec![child_pane_id],
                respond_to,
            }),
            result: Ok(()),
        });

        let response = response_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("changed-tab remove completion should respond");
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        assert!(matches!(
            success.result,
            ResponseResult::WorktreeRemoved { .. }
        ));
        assert_eq!(
            event_hub
                .events_after(0)
                .into_iter()
                .map(|(_, event)| event.event)
                .collect::<Vec<_>>(),
            vec![EventKind::WorktreeRemoved]
        );
        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(
            app.state.workspaces[0].tabs[0]
                .worktree
                .as_ref()
                .map(|membership| membership.checkout_path.as_path()),
            Some(Path::new("/repo/other"))
        );
        assert_eq!(
            app.state.terminals[&child_terminal_id].cwd,
            PathBuf::from("/repo/other")
        );
    }

    #[test]
    fn worktree_remove_by_workspace_needs_exactly_one_worktree_tab() {
        let mut app = test_app();
        let membership = |checkout: &str| crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "herdr".into(),
            repo_root: "/repo/herdr".into(),
            checkout_path: checkout.into(),
            is_linked_worktree: true,
        };
        let mut workspace = Workspace::test_new("effort");
        workspace.test_add_tab(None);
        workspace.test_add_tab(None);
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        let params = |workspace_id: &str| WorktreeRemoveTabParams {
            workspace_id: Some(workspace_id.into()),
            ..WorktreeRemoveTabParams::default()
        };
        let workspace_id = app.state.workspaces[0].id.clone();
        let code = |app: &App| {
            app.worktree_remove_target(&params(&workspace_id))
                .err()
                .map(|err| err.code)
        };

        assert_eq!(code(&app), Some("not_linked_worktree"));
        app.state.workspaces[0].tabs[1].worktree = Some(membership("/repo/one"));
        assert_eq!(
            app.worktree_remove_target(&params(&workspace_id)).ok(),
            Some((0, 1))
        );
        app.state.workspaces[0].tabs[2].worktree = Some(membership("/repo/two"));
        assert_eq!(code(&app), Some("ambiguous_worktree_tab"));
        app.state.workspaces[0].active_tab = 2;
        assert_eq!(
            app.worktree_remove_target(&params(&workspace_id)).ok(),
            Some((0, 2))
        );
        assert_eq!(
            app.worktree_remove_target(&WorktreeRemoveTabParams::default())
                .err()
                .map(|err| err.code),
            Some("invalid_request")
        );
    }

    #[test]
    fn worktree_remove_closes_every_tab_opened_for_the_checkout() {
        let mut app = test_app();
        let checkout = PathBuf::from("/repo/herdr-issue");
        let membership = crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "herdr".into(),
            repo_root: "/repo/herdr".into(),
            checkout_path: checkout.clone(),
            is_linked_worktree: true,
        };
        let mut first = Workspace::test_new("first");
        first.test_add_tab(None);
        first.tabs[1].worktree = Some(membership.clone());
        let mut second = Workspace::test_new("second");
        second.tabs[0].worktree = Some(membership);
        let first_id = first.id.clone();
        app.state.workspaces = vec![first, second];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);

        app.close_removed_worktree_tabs(&checkout);

        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(app.state.workspaces[0].id, first_id);
        assert_eq!(app.state.workspaces[0].tabs.len(), 1);
        assert!(app.tabs_for_checkout(&checkout).is_empty());
        app.state.assert_invariants_for_test();
    }
}
