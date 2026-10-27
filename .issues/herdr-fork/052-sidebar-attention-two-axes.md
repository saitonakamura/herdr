---
id: herdr-fork-sidebar-attention-two-axes
title: Draw loom's wanted and seen in herdr-sidebar, with an ack key
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
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
