---
id: herdr-fork-done-vs-unread-report
title: herdr's done and loom's unread report overlap on a loom child's sidebar row
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Raised by the user on 2026-11-03, after the sidebar got one loom mark per row (`6bbb5e10`). Parked undecided: the user wants to live with the new marks first.

When a loom child finishes, it ends its turn and sends a `done` report together. Its row then shows herdr's done glyph (teal, cleared by focusing the pane: an acknowledgement) and loom's unread `✓` mark (teal, cleared only by mark read, `a`/`loom ack`). Two "go look" signals that are cleared two different ways.

They aren't fully redundant: an agent outside loom (a root cockpit, a hand-started claude) has no reports, so done is its only "finished" signal; and a child the user talks to directly ends turns without reports.

Option discussed, not chosen: herdr-sidebar draws a loom child's state glyph (any agent with a task status) without done, so it shows live activity only (idle, working, awaiting, blocked), and the report mark alone says "finished, go look". Agents outside loom keep done. Sidebar drawing only; herdr's status, done notifications and acknowledgement stay as they are.

Revisit when the user has used the marks for a while.

## Parent

[herdr-fork map](./000-map.md)
