---
id: herdr-fork-sidebar-dotfiles
title: Turn the sidebar pane on in the dotfiles
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:p0/04898394-fb7a-409a-bbb8-3f800cce4df1
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

## Resolution

Draft, pending merge of dotfiles branch `herdr-fork/031-sidebar-dotfiles` (38893fb).

- `config.fork.toml`: `[ui.sidebar] pane = ["herdr-sidebar"]`; `[sidebar]` with `glyphs = "nerd"`, 018's `line1`/`line2`, the 018 colours spelled out, and `[sidebar.actions] close` running `loom close {agent_id}` in a popup. `focus_sidebar = "prefix+alt+s"`. `[[keys.command]]` repeats `config.toml`'s list, since the overlay replaces it whole, with `prefix+alt+f`/`t` running `herdr-sidebar ctl fold|scope "$HERDR_ACTIVE_PANE_ID"` as shell commands. `loom view` bindings stay in `config.toml`.
- Work and windows stay native with no code: the work session's overlay is `config.work.fork.toml`, which nothing writes, and windows has none. `docs/setup/herdr.md` claimed that work read `config.fork.toml`, and the branch corrects it.
- Docs: `docs/setup/herdr.md` § the sidebar pane covers what turns it on, the fallback, keys, close through loom, and glyph tuning. No `AGENTS.md` line.
- Verified 2026-10-23 on a release build of fork master `f05782fb` in a throwaway session `sb031-1023` against the branch's config: `config check` ok, `herdr-sidebar glyphs` showed no diagnostics, `session_pane.get` showed `running` with 0 restarts, `prefix+alt+s` focused the pane and esc returned focus, `prefix+alt+f`/`t` toggled collapse and scope in the state file, and `prefix+b` gave 8 cols. The close action was not exercised; the session had no agent. loom tests passed 425/425.
- Merge order: the installed dogfood (`378bba24`, which has 019 but not 030) reports `unknown config section [sidebar]` under `config check` and ignores that section. It also has no `herdr-sidebar`, so a config reload on it falls back to the native sidebar after five exits and one notification. Rebuild dogfood before merging the dotfiles branch, or before the next reload.
- Left: the `fork` session Verify, after the dogfood rebuild and a user restart.
