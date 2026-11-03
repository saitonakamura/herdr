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
- Behaviour changes go through a PR on `saitonakamura/herdr` into `master`. Once
  the head is green, the agent merges it with a merge commit
  (`gh pr merge --merge`) without asking, then pulls `master`.
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
`.unfocus`, `.set_width`; `docs/next` socket API page).

- Expanded, it is `ui.sidebar_width` clamped by the sidebar bounds, the same for
  every client. Its last column is a divider like the native sidebar's: dragging
  it calls `session_pane.set_width`, which every client follows, and a
  double-click goes back to `ui.sidebar_width`. The server keeps that width in the
  saved session, not in a client's preferences, because one program draws for
  every client. Collapsed, it is
  `ui.sidebar.pane_collapsed_width` (default 8); `sidebar_collapsed_mode =
  "hidden"` still gives it no width. Mobile layouts and shells showing several
  machines keep the native sidebar.
- Its process gets `HERDR_SOCKET_PATH`, `HERDR_SESSION` and `HERDR_CONFIG_PATH`
  but no `HERDR_PANE_ID`. It is not listed, snapshotted, saved or carried through a
  live handoff, though a width set by dragging is; every server start runs it
  afresh.
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

## Sidebar program

`herdr-sidebar` ([ADR 0006](./docs/adr/0006-sidebar-program-ships-with-the-fork.md))
is the program the sidebar pane runs: `[ui.sidebar] pane = ["herdr-sidebar"]`. It
is the workspace member `tools/sidebar/`, so upstream merges never touch it. What
it draws and does is 018's resolution (`.issues/herdr-fork/018-sidebar-content.md`).

- **Data.** Only herdr's API on `HERDR_SOCKET_PATH`: `session.snapshot` and
  `message.list`, re-read (at most 20 times a second) whenever `events.subscribe`
  reports a change, and from scratch after `events_lost` or a reconnect. herdr has
  no session-wide state event, so a second subscription holds one
  `pane.agent_status_changed` entry per agent pane. loom's facts are agent
  annotations (`effort`, `ticket`, `source`); it never reads loom's files. Its
  types are a subset of herdr's schema types, and its tests check them, and every
  request it sends, against `docs/next/api/herdr-api.schema.json`.
- **Tabs with two agents.** A row is named after its tab. A second agent in a
  tab with no Parent (a pi split in beside a child, say) hangs under the tab's
  first agent and goes by its own label or terminal title, since the tab's name
  belongs to the first.
- **What herdr doesn't know.** Age is the time in the current state as the
  program saw it, since herdr records no creation time; it updates only on
  redraws. The dirty count is herdr's, which runs `git status` only while a
  native agent row shows `git_dirty`, so keep that token in `[ui.sidebar.agents]`.
- **Config.** `[sidebar]` in `config.toml` and `config.fork.toml`, merged like the
  overlay, read at start, on SIGHUP and within about two seconds of either file
  changing. herdr accepts the table and never reads it.
- **Upgrades.** The program checks its own binary every second. When a new one is
  renamed over it, as `scripts/dogfood.sh` installs, it runs the new binary in
  the same pane and process, so the sidebar updates without a session restart.

  ```toml
  [sidebar]
  glyphs = "nerd"   # or "unicode"
  line1 = ["name", "effort", "ticket", "messages", "questions", "source", "age"]
  line2 = ["branch", "ahead_behind", "dirty", "worktree"]
  colors = { muted = "#757ead", selection = "8", working = "yellow" }
  glyph = { awaiting = "…" }   # per meaning; `herdr-sidebar glyphs` lists them
  attention = true   # the attention section; false turns it off
  [sidebar.actions]
  close = { command = ["loom", "close", "{agent_id}"], popup = true }
  ack = { command = ["loom", "ack", "{agent_id}"] }
  ```

  Actions are `reply`, `tell`, `close`, `ack`, `ui`, `rename`, `close_workspace`,
  `new_worktree` and `new_workspace`; `{agent_id}`, `{pane_id}`, `{workspace_id}`
  and `{name}` are filled in. Started from the menu, `popup = true` runs the
  command in the menu's own popup, otherwise detached (`new_workspace`, on the
  `+ new` row, goes through `popup.open`). Without an override `ui` opens loom's
  `ui` plugin popup once the menu's has closed, and `ack` says it is not
  configured: herdr has no unread reports of its own. `ack` and `close` run inside
  the menu, which waits for them and shows what they print.
- **Keys and the menu** (059). Navigation, `h`/`l` and `za` folding, `f` scope,
  `/` search and Enter (focus the row's pane) are the sidebar's own keys. A row's
  actions live in its menu: `m` or a right-click on an agent row, an attention row
  or a workspace header opens `herdr-sidebar menu pane|workspace <id>` through
  `popup.open`, which reads what it shows through the API and reaches the
  sidebar's scope and sections through `ctl`. The popup shows the name and the
  latest report's first line, and one entry per action with its key; the key, j/k
  and Enter, or a click runs one, and Esc closes it, giving keyboard focus back to
  the sidebar when it held it.
  - Agent: **open report** (`o`; closing it acks an unread one), **ack** (`a`,
    while a report is unread), **reply** (`r`), **tell** (`t`), **close** (`c`),
    **scope to subtree** (`f`), **open in loom ui** (`p`).
  - The report screen (060) has the name, the kind's glyph in its colour, the kind
    and the report's age as its header, then the report as loom wrote it. That
    text comes from the herdr message, not loom's files: its summary without the
    `<kind>: ` prefix or the ` ‖ ` rows loom appends for the parent agent, then
    its body, which is loom's `--body-file` (herdr types only the summary into
    the recipient). It is drawn as light markdown: headings bold, inline code and
    fenced blocks dimmed, list items with a hanging indent, wrapped at word
    boundaries. j/k, ctrl-d/u and the wheel scroll it.
  - Workspace: **rename** (`r`), **close** (`c`), **new worktree** (`w`),
    **collapse others** (`o`), **move up** (`K`), **move down** (`J`). A move
    keeps the popup open on the same entry, so `K K K` walks an effort up; at
    the top or bottom the entry stays and says "already first" or "already
    last".
  - With `[sidebar.actions] close`, close asks for an outcome, ok or struggled
    (with a why), and runs the override with `--outcome <it>` (and `--why`)
    appended, loom's flags. When it refuses, the menu offers `--force`; when
    unread reports are all that hold it (every line loom names a node on reads
    `[<id>] unseen:`), it also offers **ack and close**, listing each report's
    first line before acking them through `ack`. Without the override, close is herdr's
    `agent.close`: the panes it would take, a confirm, and `--force` on
    `agent_subtree_busy`.
- **Reordering efforts** (062). Pressing a workspace header and releasing
  elsewhere in the list moves that workspace in herdr's order, which herdr keeps
  and every client shows: above the header it is dropped on, or after the last
  workspace when dropped below the last tree. While dragging, a rule naming it
  marks where it will land (with no gap row above, the target header lights up
  instead). A header acts on release, so a press and release on one header is
  still a click, folding or focusing as before. Dragging works over the whole
  list only, not while scoped or searching. Moves go through `workspace.move`,
  or `workspace.move_block` for a repo's worktree group, following upstream's
  rules (`8843bbb0`): a linked worktree's workspace never moves alone and
  nothing lands inside a group. Fork servers open worktrees as tabs and report
  no workspace `worktree`, so in practice every workspace moves on its own,
  carrying its worktree tabs.
- **Attention** (052). A section pinned above the trees lists, session-wide
  whatever the scope, every wanted agent (task status `needs_user`, or blocked) and
  every agent with an unread report (loom's `unseen` annotation). It is hidden when
  empty and while searching. Rows sort loud (wanted, unread; a herdr block with
  no `needs_user`), steady (wanted, read), then quiet (unread only), oldest first;
  the agent keeps its tree row, marked the same way. Loud draws a `▌` in the state's
  colour at the row's left edge (wide rows) and bolds the name. Every row carries at most one loom mark after its name: the
  unread report's kind in its colour (done `✓`, failed `✗`, blocked `↑`, asking
  `?`, handoff `→`, any other `•`; nerd glyphs in the `nerd` set), else, once
  acked, where the task status settled, muted (complete `✓`, failed `✗`,
  needs_parent `↑`). `unread_done`, `unread_failed`, … in `[sidebar.glyph]` and
  `[sidebar.colors]` set them. The mark makes the `task` token redundant. A row's second line is loom's `reason`, else the blocked
  reason, else the report kind and the first line of the agent's latest report
  (`message.list`, read again when it sends one), without the kind loom's summary
  already leads with. A quiet row's age counts from that report's `sent_at_ms`, so
  it survives a sidebar restart; a wanted row's is the program's own clock.
  About 10 s of focus draws a loud row as steady until its state next changes;
  that is the program's own memory, and never acks anything.
- **State.** Collapsed sections, folds and the scope live in
  `$XDG_STATE_HOME/herdr/sidebar/<session>.json`. `herdr-sidebar ctl
  fold|scope|collapse-others [<pane>]` talks to the running program over
  `<HERDR_SOCKET_PATH>.sidebar`, so a keybind in any pane of the session, or the
  menu, reaches it; without a pane it acts on the focused one, and
  `collapse-others` also takes a workspace id.
- **Shipping.** `fork-release.yml` builds `herdr-sidebar-<os>-<arch>` for the four
  unix targets beside `herdr`, in `SHA256SUMS`; `scripts/dogfood.sh` installs it
  beside `herdr`. Windows builds only a stub and keeps the native sidebar.

## Agent references

An agent id on screen (a loom node id, bare, `loom:<id>` or `[<id>]`) is drawn in
its agent's status colour, and the line's trailing blank cells say who it is and
what it is doing, dimmed and right-aligned to the pane's last column:
`main-tracker · idle`, `pimig-002 · pi-migration · asking`, `closed`
(`.issues/herdr-fork/061-live-node-references.md`,
`065-node-reference-says-who-and-where.md`). Ctrl+hover underlines it and
Ctrl+click focuses its pane, through the same `pane.link.resolve` and
`pane.link.activate` as URLs, which win where both match.

- **Render only.** The server decorates the cells it renders, so the terminal
  grid, copy, selection and `pane read` (`--ansi` included) never see it.
- **Name** is what herdr-sidebar calls the agent's row: its tab's name, or for a
  tab's second agent its own label or terminal title. The workspace's label
  follows it only for an agent in another workspace than the pane drawn.
- **Fit.** At least two blank cells stay between the line's text and the
  annotation. Short of room it drops the workspace, then the name, keeping the
  word; a line with no room for the word is recoloured only. Several ids get one
  segment each, in line order, right-aligned together; the first that does not
  fit ends the text.
- **Words.** Loom's task status where it says more than herdr's agent status:
  `asking` (needs_user), `blocked` (herdr blocked, or needs_parent), `failed`,
  `done` (complete), `closed`; otherwise herdr's `working`, `awaiting`, `idle`,
  `done`.
- **closed** covers agents closed since the server started, the last 256.
- **Colours** are the sidebar's meanings, and `[sidebar.colors]` overrides apply.
- **Off:** `[ui] agent_references = false` in `config.fork.toml`.

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
