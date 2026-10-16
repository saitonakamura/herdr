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
  distribution contract CI job enforces this.
- When an upstream sync conflicts on a manifest both sides changed, resolve it
  in both copies with a version higher than either side's.
- `HERDR_AGENT_DETECTION_MANIFEST_CATALOG_URL` still overrides the catalog, and a
  local override in `~/.config/herdr/agent-detection/` still beats any remote
  manifest.

## Commits

- Upstream's style: lowercase conventional commits, no emojis, no AI co-author
  lines. Propose the message before committing.
- Instead of GitHub issue refs, put the ticket in the body:
  `refs herdr-fork-<ticket-id>` (for example `refs herdr-fork-sync-and-branch`).
- Run `just check` before committing; never bypass a failing check, fix it or
  say exactly why a narrower check suffices.
- Commits are signed through 1Password; a signing failure means the app is
  locked or not running. Never `--no-gpg-sign`.

## The live runtime

herdr runs every agent pane, including the one building it.

- Never replace the installed binary, restart a herdr server, or run `brew`
  against `herdr` without asking the user.
- Test builds against the debug `herdr-dev` server:
  `env -u HERDR_SOCKET_PATH -u HERDR_CLIENT_SOCKET_PATH cargo run -- <command>`,
  or a throwaway named session.
- Detection manifest work: the hot-reload loop in `AGENTS.md` still applies, but
  only against a throwaway session, never the live one.

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
