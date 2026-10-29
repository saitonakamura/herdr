---
id: herdr-fork-pi-sandbox-restore
title: Sandboxed pi panes lose their agent across a server restart
status: closed
labels:
  - wayfinder:research
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/cf45927f-48be-411e-b901-c5fc907ce3c3
---

## Question

Seen on 2026-10-26 when `work` moved from stock 0.9.4 to the fork (`hw --fork`).
Seven work panes running pi in the container (`scripts/pi-sandbox`, which reports
to herdr through `scripts/herdr-bridge`) came back as bare shells. Every claude
and codex pane came back.

- The stock server's last snapshot
  (`~/.config/herdr/sessions/work/session-snapshots/*-2169-0.json`, 09:24) holds
  only `cwd` for those panes, with no `agent_session` and no `agent_resume`. So a
  stock restart would have lost them too. Loom's registry still had each pi
  session id.
- `loom resume <node>`, typed into each pane by hand, brought all seven back.
  Afterwards the fork reports `agent_session` `herdr:pi` for each of them, and
  the fork's `session.json` holds all seven.
- On the fork, loom's `apply_resume` leaves restore to herdr (`herdr_resumes`).
  On stock it writes no command for pi, because `herdr:pi` owns the pane.

Find out:

1. Why did stock 0.9.4 save no session for those panes? Did the bridge's
   `pane.report_agent_session` never land, or was it dropped when the snapshot
   was written?
2. On the fork, does a restart now bring a sandboxed pi back? Which command does
   herdr type, and does it reach the container through the `pi` wrapper with the
   right profile and cwd? Test this in a throwaway session, never on `work`.
3. If either is broken, what is the fix, and where does it go: herdr, loom or
   the bridge?

## Parent

[000-map](./000-map.md)

## Resolution

Nothing in herdr needs to change; no PR. Measured on 2026-10-29 in throwaway
sessions, never on `work`.

**1. Stock saved what it was given.** On stock 0.9.4 (`/opt/homebrew/bin/herdr`,
session `t044-stock`), a process shaped like the sandbox's (a waiting bash run as
`exec -a pi`) plus the two calls `scripts/herdr-bridge` makes produced
`agent_session {herdr:pi, kind: id}` in `session.json`. That held in every
variant tried: a `startup` claim, a `resume` claim, pi exiting and coming back
in the same pane with the same id, a transient `claude`-named process in the
foreground group, and `server stop`. Stock 0.9.4 also has the same restore plan
as the fork (`pi --session <id>` for an id-kind `herdr:pi` session). So the
seven panes came back as shells because no claim was held when stock saved, not
because stock dropped or failed to restore one. This is the same class as the
personal codex panes (no session report reached herdr), not something specific
to pi.

What dropped the claims can't be recovered: no stock-era server log survives,
and the 09:24 file is weaker evidence than it looks. `session-snapshots/` is a
backup that herdr writes only when the layout fingerprint changes, at most every
15 minutes (`src/persist/writer.rs`). It is not a record of what stock held at
shutdown. The seven pi sessions dated from 2026-09-04 to 10-23 (their UUIDv7
timestamps), so they lived through earlier stock restarts.

One gap in the bridge explains how such a loss becomes permanent.
`AgentReporter` sends `pane.report_agent_session` once per session id
(`claimed_session`). herdr's own pi extension
(`src/integration/assets/pi/herdr-agent-state.ts`) sends it again on every
`agent_start`. Once herdr drops a pane's authority while pi keeps running, a
sandboxed pi never claims again. Its state reports are still answered `ok`, but
herdr ignores them, and the next save has no session. Before re-claiming, note
that herdr accepts a re-claim with no start source, which is what upstream sends,
only while the session is still anchored. Once authority has been dropped, it
also needs a recognized start source (`set_agent_session_ref_for_session_start`),
which the bridge already sends. That would be a dotfiles change, and is not made
here because the drop itself could not be reproduced.

**2. The fork brings a sandboxed pi back.** The fork 0.9.5+fork.2 binary ran in
session `t044-fork` with `resume_agents_on_restore = true`, using a pane in
`~/code/mayflower/battleship`. There, `agent start --kind pi -- --local` started
the real sandbox, and its claim was saved. After `server stop` and a start,
herdr typed `pi --local --session <id>` into the fresh login shell after direnv
had loaded the work root. That ran `scripts/pi-sandbox --session <id> --local`
with `CLAUDE_CONFIG_DIR=~/.claude-work`, the restored pane's `HERDR_PANE_ID`, the
session's socket and the saved cwd. The conversation came back, and the new
container claimed the pane again as `herdr:pi`. The live `work` session agrees:
all five sandboxed pi sessions survived the fork restarts of 10-26 20:24, 10-27
and 10-29 14:12, each restored as `pi-sandbox --session <id>`.

One trap when testing: pi writes a session file only once the session holds a
message. A sandbox started and stopped without a prompt restores to `No session
found matching '<id>'`.

**3. Fixes.** Nothing in herdr or loom. Optionally, `scripts/herdr-bridge` could
re-claim on every turn, keeping its start source, so that a dropped authority
heals itself the way it does for herdr's own pi. That is worth a dotfiles ticket
only if a sandboxed pane is seen without a session again (`herdr agent get` shows
`agent_session: null` while `screen_detection_skipped` is not true).
