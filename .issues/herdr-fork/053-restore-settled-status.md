---
id: herdr-fork-restore-settled-status
title: Save each pane's settled agent status, and restore a cut-off turn as blocked (interrupted)
status: closed
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

Merged 2026-11-02: [PR #48](https://github.com/saitonakamura/herdr/pull/48) as `312bd9b2`.

- The snapshot saves `agent_status` (`{status, blocked_reason?}`) per agent pane. A status change schedules a save, and statuses stay out of the layout fingerprint. The field is optional and read leniently.
- A resumed pane takes it back as an overlay. `blocked(r)` stays `blocked(r)` and `done` stays unseen. `working`, or `awaiting` without a live wait, becomes `blocked`/`interrupted`. A live wait reads awaiting.
- It clears on a non-idle hook report, a working or blocked screen match, the agent exiting, or the user typing into the pane. The resumed session's own idle report does not clear it.
- `interrupted` is reserved for restores: detection manifests may not name it. Messages wait for an interrupted pane; `agent prompt` still reaches it. Not reachable from any frozen endpoint codec.
- herdr-sidebar draws `blocked_interrupted` (magenta `‖`). loom keys on `agent_status` only, so it already treats the pane as blocked. The dotfiles branch `herdr-fork/053-restore-settled-status` updates the loom test stub and adds a held-tell test.
- Live repro in throwaway session `repro-053-133314`: a sentinel pane restored `blocked`/`question` and a pane stopped mid-turn restored `blocked`/`interrupted`, both after `claude --resume`.

