---
id: herdr-fork-agent-identity-parent
title: Stable agent identity and Parent
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sync-and-branch
assignee:
claimed_by: macos/w6:p9/facaa136-9db1-4343-8a94-9172fc3f376b
---

## Question

Per ADR 0004. A stable agent id, minted when an agent is started through herdr or attached (a detected agent gets one when first referred to). It survives session changes (`/clear`, fork), server restarts and moving to another pane. Parent links agent to agent: set by `agent start --parent`, changeable later (`loom move`), persisted; when the parent is closed the child becomes top-level. Exposed on `pane get` / `agent list` and as events.

Same change in loom: loom's node id is the agent id (`spawn`, `attach`, `move` set Parent); loom's own `parent` field reads from herdr. Keep every loom command and test working.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sync-and-branch

## Resolution

Draft, pending merge of [PR #8](https://github.com/saitonakamura/herdr/pull/8) (branch
`fork/010-agent-identity-parent`) and dotfiles branch `herdr-fork/010-agent-identity-parent`
(673ac7d).

- The agent id lives on the terminal (`agent_id`, `parent_agent_id`) and is persisted per pane,
  so it survives `/clear` and forks, pane moves, restarts and live handoff. It is 8 chars, a
  digit first, so it never collides with an agent name. It is minted by `agent start`,
  `agent set-parent` (any pane, including a shell, which is loom's attach) or, for a detected
  agent, the first API request after detection. An agent exiting does not drop it; only closing
  the pane does.
- Parent is set with `agent start --parent` or `agent set-parent <target> <parent>|--clear`.
  Herdr refuses an unknown parent, the target itself and its descendants. When the parent's pane
  closes, its children become top-level. Both fields are on `pane get/list/current` and
  `agent get/list`, agent targets accept the id, and every change publishes `pane.updated`.
- loom: the node id is the herdr id (spawn, attach), move sets herdr's Parent, and restore copies
  herdr's Parent back, except that a parent dropped because its pane closed stays in loom. whoami
  matches the agent id first. A stable herdr falls back to loom's own ids. Spawned panes no longer
  get `LOOM_NODE` in their env on the fork, since the id exists only after the pane does.
- On-box: nothing beyond pulling dotfiles once the branch merges. It is safe before or after the
  fork release that carries 010.
