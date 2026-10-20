---
id: herdr-fork-task-status-subtree-close
title: Task status and closing a subtree
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-agent-identity-parent
assignee:
claimed_by:
---

## Question

Agents report a task status, stored by herdr next to the detected state: `starting`, `running`, `needs_parent`, `needs_user`, `complete`, `failed`, `closed`. Closing an agent can close its whole subtree, deepest first, refusing while any of it is working or blocked unless forced, and closing panes, tabs or workspaces only when wholly owned by the subtree.

Same change in loom: `report --kind` maps onto task status (blocked → needs_parent, asking → needs_user, done → complete); `loom close` keeps its outcomes and policy and calls this.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-agent-identity-parent

## Resolution

Merged 2026-10-20 as `b5539aaf`: [PR #18](https://github.com/saitonakamura/herdr/pull/18); dotfiles
branch `herdr-fork/013-task-status-subtree-close` merged as `a7164c9`.

- herdr keeps `task_status` per agent terminal, next to the detected state: `agent start` sets
  `starting`, `agent set-task-status` the rest. Persisted, on `pane get/list` and `agent get/list`,
  published as `pane.updated`.
- `agent close <target>... [--force] [--dry-run]` closes each target and its descendants deepest
  first, the caller's pane last, refusing with `agent_subtree_busy` while any is working or blocked.
  A tab or workspace goes only when the closing pane is its last one. Capability `agent_task_status`.
- loom probes the capability, not `--help` (loom 065). Every node status change goes to herdr
  (blocked → needs_parent, asking → needs_user, done → complete), and `loom close` keeps its
  outcomes and refusal, then calls `herdr agent close`. Stock herdr keeps loom's own close.
- On-box: nothing beyond pulling dotfiles after both merge; the loom side is safe on stock herdr.
