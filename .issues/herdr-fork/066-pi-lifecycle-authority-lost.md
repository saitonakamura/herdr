---
id: herdr-fork-pi-lifecycle-authority-lost
title: A pi pane loses lifecycle authority silently and reads idle while it works
status: in-progress
labels:
  - wayfinder:research
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/wN/pi-migration-cockpit (loom 5454fa09)
---

## Question

Why did a running pi pane lose its state reporting, and does herdr need a separate
lifecycle-authority claim at all? The user is investigating this interactively from
the pt:pi-migration cockpit, which needs correct pi states for its
[staged acceptance](https://github.com/saitonakamura/dotfiles/blob/master/.issues/pi-migration/009-staged-pi-only-acceptance.md).

## The bug

On 2026-11-05 at about 12:48 local, loom node `0a32ecb6` in pane `wN:p9` showed pi's
`Working` spinner while herdr reported `idle`. The turn ran on for several minutes,
and the state never corrected itself. `herdr agent explain` on the three pi panes:

| pane | shown | `agent explain` |
| --- | --- | --- |
| `wN:p9` | `idle`, actually working | `rule: none`, `fallback_reason: default_known_agent_idle_fallback` |
| `wN:pM` | `working`, correct | `screen_detection_skip_reason: full_lifecycle_hook_authority` |
| `wN:pA` | `idle`, correct | `screen_detection_skip_reason: full_lifecycle_hook_authority` |

So `wN:p9` has no effective authority. Its extension keeps reporting through
`scripts/herdr-bridge`, and a report that doesn't count still gets `ok`, per the
bridge's own comment, so neither side sees a failure. `pi.toml` has no rule that
matches the spinner, so the fallback is `idle`. While a pane misreads this way,
herdr treats it as finished and may deliver queued messages mid-turn.

## What isn't known

Not proven: nothing yet shows which of these lost `wN:p9`'s authority.

- **Server restart.** Authority lives in server memory, and the servers restarted
  at 12:05–12:06 that day. `scripts/herdr-bridge`'s `AgentReporter` caches
  `claimed_session` and re-claims only when the session id changes, so a bridge
  that outlived the restart would never claim again. But `wN:p9`'s bridge started at
  12:06:17, after that server's 12:06:13, so it didn't outlive this restart: this
  cache can't be the cause here, whatever else a restart does.
- **`hook_authority_is_effective`** (`src/terminal/state.rs:2299`) also needs
  `detected_agent` to be pi and no `recent_agent_process_exit`. Authority could be
  present but ineffective.
- **The session.** The user typed into `wN:p9` during a manual batch. A session
  change is only accepted for `herdr:pi` with source `new`, `resume` or `fork`
  (`session_report_allows_session_replacement`, `state.rs:1641`).

## Findings so far (read-only, 2026-11-05)

`wN:p9` is the only live host-pi pane that herdr restored at the 12:06 restart: its
launcher runs `pi-host --session 01a1a1d1-…`, started 12:06:15. Every pane launched
fresh afterwards reads `full_lifecycle_hook_authority`: `wN:pK`, `wN:pM`, `wN:pN`,
`wN:pR`, `wM:p1T`. That's a lead, from one restored pane, not yet a reproduction.
`wN:pA` was closed before it could be compared, so drop it as a counterexample.

The claim's purpose is in `src/terminal/state.rs:1716-1840`: it's a **generation
gate**. Once a full-lifecycle source's process exits, its reports are suppressed
(`suppressed_full_lifecycle_hook_reports`), so a late hook from a dead process can't
revive the pane. Only a report with a recognized `session_start_source` and a newer
`seq` reopens it. If the process isn't detected yet, that report is recorded as a
replacement and returns `None`. A plain state report carries no start source, and
the bridge sends its claim once per session, so if this gate stays shut nothing
reopens it. Not yet checked: whether that's what happened to `wN:p9`.

## The design question

The user's position: herdr already knows how to behave from the agent, through the
static tiers in `src/detect/mod.rs:359-385`, so why have a runtime authority at all?

What does need a runtime answer is whether a live reporter exists in this pane:
config can't say, because host pi loads the extension from `~/.pi-personal` through
a per-session bridge, and an agent can die mid-state. But a `herdr:pi` report naming
a session is already evidence of a live reporter, so it might establish authority
itself, or be rejected visibly instead of `ok` and dropped. Find what the separate
claim protects, such as stale reports from an old session, and whether anything does
need it. The generation gate above is one answer; open is whether liveness
(`process_present` plus a matching session) could replace the handshake.

## Parent

[Fork map](./000-map.md)

## Blocked by

Nothing.
