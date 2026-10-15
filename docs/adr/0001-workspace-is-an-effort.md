# A workspace is an effort, not a repo

Upstream herdr treats a workspace as one repo: it takes its name from the first
pane's cwd and shows that repo's branch and git status, and a worktree becomes a
child workspace. The fork's user runs one line of work across several repos at
once (a cockpit agent in one repo, children in others), so the fork makes a
workspace a named container with no directory of its own. Repo, branch, git
status and worktree belong to each pane, and a worktree opens as a tab.

## Considered Options

- **Group repo workspaces under an effort level.** Rejected: it keeps a model
  that does not fit and works around it, and every repo-derived behaviour would
  still leak through.
- **Make loom's spawn tree the only structure.** Rejected: it ties herdr's core
  to one plugin's model and leaves nothing to organise work that has no agents.

## Consequences

This diverges from upstream in workspace identity, naming, the spaces panel and
worktree handling, all of which upstream changes often. Expect merge conflicts
there. Existing workspaces keep their current names, frozen at the fork's first
start.
