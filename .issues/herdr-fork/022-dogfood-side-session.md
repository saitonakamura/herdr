---
id: herdr-fork-dogfood-side-session
title: Dogfood the fork in a side session on the primary mac
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w6:p7/887cbe33-85f2-4472-9564-edde9c7fabab
---

## Question

The user wants to dogfood the fork as early as possible, in a side session first
(decided 2026-10-16): real work in a separate named session running a fork binary,
while the personal, work and default sessions stay on Homebrew's herdr 0.9.4 until
the tap cutover ([003](./003-tap-and-cutover.md)).

Deliver a repeatable way to run it on the primary mac:

- A fork binary at its own path (e.g. `~/.local/opt/herdr-fork/bin/herdr`), never
  `/opt/homebrew/bin/herdr`. Source is either a fork release asset (verify against
  `SHA256SUMS`) or a local release build of `master` plus named unmerged PR branches,
  so green PRs can be dogfooded before the user merges them. Copy the binary in;
  never point at `target/`, which a rebuild would swap under the running server.
- A launcher the user runs in a new kitty tab, which starts or attaches the side
  session with that binary on PATH first, so agents, loom and the hooks inside its
  panes call the fork CLI. The other sessions must be untouched: their servers,
  sockets, persisted state and PATH.
- Loom inside the side session must not mix its registry, inboxes or names with the
  personal lane. Find out how loom derives its lane and make the side session get
  its own, or show that it already does.
- The engine-4 detection override is on the dotfiles branch
  `herdr-fork/005-state-consistency` (409bf92: `claude.v4.toml` beside an unchanged
  `claude.toml`). Merge it into the dotfiles master if the side session needs it and
  the 0.9.4 sessions are verifiably unaffected.
- Document the side session in `docs/setup/herdr.md` (primary-mac section) in the
  dotfiles, plus a short `FORK.md` note on how to refresh the dogfood binary.

Start with the published `v0.9.5+fork.1`. Then build master + PR #4 + PR #5 (green,
not merged, trial-merged cleanly 2026-10-16), so the read-layer and state fixes get
dogfooded.

Verify in the side session: `herdr --version`, `herdr config check`,
`herdr server agent-manifests`, `loom doctor`. Also a claude pane's state going
working, blocked and done, and `herdr agent read --source screen` on the local build.
Then hand the launcher to the user. Starting and stopping the side session's own
server is in scope. Touching any other session's server is not.

## Parent

[000-map](./000-map.md)
