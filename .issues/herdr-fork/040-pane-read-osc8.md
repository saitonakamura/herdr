---
id: herdr-fork-pane-read-osc8
title: Keep OSC 8 links in pane read
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Requested by the dotfiles terminal-clickability effort, from its ticket 002 (dotfiles 3979537 / 03a5bdf).

`herdr pane read` drops OSC 8 hyperlinks in every source (`visible`, `recent`, `recent-unwrapped`, `screen`, `detection`) and every format (`--ansi`, `--raw`), measured on `0.9.5+fork.1`. The terminal keeps them: Ctrl+click reaches them through `visible_hyperlinks` (`src/terminal/runtime.rs`). The `links` picker (dotfiles `.config/herdr/plugins/links`, `prefix+alt+o`) reads the pane through `pane read`, so it cannot see the OSC 8 links that claude, `ls` and `rg` now emit. Only Ctrl+click can.

Either of these works, and the plugin's `scan.pl` already parses OSC 8 first, so neither needs a plugin change:

- **A.** `pane read --ansi` (and `--raw`) keep OSC 8 open/close sequences around the linked cells. The ANSI formatters live in `src/terminal/runtime.rs` (`visible_ansi`, `recent_ansi_snapshot`, `recent_unwrapped_ansi_snapshot`). A link that wraps or spans a styled run must still come out as one well-formed link.
- **B.** A pane links API that returns `uri`, `text` and position for each link in the requested source.

Decide between them:
- A is the smaller change, but it changes what existing `--ansi` callers receive. Check loom, the detection manifests (the `detection` source probably shouldn't change) and handoff history (`handoff_history_ansi`) before widening it. It may need an opt-in flag.
- B is new API surface: an advertised method, with neutral naming per the runtime/client boundary. It also gives Ctrl+hover-style resolution to non-TUI clients.
- Either way it is reachable from `pane read` only, not from render or detection. Keep it out of the multiplicative paths.

## Parent

[000-map](./000-map.md)

## Resolution
