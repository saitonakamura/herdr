---
id: herdr-fork-task-status-subtree-close
title: Task status and closing a subtree
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-agent-identity-parent
assignee:
claimed_by: macos/w1:p4/70ab02b8-412f-4a9d-9f96-af7db87595ce
---

## Question

Agents report a task status, stored by herdr next to the detected state: `starting`, `running`, `needs_parent`, `needs_user`, `complete`, `failed`, `closed`. Closing an agent can close its whole subtree, deepest first, refusing while any of it is working or blocked unless forced, and closing panes, tabs or workspaces only when wholly owned by the subtree.

Same change in loom: `report --kind` maps onto task status (blocked → needs_parent, asking → needs_user, done → complete); `loom close` keeps its outcomes and policy and calls this.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-agent-identity-parent
