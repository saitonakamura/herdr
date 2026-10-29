# 050 agent waits: design for the first PR

Drafted 2026-10-27 by the herdr-fork cockpit (personal session); the user settled the open points on 2026-10-29. The spec is
[050](../050-agent-waits.md), which mirrors dotfiles loom 089. The semantics come
from loom 080's resolution.

## PR split

1. **Records, API, persistence, deadlines.** This document.
2. **Node monitors** (`on = node:<agent>`), plus `awaiting` derived from a live wait.
3. **Reader hooks** (`[[readers]]`), liveness, and `mr:` / `pr:` / `ci:` sources.
4. **Run supervision** (`run:`), and the wait-for graph with its cycle check.

Each PR advertises what it adds (below), so loom can probe for every step on its own.

## Model

Pure data in `src/app/waits.rs`, beside `src/app/messages.rs` and in the same
shape: a `WaitStore` persisted in the session snapshot as an optional field
(`#[serde(default, skip_serializing_if = "WaitStore::is_empty")]`), so a snapshot
without it still loads.

```rust
struct Wait {
    id: u64,                    // store-assigned, only grows, like message ids
    waiter: AgentId,            // the agent that declared it
    on: Option<WaitSource>,     // None = a plain deadline; PR1 accepts only None
    until: Option<String>,      // the source state that satisfies it, interpreted per kind
    summary: String,            // what it waits for, 1..=200 chars, shown everywhere
    deadline_at_ms: u64,        // unix epoch ms; required (080 Q2)
    then: Decider,              // self | parent | user; required, no default (080 Q2)
    extensions: u8,             // 0..=2
    rearm_of: Option<u64>,      // the wait this one re-arms; carries extensions forward
    created_at_ms: u64,
    state: WaitState,           // live | fired { at_ms, reason, to } | satisfied { at_ms } | cancelled { at_ms }
    last_ok_at_ms: Option<u64>, // readers, from PR3
    errors: u32,                // readers, from PR3
}

struct WaitSource { kind: String, target: String }  // "node" + agent id, "mr" + ref, "run" + exit file
```

- `on.kind` is a string, not an enum, so a source added later never touches a
  type that has already shipped. The server refuses a kind it does not support,
  with `wait_source_unsupported`.
- Limits: 32 live waits per waiter and 1024 in a session. Finished waits are kept
  for `wait list` up to a history cap of 500, like delivered messages, and live
  ones are never evicted.

## Firing

- **Deadlines.** `next_headless_loop_deadline_with_git_refresh` gains
  `self.state.next_wait_deadline()`. The store keeps a `BTreeSet<(deadline_at_ms, id)>`
  index over live waits, so the next deadline is O(1) and firing is O(waits due).
  Epoch ms are converted to `Instant` against `SystemTime::now()` once per loop.
- **A deadline missed while the server was down** fires on the first loop after
  load, with reason `deadline passed while the server was down`.
- **What a fire does.** The wait goes to `fired`, and herdr queues a message to the
  resolved decider (below) through the existing message store, so batched
  delivery (051) and `--reply-to` work unchanged. The message is a `tell` from
  `None` with `from_name = "herdr wait"`, summary `wait <id> fired: <reason>: <summary>`,
  and a new optional `wait_id` on `MessageInfo`.
  `MessageKind` gains no variant (decision 2).
- **Resolving the decider happens at fire time**, not at declare time, so a
  Parent that changed since is honoured. `self` is the waiter. `parent` is the
  waiter's Parent, or the user when it has none. `user` sets a separate flag (decision 1).

## Extensions and escalation (080 Q2)

- `wait extend <id> --deadline …` moves a live wait's deadline and counts one
  extension. The third is refused with `wait_extension_limit`.
- A wait that fires with its extensions spent goes to its decider **and** one
  level up (self → parent → user). Escalation needs no extra call.
- **Re-arming.** Declaring with `--rearm <fired id>` makes a new wait (a new id,
  per 089) that inherits the old one's `extensions` and `then`. Without that, a
  re-arm would reset the count and the bound would mean nothing.

## Lifecycle

- **The waiter closes:** its live waits go to `cancelled` and nothing fires
  (decision 3).
- **The waiter's pane moves, `/clear`s, or survives a restart or live handoff:**
  its waits stay, because the waiter is an `AgentId`, which survives all of these (010).
- **Cancelling:** `wait cancel <id>`, allowed for the waiter, its Parent or the user.

## API and CLI

New methods; none of them changes an existing one.

| method | params | result |
|---|---|---|
| `wait.declare` | `waiter`, `summary`, `deadline_at_ms` or `deadline_in`, `then`, `on?`, `until?`, `rearm?` | `wait_info` |
| `wait.list` | `waiter?`, `state?` (default live) | `wait_list` |
| `wait.get` | `wait_id` | `wait_info` |
| `wait.extend` | `wait_id`, `deadline_at_ms` or `deadline_in` | `wait_info` |
| `wait.cancel` | `wait_id` | `wait_info` |

- CLI: `herdr wait declare <waiter> --summary … --deadline 30m --then parent`,
  `herdr wait list [--waiter X] [--all]`, `herdr wait show|extend|cancel <id>`.
  `--deadline` takes a duration (`90s`, `30m`, `2h`) or an RFC 3339 time.
- **Events.** `wait.declared` / `wait.fired` / `wait.finished`, published like
  `message.*`. Agent info gains optional `live_waits: u32`, which a reader like
  the sidebar needs without a second call.
- **Capability.** `agent_waits` in `herdr status server --json`, plus
  `wait_sources: []` (PR1). PR2 adds `"node"`, PR3 the reader kinds, PR4 `"run"`.
  loom probes the list rather than a server version.
- **Stable endpoint.** JSON API only. No codec, no generation-1 fixture and no
  `PaneLinkActivate`-style reinterpretation is touched. Every new field is optional.

## Tests

- `WaitStore` unit tests: id growth, the index staying consistent across
  declare, extend, cancel and fire, the limits, history eviction never touching a
  live wait, and re-arm inheriting extensions.
- `AppState::test_new()` tests: a fire queues the message to the resolved
  decider, escalation after two extensions, a fire after load for a missed
  deadline, and a waiter close cancelling.
- A snapshot roundtrip with and without the field, and an old snapshot loading.
- A live smoke test on a throwaway session: declare a 5 s wait, see the message
  arrive, restart the server mid-wait and see it fire on load.

## Decisions (user, 2026-10-29)

1. **`then user` sets a separate flag, never `task_status`.** Agent info gains
   optional `wait_needs_user: { wait_id, reason }`, and the sidebar's questions
   section (047) shows it. It clears when the wait is cancelled or re-armed, or
   when the waiter reports again. The waiter's `task_status` stays its own report
   (loom 078 is what a second writer costs).
2. **A fire is a `tell` with an optional `wait_id`, for now.** `MessageKind` has
   no `Unknown` fallback, so a new `kind: wait` would break older clients. Revisit
   it once `MessageKind` gains a fallback.
3. **A waiter that closes cancels its live waits silently.** A parent that needs
   to know when a child closes declares a wait on that child (PR2).

## Changes made while building PR1 (2026-10-29)

Where the build settled something this document left open, or departed from it:

- **`deadline_in` is `deadline_in_ms`**, an integer of milliseconds. The CLI parses
  `90s` / `30m` / `2h` / `1d` and RFC 3339 itself and sends one of the two fields,
  so the server never parses durations. A deadline that is not in the future is
  refused (`invalid_wait`), and so is `until` without `on`.
- **`then` may be left out on a re-arm**, which inherits it. Giving a different one
  is refused rather than silently ignored. Only a fired wait of the same waiter can
  be re-armed.
- **`extend` and `cancel` take an optional `by`**, the asker, and both allow only
  the waiter, its current Parent or the user. Omitting `by`, or naming a pane with
  no agent, asks as the user (that is a person at a shell). The CLI defaults `by`
  to the calling pane, like `message send --from`, and `--user` overrides it.
  The design said this only of `cancel`; giving `extend` the same rule keeps an
  unrelated agent from spending another agent's extensions.
- **A fire to the user sends no message**: herdr has no user inbox, so the
  `wait_needs_user` flag is the whole fire, and `fired.to` records `user`.
  `wait cancel` on a fired wait that still needs the user clears the flag and
  leaves it `fired`. `wait_needs_user` shows the waiter's newest such wait.
- **`wait.finished` also fires when a waiter's close cancels its waits.** Nobody
  is messaged (decision 3), but an event reader still learns the wait ended.
  `extend` publishes no `wait.*` event. Every change republishes the waiter's pane
  (`pane.updated`), since `live_waits` or `wait_needs_user` moved.
- **The missed-deadline reason is decided by comparison**: a deadline before the
  moment this server built its state (`server_started_at_ms`, wall clock) passed
  while it was down. A live handoff counts as down for the milliseconds it takes.
- **Waits are not layout.** `layout_fingerprint` drops them as it drops messages,
  or every declare would orphan the saved pane history.
- **`herdr wait` is a top-level group**, mirroring `wait.*`. Upstream's
  `spec_matches_refactored_agent_and_pane_commands` asserted there is no top-level
  `wait`, to keep the old agent-status wait out after it moved under `agent`. The
  fork narrows that assertion to "the top-level `wait` has no `--until` agent
  status and has `declare`", which still guards the regression it was written for.
  The Linux-only `tests/cli/surface.rs` check that `herdr wait output …` is refused
  now expects clap's `unrecognized subcommand 'output'` instead of `unknown command: wait`.
- **`time` gains its `parsing` feature** for RFC 3339 deadlines. No new crate.

## Changes made while building PR2 (2026-10-29)

PR2 adds `node` to `wait_sources`, and `awaiting` derived from a live wait. The design
above said only "fires on its terminal task status or on close"; this is how the build
settled it:

- **The node is kept by agent id.** `on.target` may name an agent id, an agent name or a
  pane id; declare resolves it once and stores the id, so a rename or a move changes
  nothing. herdr keeps no record of closed agents, so a well-formed agent id that names no
  agent counts as a node that has closed. An agent cannot wait on itself.
- **`until` is a task status**: `complete` (the default), `failed`, `closed`,
  `needs_parent` or `needs_user`. `starting`, `running` and anything else are refused with
  `invalid_wait`. loom's `done` is herdr's `complete`, which loom already translates.
- **Reaching `until` satisfies the wait and tells the waiter**, whatever `then` says: the
  decider is for a wait that did not end well, and satisfaction is the waiter's own news. It
  is a `tell` from `herdr wait`, `wait <id> satisfied: <reason>: <summary>`, with `wait_id`,
  and `wait.finished` carries the satisfied wait.
- **Any other end fires the wait to its decider**: a terminal status that is not `until`
  (`complete` or `failed`) as `node <id> reported <status>`, a close as `node <id> closed`.
  A `needs_*` status that is not `until` leaves it live. The deadline, extensions and
  escalation apply to node waits unchanged.
- **A wait settles at declare** when its node has already closed or already reports a status
  that settles it. The `wait.declare` response then carries the settled state, after a
  `wait.declared` event.
- **Both messages restate what was awaited and read the node again** (080's "fresh read"):
  the body names the node and its task status now, or `closed`.
- **Cost.** The store indexes live node waits by node, so a status report touches only the
  waits on that agent. Closes ride the identity sync, which already runs when an agent goes
  away and after a load or handoff; it visits the watched nodes, not every wait. When the
  waiter and its node go together, the waiter's cancel runs first, so nobody is told.
- **`awaiting` from a wait is a flag on the terminal**, set from the store whenever a
  waiter's waits change and re-derived by the identity sync after a load or handoff. While
  it is set, the effective state turns `idle` into `awaiting`. `working` and `blocked`
  outrank it, and a pane with no agent state stays `unknown`. `AppState`'s invariants check
  the flag against the store.
- **A wait-driven `awaiting` ending is not a completion**, unlike the sentinel's (028): no
  done sound or toast, and the pane does not turn `done`. The wait's own message or
  `wait_needs_user` is the news; a completion on top would ping for every fire, just before
  the message wakes the agent.

