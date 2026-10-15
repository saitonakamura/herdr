---
id: herdr-fork-repo-facts-per-pane
title: Repo facts belong to each pane
status: blocked
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-workspace-is-effort
assignee:
claimed_by:
---

## Question

Branch, ahead/behind, git status and is-a-worktree are computed per pane from its cwd, cached per repo root (ten panes in one repo cost one `git status`; see AGENTS.md § multiplicative performance paths and run `just bench-render-scale`). Exposed on `pane get` / `agent list` and events, and as agent-row tokens. A workspace row shows its name and combined agent state only. Update the dotfiles' `[ui.sidebar.spaces]` rows in the same change.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-workspace-is-effort
