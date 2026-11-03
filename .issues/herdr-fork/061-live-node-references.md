---
id: herdr-fork-live-node-references
title: A loom node id on screen renders its live state, the way GitLab renders !57 (merged)
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/w1:p1Y/9944e3bd-13b6-46cd-82bc-21bc2342b84d
---

## Question

Requested by the user on 2026-11-03, through the dotfiles loom cockpit (`7a05e2bb`).

An agent's closing prose goes stale while you watch. The dotfiles root ended a turn on "the
pi-migration child (50108375 pimig-001) needs you in wN:p2". Once the user answered there,
that line was false, but it stayed on screen as the pane's last word until something started
a new turn. Nothing rewrites text already in a pane. A statusline line was considered and
rejected (it is somewhere else, not where the claim is).

So make the reference carry its own state, the way GitLab renders `!57` as `!57 (merged)`.
herdr draws every pane from its own cell grid, so it can decorate what is on screen without
touching what the agent wrote.

- **Matching.** An 8-hex token that is a live node id in this session's agent annotations
  ([035](./035-agent-annotations.md)). Covers bare `8c1ee73d`, `loom:8c1ee73d` and
  `[8c1ee73d]`. A commit hash collides only if it equals a node id in the registry (about 240
  of 16^8), which is negligible. Closed nodes render as closed while their record exists.
- **Rendering.** The id is recoloured in its status colour, the same as the sidebar's dots
  ([045](./045-flip-report-after-colours.md), [052](./052-sidebar-attention-two-axes.md)).
  Where the line has trailing blank cells, add dim end-of-line text in them, like neovim's
  end-of-line diagnostics: `asking · wN:p2`, `done`, `closed`. Never insert text into the
  line, so layout, wrapping, copy, selection and `pane read` are unchanged. On a full line,
  recolour only.
- **Click.** Ctrl+click (as for links) focuses the node's pane.
- **Cost.** [040](./040-pane-read-osc8.md) kept link work out of render and detection, "the
  multiplicative paths". This one has to live in render, so scan only on-screen lines, cache
  matches per line until the line's content changes, and look state up in the annotations
  herdr already holds (no loom or socket call per frame). Measure the frame time with a
  screen full of ids and with none.
- **Opt-out.** A config switch, on by default in the fork build.

Decide and record:

- How the end-of-line text picks which of several ids on one line to describe (the first, or
  one per id when they fit).
- Whether `pane read --ansi` should ever show the decoration (proposed: no, it is render
  only).
- Later, not this ticket: the GitLab half for real. MR, PR and CI references (`!2938`,
  `owner/repo#47`) rendered through the reader hooks waits already use
  (dotfiles loom 089), e.g. `!2938 (reviewed)`.
  Note it under the map's open questions.

Done when a real loom session shows the incident's shape: a pane whose text names an asking
node shows it as asking, then as running once the user answers in that node's pane, with no
new turn in the first pane.

## Parent

[herdr-fork map](./000-map.md)
