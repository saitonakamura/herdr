---
id: herdr-fork-sidebar-pane
title: Sidebar pane
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sidebar-content
  - herdr-fork-agent-identity-parent
  - herdr-fork-workspace-is-effort
assignee:
claimed_by: macos/w1:pS/f0047384-d08b-43e1-97f2-df45f517964a
---

## Question

Build herdr's half of the sidebar pane: the host. This covers ADR 0002 as amended by 0005 and 0006, with the content decided in 018. The program it runs is 030; the dotfiles wiring is 031. This ticket is one fork PR and touches no dotfiles.

- **Config.**
  - `[ui.sidebar] pane = [argv]` and `pane_collapsed_width` (default 8) go in `config.fork.toml`.
  - Leaving `pane` unset means the native sidebar, exactly as today.
  - `config check` validates both keys.
- **One pane per server.**
  - The server starts the process on its first start and keeps it for the server's life.
  - The pane belongs to no workspace or tab, and pane/agent listings and the session snapshot leave it out.
  - It can be reached separately under a neutral name: no "sidebar" in API method names, per AGENTS.md § runtime/client boundary.
  - It is not persisted or restored. Each server start runs it afresh.
  - It gets `HERDR_SOCKET_PATH` and the session name in its env, so it can talk to its own server and key its state file.
- **Layout.**
  - In the expanded layout, the pane takes the whole expanded sidebar's place in every workspace and tab, on every client.
  - Its width comes from `ui.sidebar_width` (clamped by min and max) and is shared by all clients. Dragging the divider is off while the pane is active.
  - Collapsing (`toggle_sidebar`) shrinks it to `pane_collapsed_width` instead of showing the native rail (ADR 0005).
  - `sidebar_collapsed_mode = "hidden"` still means zero width.
  - Mobile stays native.
  - The program sees resizes as normal SIGWINCH.
- **Input.**
  - Mouse events in the slot go to the pane as terminal mouse reports, like any pane that enables mouse tracking.
  - A new `focus_sidebar` action gives it keyboard focus.
  - herdr does not intercept Esc. The program decides when to give focus back, and calls an advertised API method to return focus to the pane that had it before. Pick a neutral name and document it. Decide, by the boundary guardrail, whether that focus is per client.
  - The program switches panes with the existing `pane.focus`, `workspace.focus` and `tab.focus`.
- **Lifecycle.**
  - If the process exits, herdr shows the native sidebar and restarts the program with backoff, and logs each exit with `tracing`.
  - A crash loop leaves the native sidebar up and says so once.
  - A config reload that changes `pane` restarts the program.
- **Events the program needs.** Check that `events.subscribe` covers everything 018 draws: pane, tab and workspace lifecycle; `pane.updated`; `pane.agent_status_changed`; `message.queued` and `message.delivered`; and later the annotation changes from 035. Add a subscription only where one is missing.
- **Performance.**
  - The pane is one more PTY. Its output is parsed once, and only it redraws its own slot.
  - Profile `just bench-render-scale` with the pane on and off, at 1 and at 15 or more populated panes, using a stub program that redraws on every event, and report the delta.
- **Endpoint contract.**
  - Anything new on the wire stays optional.
  - A client without the feature keeps the native sidebar.
  - The private TUI socket gains no behaviour the API lacks.
- **Tests.**
  - Engine and state tests for layout with and without the pane, collapsed and hidden.
  - Restart and fallback, using a stub program that exits.
  - Listings exclude the pane.
  - Live smoke in a throwaway session with a trivial program, for example a bash loop that prints its size and the mouse reports it receives.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sidebar-content
- herdr-fork-agent-identity-parent
- herdr-fork-workspace-is-effort

## Resolution

Draft until [PR #28](https://github.com/saitonakamura/herdr/pull/28) merges.

- **Name.** The API calls it the **session pane**, since it belongs to the server session, not a workspace: `session_pane.get`, `.focus`, `.unfocus`. The documentation is in FORK.md § Sidebar pane, GLOSSARY.md and the `docs/next` socket API page.
- **Focus is shared, not per client.**
  - Keyboard focus is a session fact, like pane focus, so the server owns it, and every client that shows the pane sends its keys there.
  - herdr never takes Esc. The program gives focus back with `session_pane.unfocus` or with any pane, tab or workspace focus method. Clicking a pane does too.
  - `keys.focus_sidebar` (unbound) toggles focus, so a stuck program cannot trap the keyboard.
  - The program gets focus reports (`\e[I` and `\e[O`) when it enables them.
- **Wire.**
  - The client opts in with the hello flag `session_pane`, and the server advertises the `session_pane` capability.
  - Frames, input and slot size travel as named controls (`endpoint.session-pane.v1`, `.input.v1`, `.slot.v1`). Generation-1 codecs are untouched.
  - The server owns the width: `ui.sidebar_width`, clamped. Each client reports its slot, and the PTY follows the foreground client's slot.
  - Shells showing several machines keep the native sidebar.
- **Lifecycle.**
  - Restart backoff runs from 0.5s, doubling to 30s.
  - Five exits in a row that each lasted under 30s leave the program down, with one `warn!` and one Custom notification. A config reload gives it another try.
  - A live handoff drops the program, as it drops the popup, and the new server starts it fresh.
- **Stale panes.** The client ties a pane to the connection and server boot that sent it, and drops it on any other snapshot. A reconnect or handoff onto stock herdr, or onto a server without `pane`, therefore can't leave a frozen pane holding focus. The server sends every opted-in client a first update, `pane: null` included. Keyboard mode and mouse capture follow the pane only for clients whose slot is wider than zero. Synchronized output holds the slot back for at most 1s.
- **Events.** `events.subscribe` already covers everything 018 draws. Annotation events belong to 035.
- **Performance.** `just bench-render-scale` has a session pane section. A stub that redraws every row adds a constant 110–175µs for each frame it redraws, with no scaling between 1 and 15 panes. Output from the sidebar alone never renders the pane surface.
