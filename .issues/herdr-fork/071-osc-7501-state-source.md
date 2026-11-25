---
id: herdr-fork-osc-7501-state-source
title: OSC 7501 records decide an agent pane's lifecycle state
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-osc-7501-parse-and-answer
assignee: saitonakamura
claimed_by:
---

## Question

Once [070](./070-osc-7501-parse-and-answer.md) keeps each pane's OSC 7501 records, make the root record
decide the pane's lifecycle.

Why this matters:

- Upstream's screen rules have stopped moving ([067](./067-herdr-app-and-core-comparison.md)).
- A report written down the pane's own PTY comes from that pane's process by construction. It needs no
  claim, no generation gate and no bridge from the sandbox, and that removes the failure class behind
  [066 pi authority lost](./066-pi-lifecycle-authority-lost.md).

## Ranking (user, 2026-11-24)

**7501 decides the lifecycle:** working, blocked with a kind, done, idle, and cleared. It outranks screen
rules and the working and idle that hooks report.

**Hooks and the pi bridge stay as a layer on top**, for what 7501 can't say:

- **The sentinels.** A turn ending on `⏳ awaiting your answer` is blocked on `question`, and one ending on
  `🔄 awaiting background work` is `awaiting`. 7501 calls both of those `done`, so the sentinel refines a
  7501 `done` or `idle`; it doesn't compete with it.
- **Session ids** for resume (the claim and `pane.report_agent_session`).
- **pi's `background` flag** (loom 128), **loom's gate approvals and sentinel blocks**, which are custom
  `herdr:blocked` events (`pi/extensions/loom.ts:204`, `pi/host/gate.ts:98,336`), and **`form`**. 7501 has
  no kind for form.

**Screen rules stay** as the fallback for agents without 7501, and for a pane whose record was cleared.

Where it goes:

- `effective_state_and_blocked_reason` (`src/terminal/state.rs:2410`) and the source tiers in
  `src/detect/mod.rs:359` (full lifecycle) and `:373` (screen-assisted `herdr:claude`).
- With a 7501 record present, Claude's 3 s screen-idle hold (`SCREEN_IDLE_ENDS_LIVE_TURN_AFTER`,
  `state.rs:15`) has nothing to end: Esc gives an `idle` report at once. Today that takes about 4.5 s
  ([dotfiles 001](https://github.com/saitonakamura/dotfiles/blob/master/.issues/herdr-fork/001-claude-hooks-macos.md#resolution)).

## Mapping

| 7501 | herdr |
| --- | --- |
| `working` | working |
| `blocked` + `permission` / `question` | blocked, `Permission` / `Question` |
| `blocked` + `auth` | blocked, **new `Auth` reason** |
| `blocked` with no kind (Claude's open dialog) | blocked, no reason |
| `done` | done |
| `idle` | idle |
| `error` | herdr has no error state. Take `done` and keep `msg` as the message |
| `clear` | drop to the next source |

- **`Auth` is a new variant (user, 2026-11-24: backwards compatibility is not a concern for this fork).**
  - Add it to `detect::BlockedReason` (`detect/mod.rs:36`) and `AgentBlockedReason`
    (`src/api/schema/common.rs:276`), and to the sidebar's `blocked_word` as "login"
    (`tools/sidebar/src/model.rs:126`). The sidebar already maps reasons it doesn't know to "blocked".
  - This knowingly departs from CLAUDE.md's append-closed rule for enums a gen-1 codec reaches
    (`PaneInfo`). Say so in `FORK.md`.
  - Persisted snapshots will carry `auth` too.
- **Claude after a turn, with background tasks running**, reports root `working` with a `msg`, plus child
  records. When the Stop hook has fired and says background (`🔄` or a running sub-agent), keep reading it
  as `awaiting`, as today.

## Done

- **Engine tests** for the ranking:
  - 7501 over screen and over hook working/idle
  - a sentinel refining a 7501 `done`
  - `clear` falling back
  - every mapping row, `Auth` included
  - a record outliving a process (dropped per 070)
- **Live smoke tests per CLAUDE.md** for Claude, host pi, sandbox pi and Codex, recording versions and
  signals. Cover:
  - idle, working, blocked/permission, blocked/question
  - pi login → `Auth`
  - background → `awaiting`
  - Esc → idle, with its delay
  - exit
- **Docs:**
  - `FORK.md` describes the authority order and the `Auth` departure
  - the dotfiles `docs/setup/herdr.md` agent-integration section says 7501 now decides state
- **Follow-up filed:** the dotfiles ticket that retires pi's bridge state reports
  ([dotfiles herdr-fork 019](https://github.com/saitonakamura/dotfiles/blob/master/.issues/herdr-fork/019-pi-bridge-state-from-7501.md)).
- **Review:** the PR touches server state, so it comes to the cockpit for review.

## Resolution

Merged 2026-11-25 by the user: [saitonakamura/herdr#67](https://github.com/saitonakamura/herdr/pull/67)
as `d8b84e96` (head `3fc99d23`), every CI check green after a macOS rerun. Not deployed: the dogfood
build and fork.4 carry it, and the dotfiles `docs/setup/herdr.md` agent-integration paragraph is written
with that release.

- **Order:** restored status, then the 7501 root record, then hooks, then screen rules. `FORK.md`
  § Agent state authority has the full rules; `src/terminal/program_status.rs` has the code and the
  engine tests.
- **Path:** a per-pane watcher task sends the root record to `TerminalState` on every root report,
  apart from the 300 ms detection tick. `agent explain` shows the decision as `lifecycle`.
- **`Auth`** is appended to both reason enums; the sidebar says "login"; `agent prompt` answers
  `agent_blocked_auth`; `pane report-agent --blocked-reason auth` exists for the bridge.

Live, in a throwaway debug session with bundled manifests (2026-11-24): Claude Code 2.1.331, Codex
0.166.0, host pi 1.2.0, sandbox pi 1.1.0.

| | Claude | Codex | host pi | sandbox pi |
| --- | --- | --- | --- | --- |
| idle at start | hook (no record until the first turn) | record | record | record |
| working, done | record | record | record | record |
| permission | record, 6 s before the hook | record | not reached | not reached |
| question | `⏳`: record + hook; `AskUserQuestion`: `form` | not tried | `⏳`: record + hook | `⏳`: record + hook |
| background | `🔄`: awaiting; shell left running: idle | n/a | `🔄`: awaiting | `🔄`: awaiting |
| Esc to idle | 136 ms (was 4.4 s) | 50 ms | 68 ms | 233 ms |
| exit | record cleared, pane released | same | same | same |

What the ticket got wrong or left open:

- **"7501 outranks hook idle" cannot hold for a record left `working`.** Claude keeps root `working`
  (`msg` "1 shell still running") after a turn that left a shell running. A hook report that ends a
  turn, made since the record's turn began, stands over it; otherwise a parked dev server holds the
  pane at working. Codex has no state hooks: interrupted with a terminal still running, it reads
  working until that terminal ends.
- **`kind=question` is not always herdr's `Question`.** Claude reports its `AskUserQuestion` dialog as
  `question`. It reads `form` when a hook names a form or permission dialog or the screen rule names
  a form, so `agent prompt` does not type into it. Claude's late `permission_prompt` notification for
  the same dialog no longer flips it to `permission`.
- **`done` and `idle` both read idle.** herdr's done is an unseen idle, as before.
- **Claude reports no record until its first turn**, and its `/exit` dialog is `blocked` with no kind
  only some of the time; the other time it reads idle, as it did under an idle hook.
- **`auth` was not seen live.** pi's `/login` picker reports nothing, and going further starts a real
  sign-in. A gate approval over a `working` record was not reached either. Engine tests cover both.
- **Title rules:** `claude.toml` is unchanged. Its two title rules only match a Claude that never got
  the answer, and nothing depends on them otherwise.
- **Restore and handoff:** records are not carried; the pane runs on hooks and screen rules until
  the next report.
- **OSC 9;4** is read as empty once the pane's program has sent any 7501 report.
- A wrong first process match (host pi read as `claude` for 5 s, once) drops the record pi sent at
  startup when it is corrected; the pane runs on hooks until pi next reports.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-osc-7501-parse-and-answer
