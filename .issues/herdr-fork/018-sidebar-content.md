---
id: herdr-fork-sidebar-content
title: What the sidebar shows and how it is driven
status: closed
labels:
  - wayfinder:grilling
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/4b860d51-4e02-4eec-abe3-7e0d71cc4486
---

## Question

Grill the user on the sidebar program's content and interaction before 019 is built: what rows it draws (agents, workspaces, the spawn tree, task status, repo facts), what clicks and vim keys do, how it relates to `loom ui` and loom's view toggles. Decided already: one sidebar pane replaces the whole expanded sidebar, its program is loom's, it redraws on events, vim keys when focused (ADR 0002).

## Parent

[000-map](./000-map.md)

## Resolution

Grilled with the user on 2026-10-22 over five rounds. Vocabulary: GLOSSARY.md adds **Effort**, **Scope** and **Annotation**, and Parent now applies within one effort. Decisions with trade-offs: [ADR 0005](../../docs/adr/0005-collapsed-sidebar-is-the-pane.md) (the collapsed sidebar is the pane too), [ADR 0006](../../docs/adr/0006-sidebar-program-ships-with-the-fork.md) (the program ships with the fork as `herdr-sidebar`, partly superseding 0002), and [ADR 0007](../../docs/adr/0007-one-owner-per-fact.md) (one owner per fact, and agent annotations).

**The program.**

- It is `herdr-sidebar`, a Rust crate in the fork repo built with ratatui. It ships as release assets beside `herdr` for the four unix targets, and the tap formula and `scripts/dogfood.sh` install it.
- Windows keeps the native sidebar, with `pane` unset in its config.
- `loom ui` stays the wide popup. Moving it onto the same program is a later ticket.

**Where facts come from.**

- herdr's API is the spine, through `session.snapshot` plus the event stream. herdr owns everything runtime, so the sidebar shows the same Parent, state and names that delivery and `agent close` act on.
- loom-only facts reach the sidebar as agent annotations (029): effort id, role, kind, why and ticket.
- The binary never reads loom's files.
- Tokens remain display hints.

**What it lists.**

- Every pane, live ones only. Closed agents and their history stay in `loom ui` and `loom tree --all`.
- One section per workspace, which is one per effort. Each section collapses and expands independently, and the state persists.
- Inside a section, agents form the spawn tree by Parent, in creation order, with no priority re-sorting.
- Non-agent panes follow the agents as one dim row each: the title, else the command, then the cwd basename, or `exited`.
- Tabs get no rows of their own; the tab name is a token.
- A section header shows the workspace name, its effort id and rolled-up counts.

**Parent and effort.**

- Parent applies only within an effort.
- An agent started from another effort is a root in its own section, with no provenance marker yet.
- What that link is and how the two sides talk is [033](./033-cross-effort-spawn.md).

**Scope.**

- One slot: the sidebar narrows to one effort or one subtree until cleared. It replaces `loom view focus`.
- It never touches the collapse state, and a header line names it while it is set.

**Agent rows.** For now, each agent takes two lines:

1. Tree guides, the state glyph, the harness brand glyph, the tab name, the ticket, a pending-message count when non-zero, and age flush right.
2. Dimmed: branch, ahead/behind, dirty count and worktree. This line is dropped outside a repo.

Additional rules:

- Root rows also show the effort id.
- Task status appears as a dim word only when it adds something: `needs_parent`, `needs_user`, `failed` (which also turns the name red), or `complete`.
- A narrowing row drops age first, then the ticket, then line 2.

**Glyphs.**

- Nerd Font glyphs are the default, from the Symbols Nerd Font kitty already maps. Each is followed by a space, because kitty draws them two cells wide.
- `glyphs = "unicode"` falls back to plain symbols.
- The table:

  | meaning | glyph | colour |
  |---|---|---|
  | idle | nf-fa-circle_o | green |
  | working | nf-fa-circle | yellow |
  | awaiting | nf-fa-hourglass_half | blue, as decided in 028 |
  | done | nf-fa-check_circle | teal |
  | blocked: question | nf-fa-question_circle | magenta |
  | blocked: permission | nf-fa-exclamation_triangle | red |
  | blocked: form | nf-fa-pencil_square_o | red |
  | blocked, no reason | nf-fa-circle | red |
  | other pane | nf-fa-terminal | — |
  | pending messages | nf-fa-envelope | — |
  | branch | nf-dev-git_branch | — |
  | worktree | nf-fa-code_fork | — |

- There is no animation. Redraws happen on events only, coalesced to at most about 20 a second, since the pane's output goes through herdr's PTY parser.

**Config.**

- Glyphs, colours, the tokens on each line, and action overrides live in `[sidebar]` in `config.fork.toml`.
- They are read at start and on SIGHUP.
- Layout, click zones and narrowing stay in code.

**Mouse.**

- Clicking a row focuses that pane, switching workspace and tab as needed; the sidebar does not keep focus.
- On a header, `▾/▸` collapses or expands it, and the name focuses that workspace.
- The wheel scrolls.
- Right-click opens a menu the program draws inline:
  - header: rename, close, new worktree, collapse others;
  - agent: reply, tell, close subtree, scope to subtree, open in `loom ui`.
- A `+ new` line opens a new workspace.
- Dragging to reorder workspaces and the native launcher menu are dropped; the launcher stays on its keybind.

**Keys, when focused.**

- Movement: `j/k`, `gg/G`.
- `enter` focuses the pane and leaves the sidebar.
- `h/l` and `za` fold.
- `f` toggles scope.
- `/` searches.
- `r` reply, `t` tell, `c` close.
- `p` opens `loom ui` at that row.
- `m` opens the menu.
- `esc` clears the search, then the scope, then returns focus to the previous pane through herdr's API. The program decides what Esc means, not herdr.

**Actions.**

- Built in, on herdr's verbs:
  - `r`: an inline line, then `message send --user --kind reply --reply-to <its latest report>`;
  - `t`: `message send --user --kind tell`;
  - `c`: the `agent close --dry-run` list inline, `[y/N]`, and an offer to `--force` on `agent_subtree_busy`.
- `[sidebar.actions] <name> = { command, popup }` replaces one action.
- The dotfiles replace only `close`, with `loom close` in a popup, so dispatch nodes still record their outcome.

**State.**

- Collapsed sections, folded nodes and the scope belong to the sidebar, in `$XDG_STATE_HOME/herdr/sidebar/<session>.json`. This is presentation state, not server state.
- The keybinds go to `herdr-sidebar ctl fold|scope <pane>` over the program's own socket.
- `loom view` stays, unsynced, for stock boxes and the native fallback.

**Narrow mode.**

- Collapsing the sidebar shrinks the pane to `pane_collapsed_width`.
- Under about 16 columns the program draws one cell per depth, then the glyph, then the start of the name.
- `hidden` still means zero width, and mobile stays native.

**Parked.** An attention strip on top, listing every agent that wants the user across workspaces. The user wasn't convinced, so it waits for real use.

**Follow-ups filed.**

- [019](./019-sidebar-pane.md): the pane host, rewritten as its spec.
- [029](./029-agent-annotations.md): agent annotations.
- [030](./030-sidebar-program.md): the `herdr-sidebar` program v1.
- [031](./031-sidebar-dotfiles.md): the dotfiles wiring.
- [032](./032-loom-stops-mirroring.md): loom stops mirroring runtime facts.
- [033](./033-cross-effort-spawn.md): cross-effort spawn (grilling).
- [034](./034-restart-pane-commands.md): non-agent panes re-run their command on restore.
- [003](./003-tap-and-cutover.md) gains `herdr-sidebar` in the formula.
