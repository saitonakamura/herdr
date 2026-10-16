---
id: herdr-fork-done-semantics
title: Done is per pane and acknowledged once
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sync-and-branch
assignee:
claimed_by: macos/w6:pB/fd2813bb-a0d6-4d2a-8790-91188c78eda2
---

## Question

Change done so that:

- it clears only when that pane is focused while the outer terminal has focus, not when its tab becomes active (`src/workspace.rs` switch_tab, `src/app/actions.rs` mark_active_tab_seen);
- one acknowledgement clears it for every client and the API (today each client acknowledges separately: `src/client/shell/endpoint_agent_state.rs`);
- no sound or toast for the pane you are focused on, including the blocked sound (`actions.rs` ~45-63).

loom's attention view and `$fold` `+N!` read herdr's status: check them.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sync-and-branch

## Resolution

Draft, pending merge of [PR #10](https://github.com/saitonakamura/herdr/pull/10).

- The server owns acknowledgement: `AppState::acknowledging_panes` is the focused pane of every shell client whose outer terminal reports focus (`Some(true)`; unknown focus does not count), synced once per loop tick. Entering it sets `seen` and emits `pane.agent_status_changed` `idle`, once, for every client and the API.
- `switch_tab`, `mark_active_tab_seen` and the `pane focus` / `agent focus` acknowledgement are gone. The client's `EndpointAgentPresentation` no longer projects or acknowledges done; it trusts the server's `agent_status`. No wire change.
- Sound (blocked included) and client notifications are suppressed only for acknowledging panes. The in-app toast keeps upstream's "not in the active tab" rule plus the same check.
- loom needs no change: it reads `agent_status`, and done now surviving tab switches is what its attention view and `$fold +N!` want.
- Merged with #5 (state consistency): its `published_done_panes` idle emitter stays as a fallback; its `viewer_attached` / `pane_is_watched` gate is dropped, since with no client the acknowledging set is empty and completions stay done.
- Left open: ja / zh-cn docs still describe the old rule; not live-smoked in a real client.
