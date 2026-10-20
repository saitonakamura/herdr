---
id: herdr-fork-resume-args-env
title: Resume an agent with its spawn args and env
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-agent-identity-parent
assignee:
claimed_by: macos/8f9df145-aeab-4086-be38-5cfee881565a
---

## Question

herdr stores each agent's spawn args and env (today `--env` is dropped on restore) and resumes with them after a restart, following session moves (`/clear`, fork, codex `/new`, pi sessions as paths). Respect the per-harness flags loom drops on resume (`docs/setup/herdr.md` § loom, the resume table) and the profile dir (`agent_home`).

Same change in loom: `loom resume`, most of `lib/restore.sh` and `bin/loom-session-hook` shrink or go; `loom resume` stays as the by-hand restart.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-agent-identity-parent

## Resolution

Merged 2026-10-20 as `8a1cd484`: [PR #14](https://github.com/saitonakamura/herdr/pull/14) (branch
`fork/011-resume-args-env`, master merged in at 5bb70bdb) and dotfiles branch
`herdr-fork/011-resume-args-env` (d8c7192, master merged in).

- herdr stores `agent start` args (`launch_args`, replaceable with `agent set-args`), the pane's
  `--env`, and the profile dir a claude or codex session is reported in (`agent_home`), persists
  them, and resumes with them: per-agent session/headless flags dropped, pane env set in the
  restored shell, the profile selected with `env CLAUDE_CONFIG_DIR=…`/`env -u` on the command.
  Capability `agent_launch_resume`. The codex hook now reports its transcript path (integration 11).
- loom on such a server reports no resume command, its SessionStart hook is a no-op, escalate
  calls `agent set-args`, and `loom resume` stays the by-hand restart.
- On-box after the release: `herdr integration install codex` per box; the loom SessionStart hook
  can be unwired once every box runs the fork.
