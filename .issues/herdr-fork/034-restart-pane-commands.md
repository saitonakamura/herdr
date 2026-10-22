---
id: herdr-fork-restart-pane-commands
title: Non-agent panes re-run their command on restore
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by:
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
