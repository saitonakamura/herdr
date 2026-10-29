---
id: herdr-fork-agent-waits
title: Agent waits with persisted deadlines, node monitors, reader hooks and run supervision
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/20882d05-38fc-405c-8a42-17bbe55db2c6
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

### PR1: records, API, persistence, deadlines

Built 2026-10-29 on `fork/050-agent-waits-records`. Open for PRs 2 to 4 (node monitors, readers, run supervision and the graph). The design and every place the build departed from it are in [assets/050-waits-design.md](./assets/050-waits-design.md#changes-made-while-building-pr1-2026-10-29).

- **Store.** `src/app/waits.rs` keeps the wait records in the session snapshot as an optional `waits` field; a snapshot without it loads, and the field stays out of the layout fingerprint. A `BTreeSet<(deadline_at_ms, id)>` index and per-waiter counts are rebuilt on load, so the next deadline, `live_waits` and `wait_needs_user` are O(1) and a fire is O(waits due). The limits are 32 live waits per agent and 1024 per session, with the 500 most recent finished waits kept.
- **Firing.** The headless loop wakes for the next deadline and fires what is due. A fire to an agent is a `tell` from `herdr wait`, `wait <id> fired: <reason>: <summary>`, with `wait_id` on the message. It goes through the same `queue_message` as `message.send`, so it batches with the recipient's backlog (051). A fire to the user sets `wait_needs_user` and sends nothing. The decider is resolved at fire time, and a wait with both extensions spent also goes one level up. A deadline that passed while the server was down fires on the first loop with that reason. A waiter that closes has its live waits cancelled, and nobody is told.
- **API.** `wait.declare|list|get|extend|cancel`, `wait.declared|fired|finished` events, `live_waits` and `wait_needs_user` on agent info, and the `agent_waits` capability with `wait_sources: []`. CLI `herdr wait declare|list|show|extend|cancel`.
- **Validated** by unit tests (the store, firing, escalation, missed deadlines, closing, permissions, re-arm, and snapshot round trips) and live on a throwaway session. A 5 s wait fired 2 ms after its deadline. A wait whose deadline passed while the server was stopped fired as soon as it started again, with `deadline passed while the server was down`.

### PR2: node monitors and awaiting

Built 2026-10-29 on `fork/050-agent-waits-node`. Open for PRs 3 and 4 (readers, then run supervision and the graph). How the build settled what the design left open is in [assets/050-waits-design.md](./assets/050-waits-design.md#changes-made-while-building-pr2-2026-10-29).

- **Node waits.** `wait_sources` is `["node"]`. `--on node:<agent>` takes an agent id, name or pane id and keeps the id. `--until` is a task status, `complete` by default. Reaching it satisfies the wait and tells the waiter (`wait <id> satisfied: …`). A node that reports `complete` or `failed` instead, or closes, fires the wait to its decider. A wait on a node that has already closed, or already got there, settles when it is declared. Both messages read the node again. A status report touches only the waits on that agent, through an index; closes ride the identity sync.
- **Awaiting.** While an agent has a live wait, its idle turn reads `awaiting`, from a flag on the terminal that follows the store and is checked against it in `AppState`'s invariants. The wait ending is no completion, so a fire does not also ping `done`.
- **Validated** by unit tests (the node index, `until`, outcomes, satisfy, fire on another end, close, settling at declare, refusals, awaiting from declare, cancel, fire and load) and live on a throwaway session. A node reporting `complete` satisfied its wait. Closing the node fired one wait and satisfied the one `--until closed`. An unknown id fired at once. A node wait survived a restart and fired on a later `failed`. A turn that ended with a live wait read `awaiting`, then `idle` (not `done`) once the wait fired.

### PR3: plugin readers

Built 2026-10-29 on `fork/050-agent-waits-readers`. Open for PR4 (run supervision and the wait-for graph). How the build settled readers is in [assets/050-waits-design.md](./assets/050-waits-design.md#changes-made-while-building-pr3-2026-10-29).

- **Readers.** A plugin declares `[[readers]] kind command interval`, and herdr reads every live wait on that kind at declare, then at the interval. Each read is `command … <kind>:<target>` under loom 091's contract, in its own process group (a kill-on-close job on Windows), with a 30 s timeout. A state equal to `until` satisfies the wait and tells the waiter. Three failed reads in a row fire `failed: source unreachable`, an exit 2 fires at once, and no read landing within three intervals fires `failed: reader dead`. The wait keeps `last_state`, `last_ok_at_ms`, `errors` and `last_error`.
- **Discovery.** `wait_sources` stays `["node"]`, since capabilities are fixed when the server starts. Servers with readers advertise `wait_readers`, and `plugin.list` shows each plugin's `readers`.
- **A PR1 test race, fixed.** The `due_wait` test helper set a deadline 1 ms before now, which reads as missed while the server was down when the fixture started in the same millisecond. That flaked `a_parentless_parent_wait_flags_the_user_until_the_waiter_reports` on ubuntu CI (#41) and `a_due_wait_fires_a_tell_to_the_waiter_through_the_message_store` locally. The helper now uses the start itself, and the load test sets its start past the saved deadline.
- **Validated** by unit tests: the read contract, the schedule, a real read (one whose background child holds stdout still returns at once) and a timed-out one whose descendants die with it, manifest parsing, declare refusals, satisfy, source unreachable, refused, reader dead with a hung read and with no reader, and the schedule rebuilt after a load. Live on a throwaway session with a fake `pr` reader at a 10 s interval, it satisfied a wait within one interval of the state flipping, fired `source unreachable` after three failed reads and a refusal at once, and re-read a live wait at once after a restart. Disabling the plugin stopped reads and refused new `pr` declares. The 15-minute orphaned window was not waited out live.

