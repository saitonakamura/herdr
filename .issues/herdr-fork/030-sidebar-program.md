---
id: herdr-fork-sidebar-program
title: The herdr-sidebar program, v1
status: blocked
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by:
---

## Question

Build `herdr-sidebar`, the program the sidebar pane runs. It is a Rust crate using ratatui, in this repo (for example `tools/sidebar/`, as its own workspace member so upstream merges never touch it), per [ADR 0006](../../docs/adr/0006-sidebar-program-ships-with-the-fork.md). What it draws and does is decided in [018's resolution](./018-sidebar-content.md#resolution). Follow it. In short:

- **Data.**
  - Read only herdr's API on `HERDR_SOCKET_PATH`, using herdr's own API schema types: a snapshot, then `events.subscribe`, and re-snapshot on `events_lost`.
  - loom facts come from annotations (035). Never read loom's files.
  - Coalesce redraws to at most about 20 a second, and never animate.
- **Content.**
  - Sections per workspace, each collapsing independently.
  - Agents as the Parent tree in creation order, then non-agent panes.
  - Two-line agent rows.
  - The glyph table, with the `unicode` fallback.
  - Task status words.
  - Pending-message counts.
  - A narrow mode under about 16 columns (ADR 0005).
  - Exclude the sidebar pane itself.
- **Interaction.**
  - Clicks focus the clicked pane; header toggles; the wheel scrolls.
  - The right-click menus, and `+ new`.
  - Every key in 018's table, with Esc handing focus back through 019's API method.
  - Built-in reply, tell and close on herdr's verbs, with `[sidebar.actions]` overrides. A `popup` override opens the command in a herdr popup pane.
- **State and control.**
  - Collapsed sections, folds and scope are kept in `$XDG_STATE_HOME/herdr/sidebar/<session>.json`.
  - A `herdr-sidebar ctl fold|scope [<pane>]` subcommand talks to the running program over its own socket.
- **Config.**
  - `[sidebar]` from herdr's config files, so `config.fork.toml` beside `config.toml` (or `$HERDR_CONFIG_PATH`). It holds glyphs, colours, the tokens on each line, and actions.
  - Reload on SIGHUP.
  - `herdr-sidebar glyphs` prints the table in use.
- **Distribution, same PR.**
  - `fork-release.yml` builds `herdr-sidebar-<os>-<arch>` for the four unix targets, covered by `SHA256SUMS` and the notices.
  - `scripts/dogfood.sh` installs it beside `herdr`.
  - FORK.md mentions it.
- **Tests.**
  - The row model, tree, narrowing and key table are tested without a terminal against synthetic snapshots and events.
  - Click zones are tested on a fixed-size buffer.
  - Live testing in a throwaway session needs 019. Until 019 lands, run the program in an ordinary pane.

[003](./003-tap-and-cutover.md) installs the asset from the tap formula.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-agent-annotations
