---
id: herdr-fork-sidebar-reorder-efforts
title: Reorder efforts in herdr-sidebar by dragging a header or from its menu
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/44a77b14-2210-493d-bd87-eb70b7ac0cd2
---

## Question

Asked by the user on 2026-11-03: "i want to be able to reorder efforts in the sidebar". herdr-sidebar draws workspaces in herdr's order (`session.snapshot`'s `workspaces`) and offers no way to change it. herdr's native sidebar reorders by drag through `workspace.move { workspace_id, insert_index }` and `workspace.move_block { workspace_ids, before_workspace_id? }` (upstream `8843bbb0`, #1873, which also keeps a repo's worktree workspaces together). The order is herdr's, so it persists and every client sees it.

## Decided with the user (2026-11-03)

- **Drag.** Pressing on a workspace header and releasing over another place in the list moves that workspace there: above the header it is dropped on, or after the last workspace when dropped below it. While dragging, show where it will land (a marker row or the target header highlighted). A press and release on the same header stays a click (fold, as now).
- **Menu.** The workspace header's popup menu gains **move up** and **move down**. j/k already move the menu's selection, so their keys are `K` and `J`. After a move the popup stays open with the same entry selected, so `K K K` walks an effort up.
- **Worktree groups** move as one block, the way the native sidebar does: a repo's workspace and its linked-worktree workspaces travel together (`workspace.move_block`), and a single workspace never lands inside another repo's group. Follow upstream's rules (`8843bbb0`) rather than inventing new ones.
- **Scope:** `tools/sidebar` and FORK.md. Only API calls herdr already has; if one is missing or behaves differently, report blocked rather than changing the server.

## Resolution

Done in [PR #56](https://github.com/saitonakamura/herdr/pull/56), merge commit `4631d18f` (feature commit `3006d089`). It touches tools/sidebar and FORK.md only, and needed no dotfiles change.

- **Drag.** A workspace header now acts on release. Moving off its row starts a drag, and the drop lands above the header it is released on, or after the last workspace when released below the last tree. A rule naming the workspace marks the spot; when no gap row sits above the target (the first header), the target header is drawn reversed instead. A press and release on one header is still a click (fold or focus). Dragging starts only over the full list, not while scoped or searching.
- **Menu.** **move up** (`K`) and **move down** (`J`) keep the popup open on the same entry and report "moved up: N of M". At the top or bottom the entries stay visible and do nothing: the status says "already first" or "already last", and no request is sent.
- **Worktree groups.** `tools/sidebar/src/reorder.rs` mirrors upstream `8843bbb0`'s `workspace_move_method`: `workspace.move` for a plain workspace, `workspace.move_block` for a repo's group, a linked worktree never moved alone, and a linked header never a drop target. Finding: fork servers report `WorkspaceInfo.worktree = None` (`src/app/creation.rs`), because worktrees open as tabs (017). The group rule therefore never fires on the fork; a repo's linked worktree moves as a tab inside its workspace. The live run confirmed it: `repo`, with worktree tab `repo-feat`, dragged to the end with both tabs.
- **Verification.** Unit tests cover the drop targets, the move params, the steps and the marker, plus app drag and click tests, the menu K/J test and the schema test for the new requests. The live run used a throwaway session; its captures are in the PR body.

## Parent

[herdr-fork map](./000-map.md)
