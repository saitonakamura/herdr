---
id: herdr-fork-pi-sandbox-restore
title: Sandboxed pi panes lose their agent across a server restart
status: in-progress
labels:
  - wayfinder:research
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:p1C/cf45927f-48be-411e-b901-c5fc907ce3c3
---

## Question

Seen on 2026-10-26 when `work` moved from stock 0.9.4 to the fork (`hw --fork`).
Seven work panes running pi in the container (`scripts/pi-sandbox`, which reports
to herdr through `scripts/herdr-bridge`) came back as bare shells. Every claude
and codex pane came back.

- The stock server's last snapshot
  (`~/.config/herdr/sessions/work/session-snapshots/*-2169-0.json`, 09:24) holds
  only `cwd` for those panes, with no `agent_session` and no `agent_resume`. So a
  stock restart would have lost them too. Loom's registry still had each pi
  session id.
- `loom resume <node>`, typed into each pane by hand, brought all seven back.
  Afterwards the fork reports `agent_session` `herdr:pi` for each of them, and
  the fork's `session.json` holds all seven.
- On the fork, loom's `apply_resume` leaves restore to herdr (`herdr_resumes`).
  On stock it writes no command for pi, because `herdr:pi` owns the pane.

Find out:

1. Why did stock 0.9.4 save no session for those panes? Did the bridge's
   `pane.report_agent_session` never land, or was it dropped when the snapshot
   was written?
2. On the fork, does a restart now bring a sandboxed pi back? Which command does
   herdr type, and does it reach the container through the `pi` wrapper with the
   right profile and cwd? Test this in a throwaway session, never on `work`.
3. If either is broken, what is the fix, and where does it go: herdr, loom or
   the bridge?

## Parent

[000-map](./000-map.md)
