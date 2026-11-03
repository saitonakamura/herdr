---
id: herdr-fork-sidebar-reorder-efforts
title: Reorder efforts in herdr-sidebar by dragging a header or from its menu
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/w1:p10/44a77b14-2210-493d-bd87-eb70b7ac0cd2
---

## Question

Asked by the user on 2026-11-03: "i want to be able to reorder efforts in the sidebar". herdr-sidebar draws workspaces in herdr's order (`session.snapshot`'s `workspaces`) and offers no way to change it. herdr's native sidebar reorders by drag through `workspace.move { workspace_id, insert_index }` and `workspace.move_block { workspace_ids, before_workspace_id? }` (upstream `8843bbb0`, #1873, which also keeps a repo's worktree workspaces together). The order is herdr's, so it persists and every client sees it.

## Decided with the user (2026-11-03)

- **Drag.** Pressing on a workspace header and releasing over another place in the list moves that workspace there: above the header it is dropped on, or after the last workspace when dropped below it. While dragging, show where it will land (a marker row or the target header highlighted). A press and release on the same header stays a click (fold, as now).
- **Menu.** The workspace header's popup menu gains **move up** and **move down**. j/k already move the menu's selection, so their keys are `K` and `J`. After a move the popup stays open with the same entry selected, so `K K K` walks an effort up.
- **Worktree groups** move as one block, the way the native sidebar does: a repo's workspace and its linked-worktree workspaces travel together (`workspace.move_block`), and a single workspace never lands inside another repo's group. Follow upstream's rules (`8843bbb0`) rather than inventing new ones.
- **Scope:** `tools/sidebar` and FORK.md. Only API calls herdr already has; if one is missing or behaves differently, report blocked rather than changing the server.

## Parent

[herdr-fork map](./000-map.md)
