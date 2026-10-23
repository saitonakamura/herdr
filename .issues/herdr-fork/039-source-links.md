---
id: herdr-fork-source-links
title: Source links between efforts
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:p11/dbb9b231-4036-42d7-af37-fac11e847743
---

## Question

Per [ADR 0008](../../docs/adr/0008-parent-stays-in-a-workspace.md) and [033's resolution](./033-cross-effort-spawn.md#resolution). An agent started in another workspace gets a **Source** (GLOSSARY.md), never a Parent.

- **herdr: Parent stays inside one workspace.**
  - `agent start --parent` and `agent set-parent` refuse a parent in another workspace, with a new error code.
  - A pane moved to another workspace loses its Parent. Its children that stay behind lose theirs too.
  - Snapshot load and live handoff drop every cross-workspace Parent.
  - Each drop publishes `pane.updated`.
  - herdr knows nothing about Source.
  - This is refactor-risk by AGENTS.md: it touches persisted state, restore/handoff and identity, so characterise first and use the identity invariants.
- **loom: Source as the annotation `source=<agent id>`.**
  - Only on a server that advertises `agent_annotations`, probed with a real call. Stock herdr keeps today's cross-workspace Parent.
  - **Spawning.**
    - `spawn` sets Parent when the new agent lands in the caller's workspace, and Source when it lands in another, whatever the effort ids say.
    - `--root` and `--effort X` behave the same way here.
    - `--effort X` with no live workspace for X opens a new workspace for it. The agent is top-level there, whatever its kind, and is no longer placed in the caller's workspace.
  - **Moves and restore.**
    - `loom move` across workspaces writes `source=<old parent>` on the moved agent and `source=<moved agent>` on each child left behind.
    - `loom restore` writes `source` from the registry for every link herdr dropped.
  - **Reports.**
    - A started agent reports to its Source *and* to its effort's live root (`kind: root` in that workspace), when that root isn't itself.
    - Each copy says what its recipient is to the sender, in the summary text: `(started by you)` or `(from <effort> via <source name>)`. herdr's message gets no new field.
    - `spawn --handoff` records the Source but cuts its copy. The preamble says so, and anything the agent needs from a person goes to the user in its own pane.
    - With the Source's pane gone, only the effort copy goes out.
  - **Replies and tells.**
    - Either recipient may `reply` and `tell`.
    - The first reply withdraws the other copy if it is still pending (`message withdraw`).
    - A later reply goes through, with a warning naming who already answered.
  - **Close.**
    - Closing the Source never closes the started agent.
    - Either side may close it by naming it (`loom close <node>`).
  - **Views and depth.**
    - `loom tree` and `loom ui` draw a started agent as a root of its own effort with `← <source>`.
    - Depth counts from 0 per effort.
  - Keep `tests/run.sh` green, and add cases for both stock and fork herdr.
- The sidebar marker is [030](./030-sidebar-program.md)'s.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-agent-annotations
