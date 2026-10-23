---
id: herdr-fork-source-links
title: Source links between efforts
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by:
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

## Resolution

Merged 2026-10-23: herdr PR #30 as `54f3bdfa` (head `d9afbba9`), dotfiles `6ae7a73` (branch `herdr-fork/039-source-links` at `05d751d`, 447/447 loom tests on the merge).

- **herdr.** One rule, `drop_cross_workspace_parents` (`src/app/agent_identity.rs`). `sync_agent_identities` runs it, and snapshot load and live handoff already mark identities dirty. `pane move` also runs it on a cross-workspace move, before building its result. That is the only path that changes a terminal's workspace: `pane swap` refuses cross-tab. `agent start --parent` and `agent set-parent` refuse with `agent_parent_cross_workspace`. Each drop publishes `pane.updated`. There is no capability or wire change, because error codes are not part of any frozen fixture. Tests run on the adversarial identity workspace with `assert_invariants_for_test()`; a same-workspace move keeping every Parent is the characterisation case. Snapshot load and handoff are tested through `App::try_new` and `App::new_from_handoff` (unix only: Windows CI restored only one of the test panes, and the state-level sync test covers the rule there). Moves cover every destination, including one that closes its source workspace.
- **loom.** Source is the annotation `source`, read through `live.jq`. A node with a Source has no loom Parent. The switch is the annotations probe (`herdr_sources`), so a fork from before 039 gets Sources too.
  - `spawn` decides by landing workspace. `--effort X` goes to X's root, else to any live agent of X. With neither, it opens a new workspace (placement `workspace`).
  - Reports go to the Source and to the effort root, labelled. The file keeps the root's copy as `cc`/`cc_message`. A reply answers the newest such report sent to its sender, of any kind. The first reply withdraws the other copy and records `answered_by`/`withdrawn`, and a later reply warns. Either copy that loom had to type is redelivered. `--handoff` sets `source_cut` and never reaches the Source, even with no root.
  - Either recipient may reply or tell (`may_message`). `tree`/`ui` show `← source`.
  - `move --parent` across workspaces makes the target the Source, and `--root` carries the old root's Source. `--workspace` into the Source's own workspace makes it the Parent again.
  - `move --workspace W` is new, because `loom move` never moved panes. The spawner approved it, along with the `--parent`/`--root` handling.
  - `restore` writes `source` for every live node whose recorded Parent herdr dropped across workspaces. A new `pane.moved` hook runs the same pass, so a drag in the TUI or a hand `pane move` gets Sources at once. Stock 0.9.4 has the event.
- **Verification.** Local `just check`: everything passes except the known machine failures (macOS ACL `chmod +a`, `cross_area`, `api_ping`, release tests vs `tag.gpgsign`); CI green on all three OSes. Live smoke on a throwaway headless session: refusal, both move drops, and a cross-workspace Parent planted in `session.json` dropped on server start while a same-workspace one survived. loom `tests/run.sh`: 447 passed, 21 in `test-source.sh` for fork and stock. Mutations that turn the Source path off fail 13 and 5 of them. loom was not smoke-tested with real agents.
- **On-box.** Nothing beyond pulling dotfiles once both merge. The next `loom restore` on a fork build writes `source` for each cross-workspace Parent herdr drops.
