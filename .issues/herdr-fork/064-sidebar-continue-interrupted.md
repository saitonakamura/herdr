---
id: herdr-fork-sidebar-continue-interrupted
title: herdr-sidebar offers continue on an interrupted agent and says why it is waiting
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/w1:p24/caa3e259-cb44-4dc4-96f6-92adbdda799a
---

## Question

Seen by the user on 2026-11-03 after restarting `hp`: two Claude panes (`wM:p1` loom cockpit `7a05e2bb`, `wM:p1R` `70c6cd60`) came back `blocked`/`interrupted` ([053](./053-restore-settled-status.md)), "but dont ask to continue". By 053's design the user decides; but the resumed session sits at an empty prompt, and the only sign is the row's colour, so it reads as a block with no question.

## Decided with the user (2026-11-03)

- **Menu entry.** On an agent whose state is `blocked`/`interrupted`, the row menu (059's popup) offers **continue**, key `C` (`c` is close). It sends "continue" to the agent through herdr's `agent.prompt` (which reaches an interrupted pane; messages do not), then closes the popup. Not offered on any other state.
- **Say why.** The row's second line and the attention section's reason line read "interrupted by restart" for that state, unless loom's `reason` annotation is set, which still wins as today.
- **Scope:** `tools/sidebar` and FORK.md; only API calls herdr already has.

## Parent

[herdr-fork map](./000-map.md)
