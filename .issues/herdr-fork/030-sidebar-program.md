---
id: herdr-fork-sidebar-program
title: The herdr-sidebar program, v1
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-agent-annotations
assignee: saitonakamura
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
  - A dim, clickable `← <source name>` token on an agent with a `source` annotation, drawn only while that agent is live ([033](./033-cross-effort-spawn.md#resolution)). It focuses the source, and nothing is drawn on the source's own row.
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

## Resolution

Merged 2026-10-22 as `255cb431`: [PR #29](https://github.com/saitonakamura/herdr/pull/29).

- **Program.**
  - `tools/sidebar/` (`herdr-sidebar`) reads only herdr's API: `session.snapshot` and `message.list`, re-read at most 20 times a second on any event, and from scratch after `events_lost` or a reconnect.
  - Its types are a subset of herdr's schema types. Its tests check them, its subscriptions and every request it sends against `docs/next/api/herdr-api.schema.json`.
- **Finding: no session-wide state event.** `pane.agent_status_changed` takes one `pane_id` per entry, so the program keeps a second subscription over its agent panes and resubscribes when that set changes. 019's "events.subscribe already covers everything" held only per pane.
- **herdr side.**
  - The fork-only `popup.open` exists because no API could open an arbitrary popup.
  - `[sidebar]` is a known top-level table that herdr never reads.
  - The session pane also gets `HERDR_CONFIG_PATH`.
- **Decisions.**
  - The ctl socket is `<HERDR_SOCKET_PATH>.sidebar`, since panes and keybind commands carry `HERDR_SOCKET_PATH` but not `HERDR_SESSION`.
  - Creation order is herdr's monotonic per-workspace pane number.
  - Age is the time in the current state as the program saw it, because herdr has no creation time.
  - Without an override, `p` opens loom's `ui` plugin popup, though not at the row.
- **For 031.**
  - Keep `git_dirty` in `[ui.sidebar.agents]`, since herdr counts dirty paths only while a native row shows it.
  - The `close` override is `{ command = ["loom", "close", "{agent_id}"], popup = true }`.
  - ctl is `herdr-sidebar ctl fold|scope`. With no pane it acts on the focused one.
- **Gaps.**
  - Non-agent rows don't show the command.
  - `loom ui` can't open at a row.
- **Verified.**
  - Live in a throwaway session: the tree, keys, SGR clicks, menus, reply and close (busy, then force), ctl, the narrow mode, SIGHUP and the popup override.
  - `just check`: only the known machine-specific failures.
