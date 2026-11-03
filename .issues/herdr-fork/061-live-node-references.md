---
id: herdr-fork-live-node-references
title: A loom node id on screen renders its live state, the way GitLab renders !57 (merged)
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/9944e3bd-13b6-46cd-82bc-21bc2342b84d
---

## Question

Requested by the user on 2026-11-03, through the dotfiles loom cockpit (`7a05e2bb`).

An agent's closing prose goes stale while you watch. The dotfiles root ended a turn on "the
pi-migration child (50108375 pimig-001) needs you in wN:p2". Once the user answered there,
that line was false, but it stayed on screen as the pane's last word until something started
a new turn. Nothing rewrites text already in a pane. A statusline line was considered and
rejected (it is somewhere else, not where the claim is).

So make the reference carry its own state, the way GitLab renders `!57` as `!57 (merged)`.
herdr draws every pane from its own cell grid, so it can decorate what is on screen without
touching what the agent wrote.

- **Matching.** An 8-hex token that is a live node id in this session's agent annotations
  ([035](./035-agent-annotations.md)). Covers bare `8c1ee73d`, `loom:8c1ee73d` and
  `[8c1ee73d]`. A commit hash collides only if it equals a node id in the registry (about 240
  of 16^8), which is negligible. Closed nodes render as closed while their record exists.
- **Rendering.** The id is recoloured in its status colour, the same as the sidebar's dots
  ([045](./045-flip-report-after-colours.md), [052](./052-sidebar-attention-two-axes.md)).
  Where the line has trailing blank cells, add dim end-of-line text in them, like neovim's
  end-of-line diagnostics: `asking · wN:p2`, `done`, `closed`. Never insert text into the
  line, so layout, wrapping, copy, selection and `pane read` are unchanged. On a full line,
  recolour only.
- **Click.** Ctrl+click (as for links) focuses the node's pane.
- **Cost.** [040](./040-pane-read-osc8.md) kept link work out of render and detection, "the
  multiplicative paths". This one has to live in render, so scan only on-screen lines, cache
  matches per line until the line's content changes, and look state up in the annotations
  herdr already holds (no loom or socket call per frame). Measure the frame time with a
  screen full of ids and with none.
- **Opt-out.** A config switch, on by default in the fork build.

Decide and record:

- How the end-of-line text picks which of several ids on one line to describe (the first, or
  one per id when they fit).
- Whether `pane read --ansi` should ever show the decoration (proposed: no, it is render
  only).
- Later, not this ticket: the GitLab half for real. MR, PR and CI references (`!2938`,
  `owner/repo#47`) rendered through the reader hooks waits already use
  (dotfiles loom 089), e.g. `!2938 (reviewed)`.
  Note it under the map's open questions.

Done when a real loom session shows the incident's shape: a pane whose text names an asking
node shows it as asking, then as running once the user answers in that node's pane, with no
new turn in the first pane.

## Parent

[herdr-fork map](./000-map.md)

## Resolution

Merged 2026-11-03: [saitonakamura/herdr#57](https://github.com/saitonakamura/herdr/pull/57) as `e7e6be2b` (head `294cefb2`). No wire, snapshot or API-shape change. `FORK.md` § Agent references documents it.

Built as asked, with what the build settled:

- **Where.** The server decorates the cells it renders, in `src/ui/agent_refs.rs`. One row abstraction serves the full renderer (`render_panes`, after each visible pane's `rt.render`) and the retained path's dirty patch rows. A test checks that a retained patch leaves the same committed surface as a fresh full render.
- **Matching.** The token is any herdr agent id (a digit, then seven lowercase hex digits) bounded by non-word cells, so `8c1ee73d`, `loom:8c1ee73d` and `[8c1ee73d]` match. A loom node id is its agent id, so no annotation check was needed. A live agent is matched by its terminal, and a closed one by a record of the last 256 agents closed since the server started (`closed`). That record does not survive a restart, and herdr never reads loom's registry.
- **Words.** Loom's task status wins where it says more than herdr's agent status: `asking` (needs_user), `blocked` (herdr blocked, or needs_parent), `failed`, `done` (complete), `closed`. Otherwise herdr's own `working`, `awaiting`, `idle` or `done`, and `running`/`starting` for a pane with no detected agent. The pane id follows the word only while the agent wants someone (`asking · w1:p2`).
- **Colours** are the sidebar's meanings, with `[sidebar.colors]` overrides applied. The end-of-line text is dimmed.
- **Click.** Ctrl+hover and Ctrl+click reach an id through `pane.link.resolve` and `pane.link.activate`, after OSC 8 links and URLs. Activation focuses the agent's pane, switching workspace and tab.
- **Redraw.** `agent.set_task_status` and `agent.annotate` are `changes_ui: false`, so loom's changes never redrew anything, and the live run caught `asking` staying on screen. While the feature is on, the headless server now counts those two as UI changes. The shared operation table is untouched.
- **Opt-out.** `[ui] agent_references = false`, which belongs in `config.fork.toml`. On by default.

The two open points:

- **Several ids on one line:** one segment per distinct id, in line order, each as `word · pane` if that fits, else as the word alone. The first id that does not fit ends the text. The id's own colour pairs it with its segment, and a full line is only recoloured.
- **`pane read --ansi` never shows the decoration.** It is render-only, and `pane read` reads the terminal grid through the formatter, which the decoration never touches. Copy and selection stay unchanged for the same reason, which is the property the ticket wanted. A reader that wants a node's state can ask herdr for it directly.

**Cost** (`just bench-render-scale`, new `agent references` profile, server pane surface at 120×40, median µs). Off with ids on screen / on with no ids / on with a screen full of ids:

- 1 pane: 235 / 231 / 265.
- 15 panes: 267 / 267 / 280.

The scan is one byte test per cell, with no allocation. The agent index is built once per render pass, only when a row holds an id-shaped token. The retained path rescans only dirty rows, which is the per-line cache. The full renderer has no content cache, because hashing a line would cost as much as scanning it.

**Live**, in a throwaway session on the debug build, driven through the calls loom makes (`agent annotate`, `agent set-task-status`):

1. `child 749a7aa8 needs you` rendered `… asking · w1:p2`, in magenta.
2. After an answer typed in the node's pane and `set-task-status running`, it read `… running`. The first pane's content revision stayed 2, so it had no new output.
3. `pane read` of that pane showed plain text.
4. A real Ctrl+click on the id focused the node's pane.

`just check` locally: 3 nextest failures, each of which also fails on clean `master` (macOS ACL, `api_ping` done status, `cross_area` detach/reattach), and the release-script fixtures' `git tag` under `tag.gpgsign`. CI was green; macOS needed three attempts because GitHub could not acquire a runner.

Not this ticket: MR, PR and CI references rendered the same way through the wait reader hooks are under the map's "Not yet specified".
