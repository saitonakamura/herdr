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
