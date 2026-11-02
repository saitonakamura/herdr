---
id: herdr-fork-sidebar-attention-two-axes
title: Draw loom's wanted and seen in herdr-sidebar, with an ack key
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/w1:p1P/98495540-63fc-4a6d-a7dc-389963e21b04
---

## Question

Filed 2026-10-27 from dotfiles loom 095 (loom node `93143de0`). That ticket builds dotfiles
loom 085's attention model on the loom side. A plugin can't change how a sidebar row is drawn or add a
key to it, so this half lives here.

**What loom now gives every agent** (fork 035 annotations and 013 task status, nothing new in herdr):

- *wanted*: `task_status == needs_user`. loom sets it on an `asking` or `handoff` report. loom clears it
  on the next report, on a reply, or when the agent works again after it stopped. Before 095 a handoff
  stayed wanted after the agent resumed (dotfiles loom 078).
- *unseen*: annotation `unseen=<report kind>` (`done`, `failed`, `blocked`, `asking`, `handoff`). Every
  real report sets it, and only `loom ack` clears it. Focus never clears it.
- *reason*: annotation `reason=<one line, ≤120 chars>` while wanted. It is the question's first line, or
  the handoff's first step. loom clears it with the wanted flag.

**Wanted here.**

1. Four row states (085 item 1, the ISA-18.2 shape from loom's `research-human-attention`):
   - wanted + unseen: **loud**, the current needs_user treatment or stronger.
   - wanted + seen: **steady**, still marked, no longer pulling.
   - not wanted + unseen: **quiet but listed**, e.g. a dim marker. A finished child, or a handoff whose
     agent resumed.
   - neither: nothing.
2. A `reason` line token, or reason text in place of `task` while wanted, cut to width.
3. An `a` key on an agent row that runs `[sidebar.actions] ack` the way `c` runs `close`. Today the key
   table in `tools/sidebar/src/keys.rs` is fixed: `[sidebar.actions]` only overrides built-in commands, and
   an unknown action name is accepted and never bound. Either add a built-in `Ack` command with no default
   effect (needs the action), or let `[sidebar.actions]` entries name a `key`. dotfiles will then add
   `ack = { command = ["loom", "ack", "{agent_id}"] }` next to `close`.
4. Focus for about 10 s only *dims* the loud state, using the row's `focused` flag. It never clears unseen:
   the user moves through panes too often for focus to mean "read" (085 item 2).

Until then, dotfiles' `config.fork.toml` binds `prefix+alt+a` to `loom ack "$HERDR_ACTIVE_PANE_ID"`.

Docs: dotfiles `.issues/loom/085-attention-model.md` § Resolution,
`.issues/loom/095-attention-two-axes.md` § Resolution, `.config/herdr/plugins/loom/lib/attention.sh`.

## Decided with the user (2026-11-02)

047 (questions section) is folded into this ticket: one design, one child, one PR in `tools/sidebar`. No server or API change; everything is read from `pane list`.

Vocabulary (GLOSSARY.md): **wanted** (task status `needs_user`, or state `blocked` for any blocked reason), **unread report** (loom's `unseen` annotation, cleared only by `loom ack`), **mark read** (the `a` key), **attention section**. herdr's *acknowledgement* (focus clears `done`) is a different thing; don't call the `a` action "ack" or "seen" in sidebar UI or docs. loom's names (`loom ack`, `unseen`) stay.

- **Attention section**, pinned at the top of herdr-sidebar, always session-wide whatever the scope toggle. One row per agent that is wanted or has an unread report. Hidden when empty; `[sidebar] attention = false` turns it off. The agent also keeps its row in its tree.
- **Three marked states**, drawn the same in the section and in the tree:
  - loud (wanted, unread): status glyph in reverse video, bold name;
  - steady (wanted, read): today's look, the coloured status glyph;
  - quiet (not wanted, unread): a dim `•` after the name, coloured by report kind (done cyan, failed red, …).
  - herdr-blocked panes count as wanted and stay loud until their state changes; they have no unread axis unless loom also set `unseen`.
  - Colours come from `[sidebar.colors]`.
- **Order** in the section: loud, steady, quiet; oldest first within each.
- **Row**: line 1 is glyph, name, workspace label, `?N` when the `questions` token is set, and how long it has waited. Line 2 is the reason cut to width: loom's `reason` annotation, else herdr's blocked reason (`question`, `permission`, `form`, `interrupted`), else for a quiet row the report kind and its first line. `enter` focuses the pane; `a` and `c` work as in the tree.
- **Focus dims loud**: after about 10 s with a loud row's pane focused, it draws as steady until its state next changes. Sidebar-local, not persisted, never clears unread.
- **`a` key**: a built-in `Ack` command, a no-op unless `[sidebar.actions] ack` is configured, mirroring `c`/`close`.
- **`questions` token**: herdr-sidebar gains a line-1 `questions` token (`?N`) for tree rows.
- **Dotfiles, same change** (branch `herdr-fork/052-sidebar-attention`): `config.fork.toml` adds `ack = { command = ["loom", "ack", "{agent_id}"] }` and the `questions` token on line 1. Stock 0.9.4 and the native sidebar stay untouched; keep `prefix+alt+a`.

## Parent

[herdr-fork map](./000-map.md)
