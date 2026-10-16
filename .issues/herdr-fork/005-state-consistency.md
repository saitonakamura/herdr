---
id: herdr-fork-state-consistency
title: State consistency: server, detector and waits agree
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sync-and-branch
assignee: saitonakamura
claimed_by: macos/2c06d5cc-b134-4965-a44e-1c4c2bf815ad
---

## Question

Fix, for agents still detected from the screen (codex, Claude's fallback rules):

- the server's state disagreeing with the detector for long periods (#4875, #3993);
- `agent wait` missing a pane closing (#4690) and a focus-caused done → idle (#4473); a finished agent reading idle, not done, with no client attached (#5222);
- the detection window spanning the whole pane height, so old frames match (#4819) and narrow panes lose dialog footers (#2868): end it at the latest frame;
- `osc_title_working` outranking every blocker (#3467).

Add deterministic tests, not wall-clock ones. Re-check the dotfiles' `agent-detection/claude.toml` and `codex.toml` against the new behaviour and adjust them in the same change.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sync-and-branch

## Resolution

[PR #5](https://github.com/saitonakamura/herdr/pull/5), merged 2026-10-16 as `43664552`. Findings, design and verification: [assets/005-state-consistency.md](./assets/005-state-consistency.md) (on the PR branch).

- `pane.closed` per pane on tab, workspace and layout-replace close (#4690). The done → idle acknowledgement emits a status change (#4473). With no client attached, active-tab completions stay done (#5222).
- Manifest engine 4: wrap-tolerant `contains` (#2868) and a rule-level `after_last_line_regex` frame boundary (#4819). Claude dialog rules outrank the busy title (#3467).
- The detector re-asserts its last result every 2s, and a held agent's `working` is withdrawn (#4875, #3993; not reproduced live).
- `<agent>.v<N>.toml` overrides for a mixed fleet. Dotfiles branch `herdr-fork/005-state-consistency` adds `claude.v4.toml` beside an unchanged `claude.toml`, safe to merge any time.
- **Cleanup once every box runs a fork release with 005:** fold the dotfiles `claude.v4.toml` into `claude.toml` and delete it.
