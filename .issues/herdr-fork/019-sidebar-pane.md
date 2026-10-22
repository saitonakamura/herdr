---
id: herdr-fork-sidebar-pane
title: Sidebar pane
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sidebar-content
  - herdr-fork-agent-identity-parent
  - herdr-fork-workspace-is-effort
assignee:
claimed_by:
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
- **Events the program needs.** Check that `events.subscribe` covers everything 018 draws: pane, tab and workspace lifecycle; `pane.updated`; `pane.agent_status_changed`; `message.queued` and `message.delivered`; and later the annotation changes from 029. Add a subscription only where one is missing.
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
