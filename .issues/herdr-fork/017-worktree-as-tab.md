---
id: herdr-fork-worktree-as-tab
title: A worktree opens as a tab
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-workspace-is-effort
  - herdr-fork-worktree-post-create
assignee:
claimed_by:
---

## Question

`worktree create` makes the checkout, runs `post_create`, and opens it as a new tab in the current workspace with its cwd there. `worktree remove` closes that tab and removes the checkout; branches are never deleted. Worktree child workspaces and their sidebar grouping go. Same change in loom: `spawn --worktree` means a child in a new worktree tab.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-workspace-is-effort
- herdr-fork-worktree-post-create

## Resolution

Merged 2026-10-20 as `5b872e40`: [PR #19](https://github.com/saitonakamura/herdr/pull/19); dotfiles
branch `herdr-fork/017-worktree-as-tab` merged as `104ff7a`.

- A tab records the checkout it was opened for (`Tab.worktree`); workspaces no longer do. Worktree
  child workspaces, workspace groups and the restored-membership validation are gone;
  `workspace.close`'s `close_group` is accepted and does nothing.
- `worktree create`/`open` open a tab in `--workspace` (else the active one); the repo is `--cwd`, else
  the workspace's focused tab. `--workspace` and `--cwd` combine. A linked checkout resolves to its
  main one, so a worktree tab can make a sibling. Create's repo discovery moved to the background
  read path (a blocked `.git/config` could stall the server otherwise).
- Removal: new method `worktree.remove_tab` (`--tab` or `--workspace` = its one worktree tab); closes
  every tab of that checkout through `tab.close`, keeps the branch. `worktree.remove` keeps its frozen
  v1 shape for older clients and resolves the same way. New: `TabInfo.worktree`,
  `WorktreeInfo.open_tab_id`, capability `worktree_tabs`.
- TUI: remove acts on the focused tab; the tab menu offers "Delete worktree checkout..." when a pane
  of the tab is in a linked worktree (an agent's repo facts, or a cwd under the worktree directory).
- Upstream state: a linked worktree workspace's record moves onto its tabs at restore.
- loom: `spawn --worktree` on a `worktree_tabs` herdr creates the child as a tab of the parent's
  workspace (placement `worktree-tab`, no root group) and `close` takes that tab. Stock herdr
  unchanged. Nothing on-box: it reaches boxes with the fork release that carries #19.
