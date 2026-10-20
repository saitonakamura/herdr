use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::workspace::GitSpaceMetadata;

use super::{
    config::{deps_current, read_config, stamp, upstream_full_ref, ConfigCtx, FileDep},
    discovery::{
        automatic_workspace_label, canonicalize_best_effort_path, fallback_label_from_cwd,
        git_ref_storage_is_reftable, git_rev_parse_verify, git_space_metadata_from_info,
        git_symbolic_head_full, git_worktree_info, read_git_ref_file, read_ref_oid,
        GitWorktreeInfo,
    },
};

/// How often a checkout's working tree is rescanned while its HEAD stays put.
const DIRTY_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GitStatusRefreshDemand {
    pub branch: bool,
    pub ahead_behind: bool,
    /// Count changed paths with `git status`; the only fact that spawns a
    /// process on every refresh, so only while something displays it.
    pub dirty: bool,
}

impl GitStatusRefreshDemand {
    #[cfg(test)]
    pub const ALL: Self = Self {
        branch: true,
        ahead_behind: true,
        dirty: true,
    };

    pub fn is_empty(self) -> bool {
        !self.branch && !self.ahead_behind && !self.dirty
    }
}

/// Repo facts for one checkout, computed off the main thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusSnapshot {
    pub auto_label: String,
    pub branch: Option<String>,
    pub ahead_behind: Option<(usize, usize)>,
    /// Changed paths (staged, unstaged and untracked); `None` when not demanded.
    pub dirty: Option<usize>,
    pub space: Option<GitSpaceMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusCacheEntry {
    pub fingerprint: Option<GitStatusFingerprint>,
    pub retry_after: Option<Instant>,
    /// When `snapshot.dirty` was last counted.
    pub dirty_checked_at: Option<Instant>,
    pub snapshot: GitStatusSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusFingerprint {
    pub head: GitHeadIdentity,
    pub upstream: Option<GitUpstreamIdentity>,
    repository_context: RepoContext,
}

type RepoContext = (GitWorktreeInfo, bool, Vec<FileDep>, Option<ConfigCtx>);

fn repo_context(cwd: &Path) -> Option<RepoContext> {
    let info = git_worktree_info(cwd)?;
    let reftable = git_ref_storage_is_reftable(&info.git_common_dir);
    let mut paths = vec![info.repo_root.join(".git"), info.git_dir.join("commondir")];
    paths.push(info.git_dir.join("HEAD"));
    paths.push(info.git_common_dir.join("config"));
    paths.extend((info.git_dir != info.git_common_dir).then(|| info.git_dir.join("config")));
    let mut deps: Vec<_> = paths.into_iter().map(|path| stamp(path, None)).collect();
    deps[0].2 &= git_worktree_info(cwd).as_ref() == Some(&info)
        && git_ref_storage_is_reftable(&info.git_common_dir) == reftable
        && deps_current(&deps);
    Some((info, reftable, deps, None))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHeadIdentity {
    Branch {
        full_ref: String,
        short_name: String,
        oid: Option<String>,
    },
    Detached {
        oid: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitUpstreamIdentity {
    pub remote: String,
    pub merge_ref: String,
    pub full_ref: String,
    pub oid: Option<String>,
}

pub fn git_status_cache_key(cwd: &Path) -> Option<PathBuf> {
    git_worktree_info(cwd).map(|info| canonicalize_best_effort_path(&info.repo_root))
}

#[cfg(test)]
pub fn git_status_snapshot_for_cwd(
    cwd: &Path,
    cached: Option<&GitStatusCacheEntry>,
) -> (GitStatusSnapshot, Option<GitStatusCacheEntry>) {
    git_status_snapshot_for_cwd_with_demand(cwd, cached, GitStatusRefreshDemand::ALL)
}

pub fn git_status_snapshot_for_cwd_with_demand(
    cwd: &Path,
    cached: Option<&GitStatusCacheEntry>,
    demand: GitStatusRefreshDemand,
) -> (GitStatusSnapshot, Option<GitStatusCacheEntry>) {
    let (snapshot, entry) = git_status_snapshot_without_dirty(cwd, cached, demand);
    if !demand.dirty {
        return (snapshot, entry);
    }
    with_dirty_count(cwd, cached, snapshot, entry)
}

/// Adds the changed-path count, reusing the cached one while HEAD and upstream
/// are unchanged and the count is younger than `DIRTY_REFRESH_INTERVAL`.
fn with_dirty_count(
    cwd: &Path,
    cached: Option<&GitStatusCacheEntry>,
    mut snapshot: GitStatusSnapshot,
    entry: Option<GitStatusCacheEntry>,
) -> (GitStatusSnapshot, Option<GitStatusCacheEntry>) {
    let Some(mut entry) = entry else {
        return (snapshot, None);
    };
    if entry.fingerprint.is_none() {
        return (snapshot, Some(entry));
    }
    let now = Instant::now();
    let reusable = cached.filter(|cached| {
        cached.fingerprint == entry.fingerprint
            && cached
                .dirty_checked_at
                .is_some_and(|at| now.saturating_duration_since(at) < DIRTY_REFRESH_INTERVAL)
    });
    if let Some(cached) = reusable {
        snapshot.dirty = cached.snapshot.dirty;
        entry.dirty_checked_at = cached.dirty_checked_at;
    } else {
        snapshot.dirty = git_dirty_count(cwd);
        entry.dirty_checked_at = Some(now);
    }
    entry.snapshot = snapshot.clone();
    (snapshot, Some(entry))
}

fn git_dirty_count(cwd: &Path) -> Option<usize> {
    // --no-optional-locks: never take index.lock under an agent's own git commands.
    let output = crate::noninteractive_process::command("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(cwd)
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .count(),
    )
}

fn git_status_snapshot_without_dirty(
    cwd: &Path,
    cached: Option<&GitStatusCacheEntry>,
    demand: GitStatusRefreshDemand,
) -> (GitStatusSnapshot, Option<GitStatusCacheEntry>) {
    if let Some(cached) = cached.filter(|entry| {
        entry.fingerprint.is_none()
            && entry
                .retry_after
                .is_some_and(|retry_after| retry_after > Instant::now())
    }) {
        return (cached.snapshot.clone(), Some(cached.clone()));
    }

    let repository_context = cached
        .and_then(|entry| entry.fingerprint.as_ref())
        .map(|fingerprint| fingerprint.repository_context.clone())
        .filter(|context| deps_current(&context.2))
        .or_else(|| repo_context(cwd));
    let Some(repository_context) = repository_context else {
        let snapshot = GitStatusSnapshot {
            auto_label: fallback_label_from_cwd(cwd),
            branch: None,
            ahead_behind: None,
            dirty: None,
            space: None,
        };
        return (
            snapshot.clone(),
            Some(GitStatusCacheEntry {
                fingerprint: None,
                retry_after: Some(Instant::now() + Duration::from_secs(30)),
                dirty_checked_at: None,
                snapshot,
            }),
        );
    };
    let auto_label = automatic_workspace_label(cwd, &repository_context.0.repo_root);
    let space = git_space_metadata_from_info(&repository_context.0);

    if !demand.ahead_behind {
        let fingerprint = fingerprint(repository_context, false);
        let branch = demand
            .branch
            .then(|| fingerprint.as_ref()?.branch_name())
            .flatten()
            .map(str::to_string);
        let snapshot = GitStatusSnapshot {
            auto_label,
            branch,
            ahead_behind: None,
            dirty: None,
            space: Some(space),
        };
        return (
            snapshot.clone(),
            fingerprint.map(|fingerprint| GitStatusCacheEntry {
                fingerprint: Some(fingerprint),
                retry_after: None,
                dirty_checked_at: None,
                snapshot,
            }),
        );
    }

    let Some(fingerprint) = fingerprint(repository_context, true) else {
        return (
            GitStatusSnapshot {
                auto_label,
                branch: None,
                ahead_behind: None,
                dirty: None,
                space: Some(space),
            },
            None,
        );
    };
    let branch = fingerprint.branch_name().map(str::to_string);

    if let Some(cached) = cached.filter(|entry| entry.fingerprint.as_ref() == Some(&fingerprint)) {
        let snapshot = GitStatusSnapshot {
            auto_label,
            branch,
            ahead_behind: cached.snapshot.ahead_behind,
            dirty: None,
            space: Some(space),
        };
        return (
            snapshot.clone(),
            Some(GitStatusCacheEntry {
                fingerprint: Some(fingerprint),
                retry_after: None,
                dirty_checked_at: None,
                snapshot,
            }),
        );
    }

    let ahead_behind = fingerprint
        .head_oid()
        .zip(fingerprint.upstream_oid())
        .and_then(|(head_oid, upstream_oid)| git_ahead_behind_between(cwd, head_oid, upstream_oid));
    let snapshot = GitStatusSnapshot {
        auto_label,
        branch,
        ahead_behind,
        dirty: None,
        space: Some(space),
    };
    (
        snapshot.clone(),
        Some(GitStatusCacheEntry {
            fingerprint: Some(fingerprint),
            retry_after: None,
            dirty_checked_at: None,
            snapshot,
        }),
    )
}

#[cfg(test)]
pub(super) fn git_status_fingerprint(cwd: &Path) -> Option<GitStatusFingerprint> {
    fingerprint(repo_context(cwd)?, true)
}

fn fingerprint(mut repo: RepoContext, include_upstream: bool) -> Option<GitStatusFingerprint> {
    let head = read_head_identity(&repo.0, repo.1)?;
    let upstream = match &head {
        GitHeadIdentity::Branch { short_name, .. } if include_upstream => {
            read_upstream(&mut repo, short_name)
        }
        _ => None,
    };

    Some(GitStatusFingerprint {
        head,
        upstream,
        repository_context: repo,
    })
}

impl GitStatusFingerprint {
    fn branch_name(&self) -> Option<&str> {
        match &self.head {
            GitHeadIdentity::Branch { short_name, .. } => Some(short_name.as_str()),
            GitHeadIdentity::Detached { .. } => None,
        }
    }

    fn head_oid(&self) -> Option<&str> {
        match &self.head {
            GitHeadIdentity::Branch { oid, .. } => oid.as_deref(),
            GitHeadIdentity::Detached { oid } => Some(oid.as_str()),
        }
    }

    fn upstream_oid(&self) -> Option<&str> {
        self.upstream
            .as_ref()
            .and_then(|upstream| upstream.oid.as_deref())
    }
}

fn read_head_identity(info: &GitWorktreeInfo, reftable: bool) -> Option<GitHeadIdentity> {
    if reftable {
        return read_head_identity_from_git(info);
    }

    read_head_identity_from_files(info)
}

fn read_head_identity_from_git(info: &GitWorktreeInfo) -> Option<GitHeadIdentity> {
    if let Some(full_ref) = git_symbolic_head_full(&info.repo_root) {
        let short_name = full_ref.strip_prefix("refs/heads/")?.to_string();
        let oid = git_rev_parse_verify(&info.repo_root, &full_ref);
        return Some(GitHeadIdentity::Branch {
            full_ref,
            short_name,
            oid,
        });
    }

    git_rev_parse_verify(&info.repo_root, "HEAD").map(|oid| GitHeadIdentity::Detached { oid })
}

fn read_head_identity_from_files(info: &GitWorktreeInfo) -> Option<GitHeadIdentity> {
    let head = read_git_ref_file(&info.git_dir.join("HEAD"))?;
    let head = head.trim();
    if let Some(full_ref) = head.strip_prefix("ref: ") {
        let short_name = full_ref.strip_prefix("refs/heads/")?.to_string();
        let oid = read_ref_oid(&info.git_common_dir, full_ref);
        return Some(GitHeadIdentity::Branch {
            full_ref: full_ref.to_string(),
            short_name,
            oid,
        });
    }

    (!head.is_empty()).then(|| GitHeadIdentity::Detached {
        oid: head.to_string(),
    })
}

fn read_upstream(repo: &mut RepoContext, branch: &str) -> Option<GitUpstreamIdentity> {
    if repo
        .3
        .as_ref()
        .is_none_or(|context| context.0 != branch || !deps_current(&context.2))
    {
        repo.3 = Some(read_config(&repo.0, branch));
    }
    let config = repo.3.as_ref()?.1.clone()?;
    let full_ref = upstream_full_ref(&config)?;
    let oid = if repo.1 {
        git_rev_parse_verify(&repo.0.repo_root, &full_ref)
    } else {
        read_ref_oid(&repo.0.git_common_dir, &full_ref)
    };
    Some(GitUpstreamIdentity {
        remote: config.remote,
        merge_ref: config.merge_ref,
        full_ref,
        oid,
    })
}

fn git_ahead_behind_between(
    cwd: &Path,
    head_oid: &str,
    upstream_oid: &str,
) -> Option<(usize, usize)> {
    let range = format!("{head_oid}...{upstream_oid}");
    let output = crate::noninteractive_process::command("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-list", "--left-right", "--count", &range])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    parse_git_ahead_behind_output(&stdout)
}

fn parse_git_ahead_behind_output(stdout: &str) -> Option<(usize, usize)> {
    let mut parts = stdout.split_whitespace();
    let ahead = parts.next()?.parse().ok()?;
    let behind = parts.next()?.parse().ok()?;
    Some((ahead, behind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::git::{
        git_space_metadata,
        test_support::{init_repo_with_commit, run_git, temp_test_dir, write_fake_tracked_repo},
    };

    const BRANCH_AND_DIRTY: GitStatusRefreshDemand = GitStatusRefreshDemand {
        branch: true,
        ahead_behind: false,
        dirty: true,
    };

    #[test]
    fn dirty_count_is_taken_only_when_demanded() {
        let root = temp_test_dir("dirty-demand");
        init_repo_with_commit(&root);
        std::fs::write(root.join("untracked.txt"), "x").unwrap();
        std::fs::write(root.join("other.txt"), "y").unwrap();

        let (without, entry) = git_status_snapshot_for_cwd_with_demand(
            &root,
            None,
            GitStatusRefreshDemand {
                dirty: false,
                ..BRANCH_AND_DIRTY
            },
        );
        assert_eq!(without.dirty, None);
        assert_eq!(entry.unwrap().dirty_checked_at, None);

        let (with, entry) = git_status_snapshot_for_cwd_with_demand(&root, None, BRANCH_AND_DIRTY);
        assert_eq!(with.dirty, Some(2));
        assert!(entry.unwrap().dirty_checked_at.is_some());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dirty_count_is_reused_until_it_ages_or_head_moves() {
        let root = temp_test_dir("dirty-reuse");
        init_repo_with_commit(&root);
        let (_, entry) = git_status_snapshot_for_cwd_with_demand(&root, None, BRANCH_AND_DIRTY);
        let mut entry = entry.unwrap();
        assert_eq!(entry.snapshot.dirty, Some(0));

        std::fs::write(root.join("new.txt"), "x").unwrap();
        let (reused, _) =
            git_status_snapshot_for_cwd_with_demand(&root, Some(&entry), BRANCH_AND_DIRTY);
        assert_eq!(reused.dirty, Some(0), "a fresh count is not rescanned");

        entry.dirty_checked_at = Some(Instant::now() - DIRTY_REFRESH_INTERVAL);
        let (aged, _) =
            git_status_snapshot_for_cwd_with_demand(&root, Some(&entry), BRANCH_AND_DIRTY);
        assert_eq!(aged.dirty, Some(1));

        let (_, entry) = git_status_snapshot_for_cwd_with_demand(&root, None, BRANCH_AND_DIRTY);
        let entry = entry.unwrap();
        run_git(&root, &["add", "new.txt"]);
        run_git(&root, &["commit", "--quiet", "-m", "add"]);
        let (moved, _) =
            git_status_snapshot_for_cwd_with_demand(&root, Some(&entry), BRANCH_AND_DIRTY);
        assert_eq!(moved.dirty, Some(0), "a moved HEAD recounts at once");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn git_status_cache_key_ignores_invalid_git_marker() {
        let base = temp_test_dir("invalid-git-root");
        let cwd = base.join("workspace");
        std::fs::create_dir_all(base.join(".git")).unwrap();
        std::fs::create_dir_all(&cwd).unwrap();

        assert_eq!(git_status_cache_key(&cwd), None);

        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn non_git_refresh_reuses_cached_miss_without_rechecking_filesystem() {
        let root = temp_test_dir("cached-miss");
        let cwd = root.join("deep/nested");
        std::fs::create_dir_all(&cwd).unwrap();

        let (initial, cache_entry) = git_status_snapshot_for_cwd(&cwd, None);
        let cache_entry = cache_entry.expect("non-Git result should be cached");
        std::fs::remove_dir_all(&root).unwrap();

        let (cached, update) = git_status_snapshot_for_cwd(&cwd, Some(&cache_entry));

        assert_eq!(cached, initial);
        assert_eq!(update, Some(cache_entry));
    }

    #[test]
    fn expired_non_git_cache_detects_repository_created_in_place() {
        let root = temp_test_dir("expired-miss");
        let (_, cache_entry) = git_status_snapshot_for_cwd(&root, None);
        let mut cache_entry = cache_entry.expect("non-Git result should be cached");
        cache_entry.retry_after = Some(Instant::now() - Duration::from_secs(1));
        write_fake_tracked_repo(&root);

        let (snapshot, update) = git_status_snapshot_for_cwd(&root, Some(&cache_entry));

        assert_eq!(snapshot.branch.as_deref(), Some("main"));
        assert!(update.is_some_and(|entry| entry.fingerprint.is_some()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cached_repo_identity_clears_when_head_disappears() {
        let root = temp_test_dir("missing-head");
        write_fake_tracked_repo(&root);
        let (_, cached) = git_status_snapshot_for_cwd(&root, None);
        std::fs::remove_file(root.join(".git/HEAD")).unwrap();

        let (snapshot, _) = git_status_snapshot_for_cwd(&root, cached.as_ref());

        assert_eq!(snapshot.space, None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn branch_only_refresh_skips_ahead_behind_cache_work() {
        let root = temp_test_dir("branch-only");
        write_fake_tracked_repo(&root);

        let (snapshot, update) = git_status_snapshot_for_cwd_with_demand(
            &root,
            None,
            GitStatusRefreshDemand {
                branch: true,
                ahead_behind: false,
                dirty: false,
            },
        );

        assert_eq!(snapshot.branch.as_deref(), Some("main"));
        assert_eq!(snapshot.ahead_behind, None);
        assert!(update.is_some_and(|entry| entry.fingerprint.is_some()));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn git_status_reuses_cached_ahead_behind_when_fingerprint_matches() {
        let root = temp_test_dir("cache-hit");
        write_fake_tracked_repo(&root);
        let fingerprint = git_status_fingerprint(&root).unwrap();
        let cached = GitStatusCacheEntry {
            fingerprint: Some(fingerprint),
            retry_after: None,
            dirty_checked_at: None,
            snapshot: GitStatusSnapshot {
                auto_label: "repo".into(),
                branch: Some("main".into()),
                ahead_behind: Some((2, 1)),
                dirty: None,
                space: git_space_metadata(&root),
            },
        };

        let (snapshot, update) = git_status_snapshot_for_cwd(&root, Some(&cached));

        assert_eq!(snapshot.branch.as_deref(), Some("main"));
        assert_eq!(snapshot.ahead_behind, Some((2, 1)));
        assert_eq!(update.unwrap().snapshot.ahead_behind, Some((2, 1)));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn git_status_does_not_reuse_cache_when_branch_changes_at_same_oid() {
        let root = temp_test_dir("branch-switch");
        write_fake_tracked_repo(&root);
        let fingerprint = git_status_fingerprint(&root).unwrap();
        let cached = GitStatusCacheEntry {
            fingerprint: Some(fingerprint),
            retry_after: None,
            dirty_checked_at: None,
            snapshot: GitStatusSnapshot {
                auto_label: "repo".into(),
                branch: Some("main".into()),
                ahead_behind: Some((4, 0)),
                dirty: None,
                space: git_space_metadata(&root),
            },
        };
        std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/feature\n").unwrap();
        std::fs::write(
            root.join(".git/refs/heads/feature"),
            "1111111111111111111111111111111111111111\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".git/config"),
            "[branch \"feature\"]\n\tremote = origin\n\tmerge = refs/heads/main\n",
        )
        .unwrap();

        let (snapshot, _) = git_status_snapshot_for_cwd(&root, Some(&cached));

        assert_eq!(snapshot.branch.as_deref(), Some("feature"));
        assert_eq!(snapshot.ahead_behind, None);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn git_status_clears_ahead_behind_when_upstream_is_unset() {
        let root = temp_test_dir("upstream-unset");
        write_fake_tracked_repo(&root);
        let fingerprint = git_status_fingerprint(&root).unwrap();
        let cached = GitStatusCacheEntry {
            fingerprint: Some(fingerprint),
            retry_after: None,
            dirty_checked_at: None,
            snapshot: GitStatusSnapshot {
                auto_label: "repo".into(),
                branch: Some("main".into()),
                ahead_behind: Some((0, 3)),
                dirty: None,
                space: git_space_metadata(&root),
            },
        };
        std::fs::write(root.join(".git/config"), "").unwrap();

        let (snapshot, _) = git_status_snapshot_for_cwd(&root, Some(&cached));

        assert_eq!(snapshot.branch.as_deref(), Some("main"));
        assert_eq!(snapshot.ahead_behind, None);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn git_status_rebuilds_config_when_missing_include_appears() {
        let root = temp_test_dir("include-appears");
        write_fake_tracked_repo(&root);
        std::fs::write(
            root.join(".git/config"),
            "[branch \"main\"]\n\tremote = origin\n\tmerge = refs/heads/main\n[include]\n\tpath = branch.cfg\n",
        )
        .unwrap();
        let (_, cached) = git_status_snapshot_for_cwd(&root, None);
        std::fs::write(
            root.join(".git/branch.cfg"),
            "[branch \"main\"]\n\tremote = fork\n\tmerge = refs/heads/main\n",
        )
        .unwrap();

        let (_, updated) = git_status_snapshot_for_cwd(&root, cached.as_ref());

        let upstream = updated.unwrap().fingerprint.unwrap().upstream.unwrap();
        assert_eq!(upstream.remote, "fork");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn git_status_fingerprint_reads_packed_refs() {
        let root = temp_test_dir("packed-refs");
        write_fake_tracked_repo(&root);
        std::fs::remove_file(root.join(".git/refs/remotes/origin/main")).unwrap();
        std::fs::write(
            root.join(".git/packed-refs"),
            "# pack-refs with: peeled fully-peeled sorted\n2222222222222222222222222222222222222222 refs/remotes/origin/main\n",
        )
        .unwrap();

        let fingerprint = git_status_fingerprint(&root).unwrap();

        assert_eq!(
            fingerprint.upstream.unwrap().oid.as_deref(),
            Some("2222222222222222222222222222222222222222")
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn linked_worktree_refresh_keeps_checkout_name_as_auto_label() {
        let (base, _, checkout) =
            crate::workspace::git::test_support::create_repo_with_linked_worktree(
                "linked-refresh-label",
            );

        let (snapshot, _) = git_status_snapshot_for_cwd(&checkout, None);

        assert_eq!(
            snapshot.auto_label,
            checkout.file_name().unwrap().to_str().unwrap()
        );

        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn git_status_cache_key_is_per_linked_worktree_checkout() {
        let base = temp_test_dir("linked-worktree-keys");
        let common_dir = base.join("repo/.git");
        let worktree_one = base.join("one");
        let worktree_two = base.join("two");
        let git_dir_one = common_dir.join("worktrees/one");
        let git_dir_two = common_dir.join("worktrees/two");
        std::fs::create_dir_all(&git_dir_one).unwrap();
        std::fs::create_dir_all(&git_dir_two).unwrap();
        std::fs::create_dir_all(&worktree_one).unwrap();
        std::fs::create_dir_all(&worktree_two).unwrap();
        std::fs::write(
            worktree_one.join(".git"),
            format!("gitdir: {}\n", git_dir_one.display()),
        )
        .unwrap();
        std::fs::write(
            worktree_two.join(".git"),
            format!("gitdir: {}\n", git_dir_two.display()),
        )
        .unwrap();
        std::fs::write(git_dir_one.join("HEAD"), "ref: refs/heads/one\n").unwrap();
        std::fs::write(git_dir_two.join("HEAD"), "ref: refs/heads/two\n").unwrap();

        assert_ne!(
            git_status_cache_key(&worktree_one),
            git_status_cache_key(&worktree_two)
        );

        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn git_status_fingerprint_reads_reftable_branch_identity() {
        let root = temp_test_dir("reftable-fingerprint");
        let root_arg = root.to_string_lossy().to_string();
        let output = std::process::Command::new("git")
            .args(["init", "--ref-format=reftable", "-b", "main", &root_arg])
            .output()
            .unwrap();
        if !output.status.success() {
            std::fs::remove_dir_all(root).unwrap();
            return;
        }
        run_git(&root, &["config", "user.email", "herdr@example.invalid"]);
        run_git(&root, &["config", "user.name", "Herdr Test"]);
        run_git(&root, &["commit", "--allow-empty", "-m", "initial"]);

        let fingerprint = git_status_fingerprint(&root).unwrap();

        assert_eq!(
            fingerprint.head,
            GitHeadIdentity::Branch {
                full_ref: "refs/heads/main".into(),
                short_name: "main".into(),
                oid: git_rev_parse_verify(&root, "HEAD"),
            }
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn git_status_recomputes_ahead_behind_when_head_moves() {
        let base = temp_test_dir("head-moves");
        let remote = base.join("remote.git");
        let repo = base.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let remote_arg = remote.to_string_lossy().to_string();
        run_git(&base, &["init", "--bare", &remote_arg]);
        run_git(&repo, &["init"]);
        run_git(&repo, &["config", "user.email", "herdr@example.invalid"]);
        run_git(&repo, &["config", "user.name", "Herdr Test"]);
        run_git(&repo, &["commit", "--allow-empty", "-m", "initial"]);
        run_git(&repo, &["branch", "-M", "main"]);
        run_git(&repo, &["remote", "add", "origin", &remote_arg]);
        run_git(&repo, &["push", "-u", "origin", "main"]);

        let (initial, cache_entry) = git_status_snapshot_for_cwd(&repo, None);
        assert_eq!(initial.ahead_behind, Some((0, 0)));
        run_git(&repo, &["commit", "--allow-empty", "-m", "ahead"]);

        let (updated, _) = git_status_snapshot_for_cwd(&repo, cache_entry.as_ref());

        assert_eq!(updated.branch.as_deref(), Some("main"));
        assert_eq!(updated.ahead_behind, Some((1, 0)));

        std::fs::remove_dir_all(base).unwrap();
    }
}
