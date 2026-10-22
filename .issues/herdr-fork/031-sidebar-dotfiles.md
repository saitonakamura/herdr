---
id: herdr-fork-sidebar-dotfiles
title: Turn the sidebar pane on in the dotfiles
status: blocked
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sidebar-pane
  - herdr-fork-sidebar-program
assignee:
claimed_by:
---

## Question

Wire 019 and 030 into the dotfiles. All of it goes in fork-only files, so stock boxes are unaffected.

- **`config.fork.toml`.**
  - `[ui.sidebar] pane = ["herdr-sidebar"]`.
  - `[sidebar]` with the glyph table, colours and the per-line tokens from 018.
  - `[sidebar.actions] close = { command = ["loom", "close", "{agent_id}"], popup = true }`, so dispatch nodes still record an outcome.
  - Windows and the work generated config stay without `pane`.
- **Keybinds.**
  - Rebind `loom.view-fold` and `loom.view-focus` in the fork overlay to `herdr-sidebar ctl fold|scope`, and bind `focus_sidebar`.
  - `loom view` and its bindings in `config.toml` stay for stock boxes.
- **Docs.** `docs/setup/herdr.md` gets a section on the sidebar pane: what turns it on, the fallback, and how to tune glyphs. A line in `.config/agents/AGENTS.md` only if agents need to know.
- **Verify.** Run it in the `fork` side session after a dogfood build carrying 019 and 030, and only when the user restarts that server.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sidebar-pane
- herdr-fork-sidebar-program
