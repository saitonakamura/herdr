---
id: herdr-fork-sidebar-pane
title: Sidebar pane
status: blocked
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sidebar-content
  - herdr-fork-agent-identity-parent
  - herdr-fork-workspace-is-effort
assignee:
claimed_by:
---

## Question

Per ADR 0002. `[ui.sidebar] pane = [argv]`: one pane per server takes the expanded sidebar's place in every workspace and tab, width from config shared by all clients (dragging off while active), mouse events go to it, a `focus_sidebar` action focuses it and Esc returns. If its process exits herdr restarts it and shows the native sidebar meanwhile. Collapsed and mobile layouts stay native. Unset means the native sidebar. Same change: loom's sidebar program (`loom sidebar`) per 018, and the dotfiles config.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sidebar-content
- herdr-fork-agent-identity-parent
- herdr-fork-workspace-is-effort
