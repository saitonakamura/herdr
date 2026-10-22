# saitonakamura/herdr

A personal fork of [herdrdev/herdr](https://github.com/herdrdev/herdr), shaped to
how its owner runs coding agents. This file **overrides** the Maintainer Workflow,
Local Can Machine Workflow, Release Channels and External contributor guardrail
sections of [`AGENTS.md`](./AGENTS.md). Everything else in `AGENTS.md`
(principles, performance paths, runtime/client boundary, testing, code
conventions, detection, vendored libghostty-vt) still applies.

## Fork-only

- Never open issues, pull requests or discussions on `herdrdev/herdr`, and never
  push there. `upstream` is fetch-only.
- Keep changes small and neutrally named so upstream merges stay cheap.
- Apache-2.0: keep `LICENSE` and the notices, in the tree and in release assets.

## Remotes and branches

- `origin` = `saitonakamura/herdr`, `upstream` = `herdrdev/herdr`.
- `master` is the only long-lived branch: upstream `master` merged in, plus the
  fork's commits. Never rebase or force-push it.
- Task worktrees: `../herdr-worktrees/<slug>`. Task branches:
  `fork/<NNN>-<slug>`, where `NNN` is the ticket number.
- Behaviour changes go through a PR on `saitonakamura/herdr` into `master`. The
  agent stops at a green PR; the user merges.
- Setup, upstream sync merges and tracker-only edits may go straight to `master`.

## Syncing upstream

Merge, never rebase. From a clean `master` in the shared checkout:

```sh
git fetch upstream
git fetch origin
git switch master
git merge --ff-only origin/master
git merge upstream/master          # fast-forwards while the fork has no own commits
just check
python3 scripts/agent_detection_manifest_check.py --require-published
git push origin master
```

- On conflicts, keep the fork's behaviour and upstream's structure; when a fork
  change now exists upstream, drop the fork's copy and say so in the merge body.
- A merge commit's subject: `chore: merge upstream/master (<short-sha>)`.
- After syncing, re-check `vendor/libghostty-vt.patches.md` per `AGENTS.md`.
- The sync is how upstream's detection manifest changes reach fork builds (see
  below), so it must leave `distribution/agent-detection/` current: the check
  above fails otherwise.

## Detection manifests

Fork builds fetch remote detection manifests from the fork, not herdr.dev: the
default catalog is `distribution/agent-detection/index.toml` on fork `master`,
served as raw GitHub files. Pushing to `master` publishes it; there is no
generation step.

- A fork change to `src/detect/manifests/<agent>.toml` bumps its `version` and
  copies the file to `distribution/agent-detection/` in the same PR. The
  `fork-agent-detection` CI job enforces this.
- When an upstream sync conflicts on a manifest both sides changed, resolve it
  in both copies with a version higher than either side's.
- `HERDR_AGENT_DETECTION_MANIFEST_CATALOG_URL` still overrides the catalog, and a
  local override in `~/.config/herdr/agent-detection/` still beats any remote
  manifest.

## Commits

- Upstream's style: lowercase conventional commits, no emojis, no AI co-author
  lines. Write the message and commit; don't stop to propose it first. This
  overrides "propose the commit message and get alignment" in `AGENTS.md`'s
  Maintainer Workflow and Commit Style.
- Instead of GitHub issue refs, put the ticket in the body:
  `refs herdr-fork-<ticket-id>` (for example `refs herdr-fork-sync-and-branch`).
- Run `just check` before committing; never bypass a failing check, fix it or
  say exactly why a narrower check suffices.
- Commits are signed through 1Password; a signing failure means the app is
  locked or not running. Never `--no-gpg-sign`.

## Fork config overlay

Fork builds read an optional `config.fork.toml` beside `config.toml` (beside
`$HERDR_CONFIG_PATH`, when set) and merge it over the base file: tables merge
key by key, while scalars, arrays, sidebar row lists and `[[...]]` arrays of
tables replace. Stock herdr never reads it. A config shared with stock builds
therefore keeps fork-only keys and tokens in the overlay, where they cannot make
stock reject the whole file and fall back to defaults.

- `herdr config check` and config reload check the merged result. An invalid
  overlay is ignored with a diagnostic, and the base file still applies.
- The settings UI and `herdr channel set` still write only `config.toml`. A key
  the overlay also sets keeps the overlay's value.

## The live runtime

herdr runs every agent pane, including the one building it.

- Never replace the installed binary, restart a herdr server, or run `brew`
  against `herdr` without asking the user.
- Test builds against the debug `herdr-dev` server:
  `env -u HERDR_SOCKET_PATH -u HERDR_CLIENT_SOCKET_PATH cargo run -- <command>`,
  or a throwaway named session.
- Detection manifest work: the hot-reload loop in `AGENTS.md` still applies, but
  only against a throwaway session, never the live one.

## Sidebar pane

`[ui.sidebar] pane = [argv]` in `config.fork.toml` makes the server run that
program once in a terminal that belongs to no workspace or tab, and clients draw
it in place of the native sidebar ([ADR 0002](./docs/adr/0002-sidebar-pane.md),
[0005](./docs/adr/0005-collapsed-sidebar-is-the-pane.md)). The API, which may not
say "sidebar", calls it the **session pane** (`session_pane.get`, `.focus`,
`.unfocus`; `docs/next` socket API page).

- Expanded, it is `ui.sidebar_width` clamped by the sidebar bounds, the same for
  every client, and its divider does not drag. Collapsed, it is
  `ui.sidebar.pane_collapsed_width` (default 8); `sidebar_collapsed_mode =
  "hidden"` still gives it no width. Mobile layouts and shells showing several
  machines keep the native sidebar.
- Its process gets `HERDR_SOCKET_PATH` and `HERDR_SESSION` but no
  `HERDR_PANE_ID`. It is not listed, snapshotted, saved or carried through a
  live handoff; every server start runs it afresh.
- An exit shows the native sidebar and restarts it after 0.5s, doubling to 30s.
  Five quick exits in a row (each under 30s) leave it down with one `warn!` and
  one notification, until a config reload. A reload that changes `pane` restarts
  it; removing `pane` stops it.
- Keyboard focus is a server fact shared by all clients, like pane focus.
  `keys.focus_sidebar` (unbound by default) toggles it. herdr never takes Esc:
  the program calls `session_pane.unfocus`, or any pane, tab or workspace focus
  method, to give it back. Clicking a pane does too.
- Wire: clients opt in through the hello (`session_pane`), the server advertises
  the `session_pane` capability, and both sides talk through the named controls
  in `src/protocol/session_pane.rs`. Generation-1 codecs are untouched, so a
  client or server without the feature keeps the native sidebar.

## Dogfooding

On the primary mac, real work in the fork runs in a side session named `fork`.
Its server is a fork build in `~/.local/opt/herdr-fork/bin`, and the other
sessions stay on Homebrew's herdr. The user starts or attaches it with `hf` in
a kitty tab; the dotfiles' `docs/setup/herdr.md` § the fork side session has the
details. `scripts/dogfood.sh` installs the binary. It never touches
`/opt/homebrew` or a running server:

```sh
scripts/dogfood.sh release [<tag>]        # a fork release asset, checked against SHA256SUMS
scripts/dogfood.sh build [<pr|branch>...] # origin/master plus unmerged PRs, e.g. `build 4 5`
scripts/dogfood.sh status                 # what is installed, and from where
herdr --session fork server stop          # then `hf` starts it on the new binary
```

`build` merges the branches without committing, in its own worktree
`../herdr-worktrees/dogfood-build`, and refuses branches that do not merge
cleanly. The `fork` server is a live runtime too: the rule above applies to it,
unless the agent's brief says otherwise.

## Break nothing

loom (`~/dotfiles/.config/herdr/plugins/loom`, tests in `tests/run.sh`) and the
dotfiles depend on herdr's CLI and API. A change that breaks them fixes them in
the same change. Dotfiles changes follow `~/dotfiles/CLAUDE.md` § cross-host
reconciliation.

## Vocabulary, decisions, tracker

- [`GLOSSARY.md`](./GLOSSARY.md): the fork's terms. Use them.
- [`docs/adr/`](./docs/adr/): decisions with trade-offs. Don't rewrite one;
  supersede it with a new ADR.
- [`.issues/herdr-fork/`](./.issues/herdr-fork/): the tracker, map in
  `000-map.md`, conventions in `~/dotfiles/docs/agents/issue-tracker.md`.
  Claim a ticket before working on it.
