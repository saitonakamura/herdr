---
id: herdr-fork-agent-messages
title: Messages between agents
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-agent-identity-parent
  - herdr-fork-claude-hooks-state
assignee:
claimed_by:
---

## Question

herdr keeps a message queue per agent: kinds `report`, `reply`, `tell`; text stored by herdr. Delivery types one line, `[from <name>] <summary> (herdr message show <id>)`, when the agent is idle or blocked on `question` with an empty composer; otherwise it waits and goes oldest-first on the next state change. API, CLI (`herdr message …`) and events.

Same change in loom: `report`, `reply`, `tell` and `inbox` keep their names and flags and become calls into this; what a message says and who sees what stays loom's (a question for the user is never shown to the parent).

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-agent-identity-parent
- herdr-fork-claude-hooks-state
