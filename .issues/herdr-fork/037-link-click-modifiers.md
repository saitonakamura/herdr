---
id: herdr-fork-link-click-modifiers
title: Pass click modifiers to link handlers
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Requested by the dotfiles terminal-clickability effort ([its map](https://github.com/saitonakamura/dotfiles/blob/master/.issues/terminal-clickability/000-map.md), 366efd2), for its `links` plugin. Nothing in that effort waits on this ticket.

`PaneLinkActivateParams` (`src/api/schema/panes.rs`) carries the pane, row, column and revision, but no modifiers. Ctrl is hard-coded as the link gesture in `src/client/shell/mouse.rs`. The plugin wants Ctrl+Alt+click to **peek** at a link (a popup with live status) where Ctrl+click opens it.

- Carry the modifiers that were held on the click (at least Alt and Shift on top of Ctrl) through to the handler's action, as env such as `HERDR_PLUGIN_CLICK_MODIFIERS=ctrl+alt`.
- Decide what a modified click does when no plugin handler claims the link: the default open, or nothing.
- **Compatibility.** `pane.link.activate` is a frozen generation-1 method (`tests/fixtures/endpoint-method-shapes-v1.json`). An old server would ignore a new `modifiers` field and report success, which runs the open action when a peek was meant. So use a new method or a separately advertised capability, and leave the field off when the server lacks it. Never update the frozen fixture.
- Check how kitty delivers Ctrl+Alt+click on the mac before settling the gesture. The map defers to the plugin on which modifier means what.

## Parent

[000-map](./000-map.md)

## Resolution
