---
id: herdr-fork-state-consistency
title: State consistency: server, detector and waits agree
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sync-and-branch
assignee: saitonakamura
claimed_by: macos/w6:p5/2c06d5cc-b134-4965-a44e-1c4c2bf815ad
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
