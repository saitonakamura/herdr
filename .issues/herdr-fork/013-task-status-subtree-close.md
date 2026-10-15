---
id: herdr-fork-task-status-subtree-close
title: Task status and closing a subtree
status: blocked
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
