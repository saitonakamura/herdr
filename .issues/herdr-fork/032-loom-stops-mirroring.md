---
id: herdr-fork-loom-stops-mirroring
title: loom stops mirroring herdr's runtime facts
status: blocked
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-agent-annotations
assignee:
claimed_by:
---

## Question

Per [ADR 0007](../../docs/adr/0007-one-owner-per-fact.md), each fact has one owner. Today loom's registry still keeps its own copies of several facts about live agents: Parent, name, `.live` state, a combined state computed in `tree.jq`, and its message inbox. 024 was those copies drifting from herdr's.

On a fork herdr with 035, loom changes as follows:

- **Live nodes come from herdr.** A live node is `agent list` plus annotations. Parent, name, state and task status are read from herdr and never stored.
- **The registry keeps only what herdr cannot know.** That is policy and history: efforts, routing, limits, closed nodes, outcomes and escalations. Decide how a node becomes history when its pane closes. The deliberate 010 divergence stays: herdr makes a child top-level, while loom's history keeps who started it.
- **The inbox reads herdr's queue as the record** (`message list`). That way messages the sidebar sends straight to herdr (018: `r`/`t`) appear in `loom ui` and `loom inbox`.
- **Readers.** `loom tree`, `loom ui`, `loom view`, `whoami`, `close` and `doctor` read from the new sources.
- **Stock herdr** keeps the current registry path, behind a real-call capability probe, until every box runs the fork.
- **Migration.** Existing lanes need no migration step beyond what `restore` does.

Keep `tests/run.sh` green, and add stub-herdr cases for the fork path.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-agent-annotations
