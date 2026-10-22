---
id: herdr-fork-agent-annotations
title: Agent annotations
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by:
---

## Question

Per [ADR 0007](../../docs/adr/0007-one-owner-per-fact.md). Each agent carries **annotations** (GLOSSARY.md): an opaque string-to-string map that a program keeps on the agent. herdr stores it and never interprets it.

- **Storage.**
  - Annotations live beside `agent_id`, `parent_agent_id` and `task_status` on the agent's terminal.
  - They persist across server restart and live handoff, and survive `/clear` and pane moves, as 010's identity does.
  - Closing the pane drops them.
  - Choose limits on key and value length and on count, and document them. The key syntax should match tokens (`[A-Za-z0-9_-]`).
- **API and CLI.**
  - `agent annotate <target> k=v... [--clear k]...`, where an empty value also clears.
  - The current map appears as an optional `annotations` field on `pane get/list/current` and `agent get/list`.
  - A change publishes `pane.updated`.
  - Advertise capability `agent_annotations`.
  - On the stable endpoint this is a new optional JSON field, with no codec change.
- **Sidebar rows.** Decide whether native sidebar rows may reference annotations, for example `@role`. That would help a box without `herdr-sidebar` (Windows), but it is optional.
- **Same change in loom.**
  - Probe the capability with a real call.
  - Write `effort`, `role`, `kind` and `why` at `spawn`, `attach` and `move`, and re-write them in `restore` for agents that predate this change.
  - `scripts/herdr-ticket` writes `ticket` as an annotation, keeping the token while the native rows still read `$ticket`.
  - Stock herdr keeps today's tokens.
  - Keep `tests/run.sh` green.
- **Not in scope.** loom reading its own facts back from annotations instead of its registry is [032](./032-loom-stops-mirroring.md).

## Parent

[000-map](./000-map.md)
