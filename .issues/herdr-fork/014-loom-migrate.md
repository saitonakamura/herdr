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

## Resolution

Draft; close when the dotfiles branch is merged.

- Dotfiles branch `herdr-fork/014-loom-migrate` (head e94c5e1): `loom migrate` (dry run by default, `--apply`), a doctor `registry migrate` row, docs in `docs/setup/herdr.md` § moving a lane into herdr, and per-box run tickets dotfiles `herdr-fork/006`–`010`. `tests/run.sh`: 391 passed, 0 failed.
- No herdr PR: the API already covers it. Env is not imported because herdr sets `launch_env` only at pane creation, and loom's only env (`LOOM_NODE`/`LOOM_PARENT`) is not read on a fork build.
- Refuses with exit 1 on stock 0.9.4 (names 010–013) and on the dogfood build without 011/013; end-to-end run on a throwaway session with a master release build re-keyed the tree, set Parent/args/task status, queued the owed report, and a second `--apply` changed nothing.
- Stock-era panes keep `loom resume <old id>` as their reported resume argv until they exit, so old ids resolve through the lane's `migrated.json`.
