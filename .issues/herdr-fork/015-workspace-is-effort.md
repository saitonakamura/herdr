---
id: herdr-fork-workspace-is-effort
title: A workspace is an effort
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sync-and-branch
assignee: saitonakamura
claimed_by: macos/3d87f24a-ba58-420f-b5ef-157cbd75ec48
---

## Question

Per ADR 0001. A workspace is a named container with no cwd of its own and never takes its name from one. On the fork's first start, every workspace that today gets its name from its cwd keeps that name, frozen. Interactive creation asks for a name (`prompt_new_workspace_name` defaults to true); `workspace create` without `--label` names it after its starting cwd's basename, frozen. Persisted state from upstream must load. Classify as refactor-risk per AGENTS.md (characterisation tests, invariants).

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sync-and-branch

## Resolution

Merged 2026-10-16 as `1490aded`: [saitonakamura/herdr#13](https://github.com/saitonakamura/herdr/pull/13).

- Every `Workspace` constructor names the workspace: the label, or else the starting cwd's basename (`fallback_label_from_cwd`, `~` for home), not the repo name. That covers `workspace create`, the TUI with the prompt off, the startup workspace, `pane move` into a new workspace and worktree workspaces opened without `--label`. The name no longer follows the root pane's cwd.
- Upstream snapshots write an auto-named workspace as `custom_name: null`. The format is unchanged and still loads. On its first Git identity refresh after restore or handoff, such a workspace takes the label it was showing (the repo name) as its name and marks the session dirty. Smoke-tested in a throwaway session.
- `ui.prompt_new_workspace_name` defaults to true. The prompt is prefilled with the basename and always sends the confirmed name.
- Left for 016: `identity_cwd` and the workspace's branch and git facts. Left for 017: worktree child workspaces. Meanwhile, an unlabelled worktree workspace shows its checkout directory's name instead of its branch.
- Nothing to change in loom or the dotfiles: loom always passes `--label`.
