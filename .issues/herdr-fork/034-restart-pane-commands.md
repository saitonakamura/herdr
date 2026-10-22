---
id: herdr-fork-restart-pane-commands
title: Non-agent panes re-run their command on restore
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:pV/a6559c7f-193e-49c5-95af-0196445aa5ae
---

## Question

After a server restart, a tab's panes come back, but a dev server, watcher or `tail` comes back as a bare shell. Agents already resume through their stored argv (011). The user wants non-agent panes to re-run their command as well. This is a nice-to-have from 018.

Before building, decide with the user:

- **Which command to restore.** The foreground process at snapshot time, or only a command herdr started (`pane run`, `tab create --command`)?
- **Opt-in or default.** Re-running something like `rm` or a deploy on restore would be harmful, so consider an explicit per-pane mark, an allowlist in config, or only commands herdr started.
- **Where the cwd and env come from.**
- **How it fits beside upstream's restore code**, keeping the change small for merges.

## Parent

[000-map](./000-map.md)

## Resolution

Draft, pending merge: [PR #27](https://github.com/saitonakamura/herdr/pull/27), branch `fork/034-restart-commands`.

Decided with the user 2026-10-22:

- **Which command:** explicit mark only. `herdr pane set-restore-command [--run|--clear] <pane> <command>` (API `pane.set_restore_command`, capability `pane_restore_command`) stores a one-line shell command on the pane, persisted as `restore_command`. Panes `layout.apply` starts with a `command` get one automatically. Plugin panes, popups and custom keybinding commands do not.
- **How it runs:** typed into the pane's fresh shell on a cold restart, like agent resume (rc and direnv first; Ctrl-C leaves a shell). An agent pane with a resumable session still resumes the agent, and live-handoff panes are not re-run.
- **cwd and env:** the pane's saved cwd and its `--env`.
- **Fit beside upstream:** one optional field each on `TerminalState`, `PaneSnapshot` and `PaneInfo`, typed after the plain-shell spawn in `restore_tab`. The setter is in `src/app/pane_restore_command.rs`.

No loom or dotfiles change was needed: nothing there reads or writes these fields.
