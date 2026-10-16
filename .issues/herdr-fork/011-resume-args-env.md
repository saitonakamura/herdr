---
id: herdr-fork-resume-args-env
title: Resume an agent with its spawn args and env
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-agent-identity-parent
assignee:
claimed_by:
---

## Question

herdr stores each agent's spawn args and env (today `--env` is dropped on restore) and resumes with them after a restart, following session moves (`/clear`, fork, codex `/new`, pi sessions as paths). Respect the per-harness flags loom drops on resume (`docs/setup/herdr.md` § loom, the resume table) and the profile dir (`agent_home`).

Same change in loom: `loom resume`, most of `lib/restore.sh` and `bin/loom-session-hook` shrink or go; `loom resume` stays as the by-hand restart.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-agent-identity-parent
