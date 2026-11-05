---
id: herdr-fork-sidebar-reads-close-json
title: herdr-sidebar reads loom close --json instead of its stderr
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Follow-up from [059 sidebar popup menu](./059-sidebar-popup-menu.md). `tools/sidebar/src/menu.rs::refusal` matches `loom close`'s stderr prose, so rewording a line silently turns "ack and close" into force, or force into nothing. Dotfiles [loom 111](https://github.com/saitonakamura/dotfiles/blob/master/.issues/loom/111-close-structured-refusal.md#resolution) gave close a form a program can read. `loom close --help` is the contract.

- **One call:** `loom close NODE … --json` closes or explains. One object on stdout, every prose line on stderr.
- **Exit status:** 0 closed, 1 error, 3 held, 4 busy.
- **Refused:** `{"closable":false,"closed":false,"refusal":"held"|"busy","ack_clears":BOOL,"nodes":[{"id","name","pane","refusal","reason","ack_clears","verified_releases","status","live","line"}]}`.

## What to change

- **`CloseRequest::args`** adds `--json`.
- **`Refusal`** comes from the exit status and the object: 3 is `Held`, 4 is `Busy`, anything else non-zero is `Error`.
- **`unread_only` becomes the top-level `ack_clears`.** Don't re-derive it from `reason`. `ack_clears` covers both gates, and the prose never did: a node unseen and still working parses today as unread only, so the menu offers "ack and close", the user acks, and the close then refuses as busy. Add a test for that case.
- **`unread_nodes`** takes id and name from `nodes`, and can use `pane` in place of looking it up in the snapshot. When `ack_clears` is true every entry is `unseen`, so no node appears twice. Elsewhere it can, once per gate.
- **The refused screen's `lines`** chain stderr and stdout today. Take stderr alone, or the object lands on screen. The prose there is unchanged.
- **Exit 1** stays `Error`, with the first stderr line as the status. Stdout is empty, except when a pane would not close, where `"left":[…]` names what stayed.

## What to keep, and what to check

- **Older loom:** it exits 1 with `loom: close: unknown argument --json` and closes nothing. On that string alone, rerun without the flag and use the prose parser. It goes when no box has an older loom, which dotfiles reaches on its next pull; keep it through the cutover anyway.
- **`[sidebar.actions] close` is user-set argv.** `--json` now goes to whatever it names, so check what a command that isn't loom's should do, and say so in `FORK.md`.
- **`Refusal::Busy if request.force`** should no longer fire, because `--force` passes every hold and herdr does not refuse a forced close. Keep the arm so an unexpected busy still shows as a status line rather than offering force again.
- **Also new to the sidebar:** any herdr error other than `agent_subtree_busy` used to print as busy, which offered force. It now reads as an error and offers nothing, on both old and new parsers.

Switch only on `refusal`, `reason` and the exit status. `line` is prose.

Done: the menu's three outcomes come from `--json`, the prose parser runs only for an older loom, `FORK.md` says so, and the unseen-and-working case has a test.

## Parent

[000-map](./000-map.md)

## Blocked by

Nothing.
