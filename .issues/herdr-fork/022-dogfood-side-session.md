---
id: herdr-fork-dogfood-side-session
title: Dogfood the fork in a side session on the primary mac
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/887cbe33-85f2-4472-9564-edde9c7fabab
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

## Resolution

[PR #6](https://github.com/saitonakamura/herdr/pull/6), merged 2026-10-16 as `c98acac6` (`scripts/dogfood.sh` + FORK.md § Dogfooding).

- **Binary**: `~/.local/opt/herdr-fork/bin/herdr`, installed by `scripts/dogfood.sh release|build|status`
  (copy + rename, never `target/`, never `/opt/homebrew`). Installed now: `build 4 5` =
  origin/master@2d4c4ada + fork/004-read-layer@45c0f1f3 + fork/005-state-consistency@f45dcbe2.
  `--version` reads `0.9.5+fork.1` either way; `dogfood.sh status` tells builds apart.
- **Launcher**: `hf` in the dotfiles `.zshrc` (07c89e2) attaches session `fork` with that binary.
  It exports `HERDR_DOGFOOD_BIN`, which the end of `.zshrc` re-prepends in the session's panes,
  because `.zprofile`'s `brew shellenv` puts `/opt/homebrew/bin` back in front. Other sessions
  never set it, so their PATH is unchanged.
- **loom lane**: already separate. loom names the lane after the session owning
  `$HERDR_SOCKET_PATH`, so `loom doctor` in a `fork` pane says `lane: fork`. Efforts still file
  under `pt:`.
- **Override**: dotfiles `herdr-fork/005-state-consistency` merged into master (4475490). A
  throwaway 0.9.4 session still loaded `claude.toml` 2026.08.13.3, and the fork build loads
  `claude.v4.toml` 2026.10.15.2.
- **Verified in `fork`**: `herdr config check` ok, `server agent-manifests`, `loom doctor` 15 ok,
  and a claude pane went working → blocked (`awaiting_user_answer`) → done. `agent read --source
  screen` works, and the brew CLI's `reload-agent-manifests` reaches the fork server.
- **Shared with the 0.9.4 sessions**: the config root and the remote-manifest cache/status in
  `~/.local/state/herdr` (the fork server refreshes the same upstream catalog; see 021).
- **Gotcha**: a `fork` server started with a bare PATH runs loom's startup hook under
  `/bin/bash` 3.2, which fails to parse `restore.sh`. `hf` from a kitty tab is fine.
