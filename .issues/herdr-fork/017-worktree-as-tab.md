---
id: herdr-fork-worktree-as-tab
title: A worktree opens as a tab
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-workspace-is-effort
  - herdr-fork-worktree-post-create
assignee:
claimed_by: macos/w1:p6/613e7272-4a71-452d-8e2e-56e0acb9081b
---

## Question

`worktree create` makes the checkout, runs `post_create`, and opens it as a new tab in the current workspace with its cwd there. `worktree remove` closes that tab and removes the checkout; branches are never deleted. Worktree child workspaces and their sidebar grouping go. Same change in loom: `spawn --worktree` means a child in a new worktree tab.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-workspace-is-effort
- herdr-fork-worktree-post-create
