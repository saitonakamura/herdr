---
id: herdr-fork-agent-messages
title: Messages between agents
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-agent-identity-parent
  - herdr-fork-claude-hooks-state
assignee:
claimed_by: macos/0f6c6ace-950b-4cb4-92b1-47dbae692b2b
---

## Question

herdr keeps a message queue per agent: kinds `report`, `reply`, `tell`; text stored by herdr. Delivery types one line, `[from <name>] <summary> (herdr message show <id>)`, when the agent is idle or blocked on `question` with an empty composer; otherwise it waits and goes oldest-first on the next state change. API, CLI (`herdr message …`) and events.

Same change in loom: `report`, `reply`, `tell` and `inbox` keep their names and flags and become calls into this; what a message says and who sees what stays loom's (a question for the user is never shown to the parent).

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-agent-identity-parent
- herdr-fork-claude-hooks-state

## Resolution

Merged 2026-10-20 as `fc1e4c0b`: [PR #15](https://github.com/saitonakamura/herdr/pull/15) (branch
`fork/012-agent-messages`) and dotfiles branch `herdr-fork/012-agent-messages` (fe0839d).

- herdr keeps a per-agent message queue (`report`, `reply`, `tell`) in session state, persisted
  across restart and handoff. `message.send|list|get|withdraw` over the API, `herdr message
  send|list|show|withdraw` on the CLI, `message.queued`/`message.delivered` events (plugin
  hookable). Delivery types `[from <name>] <summary> (herdr message show <id>)` through the
  `agent prompt` path when the recipient is idle without a draft, or blocked on a question with
  an empty composer; otherwise oldest-first, one attempt per state change. Messages to agents that
  are gone are dropped.
- loom probes `herdr message list --help`; with it, report/reply/tell queue in herdr and record the
  id as `message:`, inbox syncs `delivered:` from herdr, doctor counts only what herdr holds
  pending. An asking report sends the parent only the pane the user is wanted in, never the
  question, and is withdrawn once the child moves on. Stock herdr keeps loom's own push.
- On-box: nothing beyond pulling dotfiles after both merge; the loom side is safe on stock herdr.
