# 050 agent waits: design for the first PR

Draft, 2026-10-27, by the herdr-fork cockpit (personal session). The spec is
[050](../050-agent-waits.md), which mirrors dotfiles loom 089. The semantics come
from loom 080's resolution. Everything marked **open** is waiting on the user.

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
  `MessageKind` gains no variant (**open 2**).
- **Resolving the decider happens at fire time**, not at declare time, so a
  Parent that changed since is honoured. `self` is the waiter. `parent` is the
  waiter's Parent, or the user when it has none. `user` is **open 1**.

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
  (**open 3**).
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

## Open (the user decides)

1. **What `then user` does when it fires.** 080 Q6 says "a `needs_user` flag with
   its reason". The proposal: it does **not** overwrite the waiter's `task_status`,
   which is the agent's own report (loom 078 shows what a stale `needs_user`
   costs). The fired wait stays visible instead: agent info gains optional
   `wait_needs_user: { wait_id, reason }`, the sidebar's questions section (047)
   shows it, and it clears when the wait is cancelled or re-armed, or the agent
   reports. The alternative: set `task_status = needs_user` and let loom 078's fix
   clear it.
2. **How a fire is delivered.** The proposal: a `tell` plus a `wait_id` field. The
   alternative, a new `MessageKind::Wait`, reads better, but `MessageKind` has no
   `Unknown` fallback, so an older client listing messages would fail to parse it.
3. **A waiter that closes with live waits.** The proposal: cancel them silently.
   The alternative: fire them to the decider with reason `waiter closed`, which
   tells a parent that a child it was counting on is gone. Node monitors (PR2)
   cover that case from the parent's side anyway.
