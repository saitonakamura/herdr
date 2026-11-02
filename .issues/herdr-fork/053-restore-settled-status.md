---
id: herdr-fork-restore-settled-status
title: Save each pane's settled agent status, and restore a cut-off turn as blocked (interrupted)
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Seen 2026-11-02 after restarting the work session (`hw`) on dogfood `97ce123a`. 13 claude panes had ended their turn on the `⏳ awaiting your answer` sentinel. Before the restart they read `blocked`; after it they read `idle` or `done`, and the sidebar lost their question icon. `herdr agent explain` on each still matched `awaiting_user_answer` and said `blocked`, so screen detection saw the question; the status herdr reports did not follow it. Example: `w2S:p32` had loom `task_status: needs_user` and agent status `done`. Status is not in the session snapshot at all (`agent_status` is absent from the saved pane keys), so a restart rebuilds it. On the fork, claude panes run under lifecycle-hook authority, which restores as idle, and nothing re-reads the screen until a turn ends.

The user's direction: don't lean on the screen to recover this. Save the state.

Decided with the user (2026-11-02):

- **Save** each pane's last settled agent status in the session snapshot beside `agent_session`. That covers `blocked` with its reason (`question`, `permission`, …) and `done` versus `idle`, so an unseen finished turn stays unseen.
- **Restore** that status as the pane's initial status after a restart, for agents that resume.
- **A cut-off turn asks.** A pane saved as `working`, or as `awaiting` without a live agent wait, restores as `blocked` with a new reason, `interrupted`. The restart cut that turn off (or killed the background work it waited on), so the user decides whether to continue; herdr never types "continue" itself.
- **A live wait stays `awaiting`.** Agent waits (050) are already persisted and keep firing, so nothing was interrupted.
- **What clears a restored status:** the next real event replaces it — a lifecycle hook (a turn starting), user input in the pane, or a fresh screen match. No timer.
- **Compatibility.** The snapshot field is optional, so older snapshots restore as they do today and stock 0.9.4 ignores it. `interrupted` is a new `AgentBlockedReason` value. Check whether that enum is reachable from a frozen endpoint codec and whether older clients already tolerate unknown values. Give it an `Unknown` fallback where they don't, per CLAUDE.md "Stable client endpoint contract".

Check that loom and the dotfiles sidebar treat `blocked` + `interrupted` sensibly (attention queue, not an error). Where loom keys on `blocked_reason`, a dotfiles branch fixes it in the same change.

## Parent

[000-map](./000-map.md)

## Resolution
