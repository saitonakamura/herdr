---
id: herdr-fork-repo-facts-per-pane
title: Repo facts belong to each pane
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-workspace-is-effort
assignee: saitonakamura
claimed_by: macos/w6:pN/bac80b05-6698-4a2f-9636-35a71f8ca36b
---

## Question

Branch, ahead/behind, git status and is-a-worktree are computed per pane from its cwd, cached per repo root (ten panes in one repo cost one `git status`; see AGENTS.md § multiplicative performance paths and run `just bench-render-scale`). Exposed on `pane get` / `agent list` and events, and as agent-row tokens. A workspace row shows its name and combined agent state only. Update the dotfiles' `[ui.sidebar.spaces]` rows in the same change.

015 (PR #13) left the workspace's `identity_cwd` and its workspace-level git facts in place. Removing them, so a workspace has no directory of its own (ADR 0001), is part of this ticket.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-workspace-is-effort

## Resolution

Draft, open until merged: [PR #16](https://github.com/saitonakamura/herdr/pull/16) (branch
`fork/016-repo-facts-per-pane`) and dotfiles branch `herdr-fork/016-repo-facts-per-pane` (1665eea).

- Each pane's terminal carries `repo` (checkout root, repo name, branch, ahead/behind,
  linked-worktree flag, optional dirty count) from its cwd. The Git worker dedupes by checkout
  root, so ten panes in one checkout cost one refresh. Branch and ahead/behind are always fresh;
  the dirty count (`git status --porcelain`) runs only while an agent row shows `git_dirty`, at
  most every 5 s per checkout. Exposed as optional `repo` on `pane get/list` and `agent get/list`,
  with `pane.updated` on change.
- The workspace lost `identity_cwd` and its git facts. Space rows show name and rolled-up state;
  `branch`/`git_status` still parse there but render nothing and `config check` flags them. New
  agent-row tokens: `repo`, `branch`, `git_status`, `git_dirty`, `worktree`.
- Fork config overlay (own commit, FORK.md § fork config overlay): fork builds merge an optional
  `config.fork.toml` over `config.toml`, so the shared dotfiles carry fork-only tokens without
  stock rejecting the file. The dotfiles put the trimmed space rows and a repo row per agent there;
  `config.toml` is unchanged.
- On-box: `~/.config/herdr/config.fork.toml` is linked on the primary mac and resolves once the
  dotfiles branch merges. Other unix boxes run the `ln -sfn` from `docs/setup/herdr.md` at
  cutover. The work and windows generated configs don't carry the overlay yet.
