use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use tracing::warn;

use super::{App, GIT_REMOTE_STATUS_REFRESH_INTERVAL, GIT_REPO_DISCOVERY_REFRESH_INTERVAL};
use crate::events::AppEvent;
use crate::terminal::TerminalId;
use crate::workspace::{GitStatusCacheEntry, GitStatusRefreshDemand, PaneGitStatus, PaneRepo};

#[derive(Clone, Debug, PartialEq, Eq)]
struct PaneGitRefreshItem {
    terminal_id: TerminalId,
    cwd: PathBuf,
    cache_key_hint: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PaneGitRefreshTarget {
    terminal_id: TerminalId,
    cwd: PathBuf,
}

/// One checkout's refresh, shared by every pane whose cwd is inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CheckoutGitRefreshJob {
    cache_key: PathBuf,
    cached: Option<GitStatusCacheEntry>,
    targets: Vec<PaneGitRefreshTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PaneGitRefreshOutput {
    results: Vec<PaneGitStatus>,
    cache_updates: Vec<(PathBuf, GitStatusCacheEntry)>,
}

impl App {
    pub(crate) fn refresh_restored_workspace_git_metadata(&mut self) {
        if self.state.workspaces.is_empty() {
            return;
        }
        let now = Instant::now();
        self.request_git_identity_refresh(now);
        // Start once even without an attached TUI or sidebar Git tokens. Reuse
        // the single detached worker so stalled I/O cannot block server shutdown.
        self.start_git_status_refresh_if_due(now);
    }

    pub(crate) fn start_git_status_refresh_if_due(&mut self, now: Instant) {
        let Some(deadline) = self.git_refresh_deadline() else {
            return;
        };

        if now < deadline {
            return;
        }

        let refresh_repo_discovery = self.git_identity_refresh_requested
            || now.saturating_duration_since(self.last_git_repo_discovery_refresh)
                >= GIT_REPO_DISCOVERY_REFRESH_INTERVAL;
        let panes = self.pane_git_refresh_items(refresh_repo_discovery);

        if panes.is_empty() {
            self.last_git_remote_status_refresh = now;
            self.git_identity_refresh_requested = false;
            self.git_refresh_spawn_retry_pending = false;
            return;
        }

        let event_tx = self.event_tx.clone();
        let cache = self.git_status_cache.clone();
        let mut demand = self.git_refresh_demand();
        if self.git_identity_refresh_requested {
            demand.branch = true;
        }
        let spawned = crate::thread_spawn::spawn_named("herdr-git-refresh", move || {
            let output = refresh_pane_git_statuses_with_cache_and_demand(panes, &cache, demand);
            let _ = event_tx.blocking_send(AppEvent::GitStatusRefreshed {
                results: output.results,
                cache_updates: output.cache_updates,
            });
        });
        if let Err(err) = spawned {
            // Keep the pending requests and retry after the normal interval.
            warn!(err = %err, "failed to spawn git refresh thread; retrying later");
            self.last_git_remote_status_refresh = now;
            self.git_refresh_spawn_retry_pending = true;
            return;
        }
        self.git_refresh_spawn_retry_pending = false;
        self.git_refresh_in_flight = true;
        self.git_identity_refresh_requested = false;
        if refresh_repo_discovery {
            self.last_git_repo_discovery_refresh = now;
        }
    }

    pub(crate) fn request_git_identity_refresh(&mut self, now: Instant) {
        self.git_identity_refresh_requested = true;
        self.mark_git_status_refresh_due(now);
    }

    pub(crate) fn mark_git_status_refresh_due(&mut self, now: Instant) {
        self.git_status_cache
            .retain(|_, entry| entry.fingerprint.is_some());
        if self.git_refresh_in_flight {
            self.git_refresh_due_after_in_flight = true;
            return;
        }
        self.last_git_remote_status_refresh = now
            .checked_sub(GIT_REMOTE_STATUS_REFRESH_INTERVAL)
            .unwrap_or(now);
        self.git_refresh_due_after_in_flight = false;
    }

    pub(crate) fn git_refresh_deadline(&self) -> Option<Instant> {
        (!self.git_refresh_in_flight
            && !self.state.workspaces.is_empty()
            && (self.git_identity_refresh_requested || !self.git_refresh_demand().is_empty()))
        .then_some(self.last_git_remote_status_refresh + GIT_REMOTE_STATUS_REFRESH_INTERVAL)
    }

    /// Branch and ahead/behind stay fresh for every pane, since the API exposes
    /// them; they cost file reads, and `git rev-list` only when HEAD or its
    /// upstream moves. The dirty count needs `git status`, so it runs only while
    /// an agent row shows it.
    fn git_refresh_demand(&self) -> GitStatusRefreshDemand {
        let agents = &self.state.sidebar_agents;
        let dirty = agents
            .rows
            .iter()
            .chain(agents.rows_by_agent.values().flatten())
            .flatten()
            .any(|token| matches!(token.parts().0, crate::config::AgentSidebarToken::GitDirty));
        GitStatusRefreshDemand {
            branch: true,
            ahead_behind: true,
            dirty,
        }
    }

    /// One item per terminal shown in a pane. Reading a pane's cwd is a cheap
    /// accessor; no Git work happens here.
    fn pane_git_refresh_items(&self, refresh_repo_discovery: bool) -> Vec<PaneGitRefreshItem> {
        let mut items = Vec::new();
        for tab in self.state.workspaces.iter().flat_map(|ws| ws.tabs.iter()) {
            for pane_id in tab.panes.keys() {
                let Some(terminal_id) = tab.terminal_id(*pane_id) else {
                    continue;
                };
                let Some(cwd) =
                    tab.cwd_for_pane(*pane_id, &self.state.terminals, &self.terminal_runtimes)
                else {
                    continue;
                };
                let cache_key_hint = self
                    .state
                    .terminals
                    .get(terminal_id)
                    .and_then(|terminal| terminal.repo.as_ref())
                    .filter(|repo| !refresh_repo_discovery && repo.cwd == cwd)
                    .map(|repo| repo.status_cache_key.clone());
                items.push(PaneGitRefreshItem {
                    terminal_id: terminal_id.clone(),
                    cwd,
                    cache_key_hint,
                });
            }
        }
        items
    }
}

fn deduplicate_git_refresh_items(
    items: Vec<PaneGitRefreshItem>,
    cache: &HashMap<PathBuf, GitStatusCacheEntry>,
) -> Vec<CheckoutGitRefreshJob> {
    let mut indexes = HashMap::<PathBuf, usize>::new();
    let mut jobs = Vec::<CheckoutGitRefreshJob>::new();

    for item in items {
        let reconcile = item.cache_key_hint.is_none();
        let cache_key = item.cache_key_hint.unwrap_or_else(|| {
            crate::workspace::git_status_cache_key(&item.cwd).unwrap_or_else(|| item.cwd.clone())
        });
        let target = PaneGitRefreshTarget {
            terminal_id: item.terminal_id,
            cwd: item.cwd,
        };
        if let Some(&index) = indexes.get(&cache_key) {
            jobs[index].cached = jobs[index].cached.take().filter(|_| !reconcile);
            jobs[index].targets.push(target);
            continue;
        }

        let cached = cache.get(&cache_key).filter(|_| !reconcile).cloned();
        indexes.insert(cache_key.clone(), jobs.len());
        jobs.push(CheckoutGitRefreshJob {
            cache_key,
            cached,
            targets: vec![target],
        });
    }

    jobs
}

fn refresh_pane_git_statuses_with_cache_and_demand(
    items: Vec<PaneGitRefreshItem>,
    cache: &HashMap<PathBuf, GitStatusCacheEntry>,
    demand: GitStatusRefreshDemand,
) -> PaneGitRefreshOutput {
    let mut results = Vec::new();
    let mut cache_updates = Vec::new();

    for job in deduplicate_git_refresh_items(items, cache) {
        let (snapshot, cache_entry) = crate::workspace::git_status_snapshot_for_cwd_with_demand(
            &job.cache_key,
            job.cached.as_ref(),
            demand,
        );
        if let Some(cache_entry) = cache_entry {
            cache_updates.push((job.cache_key.clone(), cache_entry));
        }
        results.extend(job.targets.into_iter().map(|target| PaneGitStatus {
            terminal_id: target.terminal_id,
            demand,
            repo: PaneRepo {
                facts: snapshot.clone().for_cwd(&target.cwd),
                cwd: target.cwd,
                status_cache_key: job.cache_key.clone(),
            },
        }));
    }

    PaneGitRefreshOutput {
        results,
        cache_updates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;

    /// Per-pane repo refresh cost at 1 and 15 panes: item collection on the main
    /// thread, and the worker with a warm cache in one checkout or in fifteen.
    #[test]
    #[ignore = "manual pane repo refresh scaling profile"]
    fn pane_git_refresh_scale_profile() {
        const ROUNDS: usize = 200;
        let base = std::env::temp_dir().join(format!("herdr-git-scale-{}", std::process::id()));
        let repos = (0..15)
            .map(|index| {
                let repo = base.join(format!("repo-{index}"));
                std::fs::create_dir_all(&repo).unwrap();
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args(["init", "--quiet"])
                    .output()
                    .unwrap();
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args([
                        "-c",
                        "user.email=h@example.invalid",
                        "-c",
                        "user.name=h",
                        "commit",
                        "--quiet",
                        "--allow-empty",
                        "-m",
                        "initial",
                    ])
                    .output()
                    .unwrap();
                repo
            })
            .collect::<Vec<_>>();
        let median = |mut samples: Vec<std::time::Duration>| {
            samples.sort_unstable();
            samples[samples.len() / 2]
        };

        for count in [1, 15] {
            let mut app = test_app(&crate::config::Config::default());
            for index in 0..count {
                let mut ws = Workspace::test_new("scale");
                ws.test_cwd = repos[0].join(format!("pane-{index}"));
                app.state.workspaces.push(ws);
            }
            app.state.ensure_test_terminals();
            let collect = median(
                (0..ROUNDS)
                    .map(|_| {
                        let started = Instant::now();
                        std::hint::black_box(app.pane_git_refresh_items(false));
                        started.elapsed()
                    })
                    .collect(),
            );
            println!("collect items, {count} panes: median {collect:?}");
        }

        for (label, demand) in [
            (
                "branch+ahead/behind",
                GitStatusRefreshDemand {
                    dirty: false,
                    ..GitStatusRefreshDemand::ALL
                },
            ),
            ("with git_dirty", GitStatusRefreshDemand::ALL),
        ] {
            for (layout, count, distinct) in [
                ("1 pane", 1, 1),
                ("15 panes, one checkout", 15, 1),
                ("15 panes, 15 checkouts", 15, 15),
            ] {
                let items = || {
                    (0..count)
                        .map(|index| item(repos[index % distinct].clone(), None))
                        .collect::<Vec<_>>()
                };
                let mut cache = HashMap::new();
                for (key, entry) in
                    refresh_pane_git_statuses_with_cache_and_demand(items(), &cache, demand)
                        .cache_updates
                {
                    cache.insert(key, entry);
                }
                let warm = median(
                    (0..20)
                        .map(|_| {
                            let hinted = items()
                                .into_iter()
                                .map(|mut item| {
                                    item.cache_key_hint =
                                        Some(std::fs::canonicalize(&item.cwd).unwrap());
                                    item
                                })
                                .collect();
                            let started = Instant::now();
                            std::hint::black_box(refresh_pane_git_statuses_with_cache_and_demand(
                                hinted, &cache, demand,
                            ));
                            started.elapsed()
                        })
                        .collect(),
                );
                println!("worker {label}, {layout}: warm median {warm:?}");
                if demand.dirty {
                    let recount = median(
                        (0..20)
                            .map(|_| {
                                for entry in cache.values_mut() {
                                    entry.dirty_checked_at =
                                        Some(Instant::now() - std::time::Duration::from_secs(60));
                                }
                                let started = Instant::now();
                                std::hint::black_box(
                                    refresh_pane_git_statuses_with_cache_and_demand(
                                        items(),
                                        &cache,
                                        demand,
                                    ),
                                );
                                started.elapsed()
                            })
                            .collect(),
                    );
                    println!("worker {label}, {layout}: dirty recount median {recount:?}");
                }
            }
        }
        let _ = std::fs::remove_dir_all(base);
    }

    fn item(cwd: PathBuf, cache_key_hint: Option<PathBuf>) -> PaneGitRefreshItem {
        PaneGitRefreshItem {
            terminal_id: TerminalId::alloc(),
            cwd,
            cache_key_hint,
        }
    }

    #[test]
    fn panes_in_one_checkout_share_one_refresh() {
        let repo =
            std::env::temp_dir().join(format!("herdr-git-refresh-dedupe-{}", std::process::id()));
        let nested = repo.join("nested");
        let other = repo.join("other");
        std::fs::create_dir_all(&nested).expect("create nested dir");
        std::fs::create_dir_all(&other).expect("create other dir");
        std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .arg("init")
            .output()
            .expect("run git init");
        let items = (0..10)
            .map(|index| {
                item(
                    if index % 2 == 0 {
                        nested.clone()
                    } else {
                        other.clone()
                    },
                    None,
                )
            })
            .collect::<Vec<_>>();
        let terminal_ids = items
            .iter()
            .map(|item| item.terminal_id.clone())
            .collect::<Vec<_>>();

        assert_eq!(
            deduplicate_git_refresh_items(items.clone(), &HashMap::new()).len(),
            1
        );
        let output = refresh_pane_git_statuses_with_cache_and_demand(
            items,
            &HashMap::new(),
            GitStatusRefreshDemand::ALL,
        );

        assert_eq!(output.cache_updates.len(), 1);
        assert_eq!(
            output.cache_updates[0].0,
            std::fs::canonicalize(&repo).expect("canonical repo path")
        );
        assert_eq!(output.results.len(), 10);
        assert_eq!(
            output
                .results
                .iter()
                .map(|result| result.terminal_id.clone())
                .collect::<Vec<_>>(),
            terminal_ids
        );
        assert_eq!(output.results[0].repo.cwd, nested);
        assert_eq!(output.results[1].repo.cwd, other);
        assert_eq!(output.results[0].repo.facts.dirty, Some(0));

        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn shared_root_repo_refresh_keeps_pane_specific_fallback_labels() {
        let cache_key = PathBuf::from("/");
        let cached = GitStatusCacheEntry {
            fingerprint: None,
            retry_after: Some(Instant::now() + std::time::Duration::from_secs(30)),
            dirty_checked_at: None,
            snapshot: crate::workspace::GitStatusSnapshot {
                auto_label: "/".into(),
                branch: Some("main".into()),
                ahead_behind: None,
                dirty: None,
                space: Some(crate::workspace::GitSpaceMetadata {
                    key: "/.git".into(),
                    checkout_key: "/".into(),
                    repo_name: "repo".into(),
                    repo_root: cache_key.clone(),
                    is_linked_worktree: false,
                }),
            },
        };
        let items = ["alpha", "beta"]
            .into_iter()
            .map(|name| item(cache_key.join(name), Some(cache_key.clone())))
            .collect();

        let output = refresh_pane_git_statuses_with_cache_and_demand(
            items,
            &HashMap::from([(cache_key, cached)]),
            GitStatusRefreshDemand::ALL,
        );

        assert_eq!(output.cache_updates.len(), 1);
        assert_eq!(output.results.len(), 2);
        assert_eq!(output.results[0].repo.facts.auto_label, "alpha");
        assert_eq!(output.results[1].repo.facts.auto_label, "beta");
        assert_eq!(output.results[0].repo.facts.branch.as_deref(), Some("main"));
        assert_eq!(output.results[1].repo.facts.branch.as_deref(), Some("main"));
    }

    fn app_with_pane_at(cwd: &std::path::Path) -> super::super::App {
        let mut app = test_app(&crate::config::Config::default());
        let mut ws = Workspace::test_new("test");
        ws.test_cwd = cwd.to_path_buf();
        app.state.workspaces.push(ws);
        app.state.ensure_test_terminals();
        app
    }

    #[test]
    fn pane_refresh_items_cover_every_pane_without_discovering_git() {
        let cwd = std::env::temp_dir().join(format!("herdr-uncached-cwd-{}", std::process::id()));
        let mut app = app_with_pane_at(&cwd);
        let mut second = Workspace::test_new("second");
        second.test_cwd = cwd.join("deeper");
        app.state.workspaces.push(second);
        app.state.ensure_test_terminals();

        let items = app.pane_git_refresh_items(false);

        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|item| item.cwd == cwd));
        assert!(items.iter().any(|item| item.cwd == cwd.join("deeper")));
        assert!(items.iter().all(|item| item.cache_key_hint.is_none()));
    }

    fn cache_repo_at(app: &mut super::super::App, cwd: &std::path::Path, key: &std::path::Path) {
        let terminal = app.state.terminals.values_mut().next().unwrap();
        terminal.repo = Some(PaneRepo {
            cwd: cwd.to_path_buf(),
            status_cache_key: key.to_path_buf(),
            facts: crate::workspace::GitStatusSnapshot {
                auto_label: "stale".into(),
                branch: None,
                ahead_behind: None,
                dirty: None,
                space: None,
            },
        });
    }

    #[test]
    fn pane_refresh_items_reuse_the_cached_key_while_cwd_stays() {
        let cwd = PathBuf::from("/repo/deep/nested");
        let cache_key = PathBuf::from("/repo");
        let mut app = app_with_pane_at(&cwd);
        cache_repo_at(&mut app, &cwd, &cache_key);

        let items = app.pane_git_refresh_items(false);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].cache_key_hint, Some(cache_key.clone()));

        app.state.terminals.values_mut().next().unwrap().cwd = PathBuf::from("/elsewhere");
        assert_eq!(app.pane_git_refresh_items(false)[0].cache_key_hint, None);
    }

    #[test]
    fn periodic_repo_discovery_ignores_cached_key_hints() {
        let cwd = PathBuf::from("/repo/deep/nested");
        let mut app = app_with_pane_at(&cwd);
        cache_repo_at(&mut app, &cwd, std::path::Path::new("/repo"));

        let items = app.pane_git_refresh_items(true);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].cache_key_hint, None);
        let cache_key = items[0].cwd.clone();
        let cached = GitStatusCacheEntry {
            fingerprint: None,
            retry_after: None,
            dirty_checked_at: None,
            snapshot: crate::workspace::GitStatusSnapshot {
                auto_label: "stale".into(),
                branch: None,
                ahead_behind: None,
                dirty: None,
                space: None,
            },
        };
        let jobs = deduplicate_git_refresh_items(items, &HashMap::from([(cache_key, cached)]));
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].cached, None);
    }

    #[test]
    fn failed_git_refresh_spawn_retries_after_interval() {
        let mut config = crate::config::Config::default();
        config.ui.sidebar.spaces.rows = vec![vec![crate::config::SpaceSidebarToken::Workspace]];
        let mut app = test_app(&config);
        app.state.workspaces.push(Workspace::test_new("test"));
        app.state.ensure_test_terminals();
        let now = Instant::now();
        app.request_git_identity_refresh(now);

        crate::thread_spawn::test_hook::fail_next_spawns(1);
        app.start_git_status_refresh_if_due(now);

        assert!(!app.git_refresh_in_flight);
        assert!(app.git_identity_refresh_requested);
        assert_eq!(
            app.git_refresh_deadline(),
            Some(now + GIT_REMOTE_STATUS_REFRESH_INTERVAL)
        );

        let retry_at = now + GIT_REMOTE_STATUS_REFRESH_INTERVAL;
        app.start_git_status_refresh_if_due(retry_at);
        assert!(app.git_refresh_in_flight);
        assert!(!app.git_identity_refresh_requested);
    }

    #[test]
    fn cwd_identity_refresh_runs_without_sidebar_git_tokens() {
        let mut config = crate::config::Config::default();
        config.ui.sidebar.spaces.rows = vec![vec![crate::config::SpaceSidebarToken::Workspace]];
        let mut app = test_app(&config);
        app.state.workspaces.push(Workspace::test_new("test"));
        app.state.ensure_test_terminals();
        let now = Instant::now();

        app.request_git_identity_refresh(now);

        assert!(app.git_refresh_deadline().is_some());
        app.start_git_status_refresh_if_due(now);
        assert!(app.git_refresh_in_flight);
        assert!(!app.git_identity_refresh_requested);
    }

    #[tokio::test]
    async fn restored_session_refreshes_pane_repo_facts_without_sidebar_git_tokens() {
        let repo =
            std::env::temp_dir().join(format!("herdr-restored-git-refresh-{}", std::process::id()));
        let nested = repo.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(repo.join(".git/objects")).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        let mut config = crate::config::Config::default();
        config.ui.sidebar.spaces.rows = vec![vec![crate::config::SpaceSidebarToken::Workspace]];
        let mut app = test_app(&config);
        let mut workspace = Workspace::test_new("restored");
        workspace.test_cwd = nested.clone();
        app.state.workspaces.push(workspace);
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        for terminal in app.state.terminals.values_mut() {
            terminal.cwd = nested.clone();
        }
        app.state.assert_invariants_for_test();

        app.refresh_restored_workspace_git_metadata();
        assert!(app.git_refresh_in_flight);
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while app.git_refresh_in_flight {
                let event = app.event_rx.recv().await.expect("Git worker completion");
                app.handle_internal_event(event);
            }
        })
        .await
        .expect("restored Git metadata refresh completes");

        let repo_facts = app
            .state
            .terminals
            .values()
            .next()
            .and_then(|terminal| terminal.repo.as_ref())
            .expect("pane repo facts");
        assert_eq!(repo_facts.facts.branch.as_deref(), Some("main"));
        assert_eq!(
            repo_facts
                .facts
                .space
                .as_ref()
                .map(|space| space.repo_name.as_str()),
            repo.file_name().and_then(|name| name.to_str())
        );
        app.state.assert_invariants_for_test();
        std::fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn due_git_refresh_starts_without_sidebar_consumer() {
        let mut config = crate::config::Config::default();
        config.ui.sidebar.spaces.rows = vec![vec![crate::config::SpaceSidebarToken::Workspace]];
        config.ui.sidebar.agents.rows = vec![vec![crate::config::AgentSidebarToken::Agent]];
        let mut app = test_app(&config);
        app.state.workspaces.push(Workspace::test_new("test"));
        app.state.ensure_test_terminals();
        let now = Instant::now();
        app.last_git_remote_status_refresh = now - GIT_REMOTE_STATUS_REFRESH_INTERVAL;

        app.start_git_status_refresh_if_due(now);

        assert!(app.git_refresh_in_flight);
    }

    #[test]
    fn dirty_counts_are_demanded_only_by_an_agent_row_showing_git_dirty() {
        let always = GitStatusRefreshDemand {
            branch: true,
            ahead_behind: true,
            dirty: false,
        };
        let with_dirty = GitStatusRefreshDemand {
            dirty: true,
            ..always
        };
        let cases: [(&str, GitStatusRefreshDemand); 4] = [
            ("rows = [[\"agent\"]]", always),
            ("rows = [[\"branch\", \"git_status\"]]", always),
            (
                "rows = [[{ token = \"git_dirty\", fg = \"#fff\" }]]",
                with_dirty,
            ),
            (
                "rows = [[\"agent\"]]\n[rows_by_agent]\nclaude = [[\"git_dirty\"]]",
                with_dirty,
            ),
        ];

        for (agents, expected) in cases {
            let mut config = crate::config::Config::default();
            config.ui.sidebar.agents = toml::from_str(agents).unwrap();
            let mut app = test_app(&config);
            app.state.workspaces.push(Workspace::test_new("test"));

            assert_eq!(app.git_refresh_demand(), expected, "agents: {agents}");
            assert!(app.git_refresh_deadline().is_some(), "agents: {agents}");
        }
    }

    #[test]
    fn headless_deadline_can_suppress_git_refresh_timer() {
        let mut app = test_app(&crate::config::Config::default());
        app.state.workspaces.push(Workspace::test_new("test"));
        let now = Instant::now();
        app.last_git_remote_status_refresh = now - GIT_REMOTE_STATUS_REFRESH_INTERVAL;

        assert_eq!(
            app.next_headless_loop_deadline_with_git_refresh(now, false, false),
            None
        );
        assert_eq!(
            app.next_headless_loop_deadline_with_git_refresh(now, false, true),
            Some(now)
        );
    }

    #[test]
    fn explicit_git_refresh_invalidates_cached_non_git_results() {
        let mut app = test_app(&crate::config::Config::default());
        let cwd = std::env::temp_dir().join(format!("herdr-git-miss-{}", std::process::id()));
        std::fs::create_dir_all(&cwd).unwrap();
        let (_, entry) = crate::workspace::git_status_snapshot_for_cwd_with_demand(
            &cwd,
            None,
            GitStatusRefreshDemand::ALL,
        );
        app.git_status_cache
            .insert(cwd.clone(), entry.expect("non-Git cache entry"));

        app.mark_git_status_refresh_due(Instant::now());

        assert!(app.git_status_cache.is_empty());
        std::fs::remove_dir_all(cwd).unwrap();
    }

    #[test]
    fn git_refresh_due_request_survives_in_flight_refresh() {
        let mut app = test_app(&crate::config::Config::default());
        let now = Instant::now();
        app.git_refresh_in_flight = true;

        app.mark_git_status_refresh_due(now);
        assert!(app.git_refresh_due_after_in_flight);

        app.handle_internal_event(AppEvent::GitStatusRefreshed {
            results: Vec::new(),
            cache_updates: Vec::new(),
        });

        assert!(!app.git_refresh_in_flight);
        assert!(!app.git_refresh_due_after_in_flight);
        assert_eq!(app.git_refresh_deadline(), None);

        app.state.workspaces.push(Workspace::test_new("test"));
        let deadline = app
            .git_refresh_deadline()
            .expect("refresh should be due once a workspace exists");
        assert!(deadline <= Instant::now());
    }

    fn test_app(config: &crate::config::Config) -> super::super::App {
        super::super::App::new(
            config,
            crate::app::AppPolicy::TEST,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        )
    }
}
