---
id: herdr-fork-agent-waits
title: Agent waits with persisted deadlines, node monitors, reader hooks and run supervision
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/w1:p1B/20882d05-38fc-405c-8a42-17bbe55db2c6
---

## Question

The fork side of dotfiles loom 089 (`~/dotfiles/.issues/loom/089-herdr-agent-waits.md`), from the grilling resolutions of loom 080 (`loom wait`), 077 and 086 (dotfiles `8c485f7`). The spec lives in the loom ticket; this ticket is where the fork work is tracked. Keep the two statuses in step. The Rust loom (`tools/loom` in this workspace, loom 093) is blocked on this and on loom 091.

In short:
- **Waits as records** in the session snapshot: `{id, waiter, on, until, deadline, then, extensions, last_ok_at, errors}`. A wait fires as a herdr message to its decider, a deadline missed while the server was down fires on restart, and a re-armed wait gets a new id.
- **Node monitors**: a wait on an agent fires on its terminal task status or on close, and fires at once if the agent has already closed.
- **Interval reader hooks**: `[[readers]] kind = "mr" command = [...] interval = "5m"` in a plugin manifest. Readers run on the plugin-command threads, with `last_ok_at` and a result recorded per run. After N errors in a row the wait fires `failed: source unreachable`; when no attempt lands within the liveness window it fires `failed: reader dead`.
- **Run supervision** of a `loom run` exit file and pid. herdr never executes the command itself.
- **A wait-for graph** over waits, cycle-checked on insert. Asks and `blocked` join it later.
- Bounded extensions (2), then escalation one level up.
- An API to declare, list, extend and cancel waits. The `awaiting` status comes from a live declared wait, on top of today's sentinel (fork 028).

Design for the first PR: [assets/050-waits-design.md](./assets/050-waits-design.md).

Fork notes:
- Too big for one PR. Split it: records + API + persistence + deadline firing first, then node monitors, then readers, then run supervision and the graph.
- The server loop is already deadline-driven (`src/app/runtime.rs`). Keep each tick O(waits due), not O(waits).
- Plugin hooks today are `startup` and `events` only (`src/app/api/plugins/manifest.rs`). `[[readers]]` is a new manifest table; existing manifests keep their meaning.
- New methods are advertised capabilities. Snapshot fields are optional with defaults (stable endpoint contract, AGENTS.md).
- The reader threads must not leak processes the way the test harness does ([042](./042-test-plugin-process-leak.md)): give each reader run a process group and kill it on timeout or shutdown.

## Parent

[000-map](./000-map.md)

## Resolution
