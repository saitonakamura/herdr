---
id: herdr-fork-loom-migrate
title: Move live loom lanes into herdr
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-resume-args-env
  - herdr-fork-agent-messages
  - herdr-fork-task-status-subtree-close
assignee:
claimed_by: macos/w1:pB/2901d6af-33c8-48fa-afbf-27c0b335a4ca
---

## Question

A one-time `loom migrate`, per box and per herdr session: import each live lane (nodes → agents with Parent, args, env, task status; undelivered inbox items → messages) and keep the old lane directory as a backup. Idempotent and dry-runnable. Then write one dotfiles ticket per box to run it after that box has a release with 010–013.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-resume-args-env
- herdr-fork-agent-messages
- herdr-fork-task-status-subtree-close
