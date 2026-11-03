---
id: herdr-fork-sidebar-continue-interrupted
title: herdr-sidebar offers continue on an interrupted agent and says why it is waiting
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Seen by the user on 2026-11-03 after restarting `hp`: two Claude panes (`wM:p1` loom cockpit `7a05e2bb`, `wM:p1R` `70c6cd60`) came back `blocked`/`interrupted` ([053](./053-restore-settled-status.md)), "but dont ask to continue". By 053's design the user decides; but the resumed session sits at an empty prompt, and the only sign is the row's colour, so it reads as a block with no question.

## Decided with the user (2026-11-03)

- **Menu entry.** On an agent whose state is `blocked`/`interrupted`, the row menu (059's popup) offers **continue**, key `C` (`c` is close). It sends "continue" to the agent through herdr's `agent.prompt` (which reaches an interrupted pane; messages do not), then closes the popup. Not offered on any other state.
- **Say why.** The row's second line and the attention section's reason line read "interrupted by restart" for that state, unless loom's `reason` annotation is set, which still wins as today.
- **Scope:** `tools/sidebar` and FORK.md; only API calls herdr already has.

## Parent

[herdr-fork map](./000-map.md)

## Resolution

Merged 2026-11-03: [PR #58](https://github.com/saitonakamura/herdr/pull/58) as `483a0009`. No dotfiles branch: nothing outside `tools/sidebar` and FORK.md changed.

- The row menu offers **continue** (`C`, listed first) only on an agent that is `blocked`/`interrupted`. It sends `agent.prompt { target: <pane>, text: "continue" }` and closes the popup.
- The "say why" is the attention row's reason line, which reads `interrupted by restart` unless loom's `reason` is set. The Question's "the row's second line" meant that line, not the tree row: an interrupted agent is always wanted, so it always has an attention row, and tree rows keep their branch line.
- Live run in throwaway session `repro-064`: a Haiku Claude (Claude Code 2.1.317) stopped mid-turn restored `blocked`/`interrupted`. The row read `interrupted by restart`, `m` showed `C  continue`, and `C` resumed the turn. The captures are in the PR.
- macOS CI was unavailable: `check (macos-latest)` was cancelled at 15m01s with 0 steps on both attempts, because no runner was assigned. master at `9223e8f1` and `29fd3f2a` shows the same. It was merged, with the spawner's approval, on a local macOS run: fmt, clippy `-D warnings`, sidebar tests 117/117, full nextest 4352 passed with only the 3 known local failures. ubuntu and windows CI were green.
