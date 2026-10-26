---
id: herdr-fork-drag-session-pane-width
title: Drag the divider between the sidebar pane and the tabs to resize it
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by:
---

## Question

Filed 2026-10-26 at the user's request: "can we add draggable separator between
sidebar and tabs?"

The native sidebar already has this. `src/client/shell/mouse.rs` turns a press on
`hits.sidebar_divider` into `ClientChromeDrag::SidebarWidth`, and a double-click
resets the width to `sidebar_width`. With the session pane (herdr-sidebar, ADR 0006)
the client skips the native sidebar render, so nothing sets `hits.sidebar_divider`.
Its width also comes from the server's `SessionPaneView.width`
(`src/client/shell/config.rs`: "The server clamps it, so every client gives the
pane one width"), because every attached client shows the same PTY.

Make that boundary draggable when the session pane is shown:

- **The width stays shared.** One program runs per server, so one width holds for
  all clients. A drag asks the server to set it, through an advertised API method
  with a neutral name (e.g. `session_pane.set_width`), clamped to
  `sidebar_min_width`/`sidebar_max_width`. Every client follows the next
  `SessionPaneView`. Keep to the stable client endpoint contract in CLAUDE.md: a
  server without the method disables only the drag.
- **Same feel as the native divider.** Same hit column, same drag, and a
  double-click resets to the configured `sidebar_width`. While dragging, the
  client may draw the new width locally before the server's update arrives, as
  long as the update wins.
- **It persists** for the session, across a server restart, in session state or
  wherever the native manual width lives today. Say which, and why.
- Collapsed and hidden modes keep their widths. The drag applies to the expanded
  width only.
- **Tests.** Client tests for the hit area, the drag and the double-click with a
  session pane shown. Server tests for the clamp and the update reaching
  clients.
- **Live verify** in a throwaway session on a build of the branch with
  `[ui.sidebar] pane = ["herdr-sidebar"]`: drag it wider and narrower, then
  double-click. Check that herdr-sidebar redraws at each width.

## Parent

[000-map](./000-map.md)
