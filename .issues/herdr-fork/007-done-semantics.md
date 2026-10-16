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
