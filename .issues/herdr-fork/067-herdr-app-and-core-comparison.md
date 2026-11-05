---
id: herdr-fork-herdr-app-and-core-comparison
title: Compare upstream's herdr-app and herdr-core with the TUI fork, and what switching would cost
status: in-progress
labels:
  - wayfinder:research
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/w1:p2A/app-core-067 (loom 75cf2a53)
---

## Question

Around 2026-11-05 herdr.dev was rewritten around a new desktop app and a headless core, with the TUI now
"the original". Is there a good reason for the user to move from their TUI fork to herdr-app, or to a fork of
it, and what would that cost? Nothing is decided; the user decides from this ticket and closes it.

## Parent

[000-map](./000-map.md)

## Blocked by

Nothing.

## Resolution

Read from source only: neither `hc` nor the app was built or run, so how the app feels is still unknown.
Evidence, commits and the full tables are in
[`docs/research/herdr-app-core-comparison.md`](../../docs/research/herdr-app-core-comparison.md).

### The answer

**Stay on the TUI fork, and keep the plan: upstream sync, `v0.9.5+fork.3`, then the cutover. Add OSC 7501 to
the fork, and I'd put it before MR/PR references. Try the app in isolation for a week; only that can show
whether it's worth about 40 tickets.**

The reason to switch is real but narrow: a better kind of UI surface. Against it, the things the fork exists
for are the things core refuses on principle: resume after a reboot, a message queue, waits with deadlines,
screen detection, efforts. They'd have to be rebuilt beside core, not ported. The strongest finding is
something else: **upstream's work has moved away from the TUI, so the fork is now effectively its maintainer.**

### Is there a reason?

**Yes, in the app's surfaces.** A mod can open any number of overlays: dialogs, popups and non-modal
draggable windows. It can dock floating blocks and hover drawers, split a panel in as a pane, and put a live
terminal inside a floating window, all with real fonts, images and markdown. `popup.open` is one modal PTY
program at a time. Growing popups in the fork takes about 2–3 tickets but still gives you terminal cells.

**Other real gains:**

- Agents survive core restarts and updates, because holders keep the programs running.
- Status arrives down the pane's own PTY, so the failure class behind
  [066 pi authority lost](./066-pi-lifecycle-authority-lost.md) doesn't exist there.
- Theme flips follow the system with no kitty chain.
- Reordering spaces, multi-machine lists and "done" across clients are native.

**What overlays can't do:** anything on terminal text. Mods are barred from it, so live node references and
the planned MR/PR references would need a patch to the app.

**What's worse, from the source:**

- There is no copy mode and no modal keys. The sidebar isn't keyboard-reachable as shipped, though a mod can
  fix that. For a vim user this is the biggest daily risk.
- The app replaces kitty as the terminal.
- Over SSH you get one terminal through `hc attach`, with no sidebar.
- Telemetry is on by default.
- It's 0.1.x with live-runtime bugs still being fixed: terminals freezing (fixed 0.1.16 and 0.1.17), and on
  macOS, terminals needing a reopen after 0.1.18.

**The mods API** has wide reach and is unsandboxed, but it has no version and no compatibility test. So far
it has changed by widening only. A mod runs only while the window is open, so a mod can't hold any of loom's
runtime. **The core API is much firmer:** a test enforces "only grows", and no name has been removed across 10
release copies.

### Requirements, in short

The [full table](../../docs/research/herdr-app-core-comparison.md#2-requirements) has about 50 rows, none
dropped.

| | |
| --- | --- |
| **Native, some better than ours** | Parent at spawn, notes on stuck children, typing gate, `hc wait`, command supervision, labels as annotations, done across clients, reorder, per-session sockets, surviving server updates |
| **Labels or a mod** | Source, task status, asks, unread and ack, attention, sidebar and menu, tab names, worktree as tab, prefix keys, notifications |
| **A loom sidecar in a kept `hc` terminal** | message queue and batched delivery, waits with deadlines, readers, resume after a reboot, interrupted turns, non-agent panes re-running |
| **Patch to the app, or gone** | live node references, MR/PR references, link hooks, OSC 8 in reads |
| **Gone or changed for good** | screen detection (agents without 7501 show nothing), `form` and `interrupted` blocked reasons as native states, copy mode, moving a child to another Parent natively |
| **Awkward** | effort = space needs the window open to name a space ([app#43](https://github.com/herdrdev/herdr-app/issues/43) unanswered); core keeps the env of whatever started it first, a trap for the work lane |

### loom

Core replaces about a third of loom's runtime half outright (annotations, reads, most of Parent) and part of
another third (identity, subtree close, simple waits). The nouns are close, but the contracts differ: core
types now or refuses where we queue, it has no efforts and refuses them, and state is whatever the program
says. **loom can sit on `hc`'s API**, but that's a rewrite of its herdr-facing half, not a port. It needs a
sidecar to host what the plugin manifest gives it today, which partly undoes what
[ADR 0004](../../docs/adr/0004-loom-runtime-in-herdr.md) gained. The policy half ports unchanged. `loom run`
gets simpler on `hc run`. The bridge's state reports go away, but its control half stays, because core's API
can run commands and write files.

### Options, in tickets

Of the fork's 66 tickets: **0 port as they are in Rust**, 17 stop being needed, 12 are small, 19 are real
rewrites, 5 need an app patch or are gone, and 13 are process work.
[Listed by name](../../docs/research/herdr-app-core-comparison.md#4-options-in-tickets).

| option | size | thrown away | verdict |
| --- | --- | --- | --- |
| **(a) stay on the fork** | 0 to stay | nothing; but detection, Ghostty bumps and fixes are now ours | **yes** |
| **(b) stock app and core, mods, ported loom** | **about 40** | about 27,600 lines added in `src/`, `herdr-sidebar`, node references, planned MR/PR references, copy mode, kitty as the terminal | only if the trial wins you over |
| **(c) fork the app and/or core** | **about 50–55**, plus merging about 20 app and 9 core commits a day | as (b), except references survive | no; if you must, patch rendering only, never put runtime in core |
| **(d) fork plus OSC 7501** | 2–3 | nothing | **yes** |
| **(d) app beside the TUI, isolated** | 0–1 | nothing; agents aren't shared | **yes, as a trial** |
| **(d) TUI on core** ([andymai's fork](https://github.com/andymai/herdr/tree/feat/core-backed-panes)) | 10–15 | two runtimes hold the truth | wait for upstream's decision |
| **(d) Sheprd client over the fork's server** | 0–1 to try | nothing | optional look; third-party, compatibility unverified |

The (b) and (c) sizes cover all five boxes, the work lane and the pi sandbox. The breakdown is in the
research file; these are estimates, none of it built.

### Upstream's direction

- **Commit rate.** The TUI fell from 6.8 commits a day (09-15 to 10-15) to 0.75 (10-16 to 11-05), with none
  since 10-29. Its last release is 0.9.5 on 10-14, its last merged PR landed 10-26, and six later PRs were
  closed unmerged. The app runs at about 20 a day and core at about 9.
- **No shared code, and fixes don't flow.** Core's rules say "Fresh code. Do not copy from Herdr 1".
- **Maintainer, 10-29:** "whether the tui moves onto herdr-core later is still open, i'll update here when
  that's decided" ([app#63](https://github.com/herdrdev/herdr-app/issues/63)). The site promises only that it
  "keeps working". So it's neither dead nor promised; plan as if no more help is coming.
- **Practical effect on us.** Upstream's detection catalog last changed 10-13, so after this sync, keeping
  screen rules current for new CLI versions is our job.

### OSC 7501

An open status protocol ([spec, rev 0.4](https://www.superlogical.com/rex/docs/build/program-status)):
programs report idle, working, done, blocked (permission, question, auth) or error down their own PTY.
**Claude Code and pi already speak it, but only after the terminal answers a support query.** Codex's binary
contains it too. The fork's vendored libghostty predates the parser by a month, so the query goes unanswered
and neither agent reports in fork panes (inferred from source, not observed).

**It's cheap and it matters even if we stay:** about 2 tickets, one to parse and answer and one to make it a
state source and retire pi's bridge state reports. It answers the design question in
[066 pi authority lost](./066-pi-lifecycle-authority-lost.md): for state, no separate claim is needed, and it
needs no bridge from the sandbox. It also covers the risk that upstream's screen rules stop being updated.
Sentinels stay, because a prose question isn't a state these agents are known to report.

### What should change in the plan

1. **Nothing before the cutover.** Sync, soak, `fork.3` and the per-box cutover stand. Nothing here is a reason
   to delay them, and the bridge fix decided for
   [066 pi authority lost](./066-pi-lifecycle-authority-lost.md) still goes first.
2. **Your call: OSC 7501 before MR/PR references.** You named MR/PR references as the first feature after the
   cutover. I'd go 7501 first, since it protects what you already have.
3. **Treat upstream as a source of occasional fixes, not a maintainer.** That includes detection manifests.

MR/PR references stay a fork feature either way; the app couldn't host them without a patch.

### What would change my mind

- **The trial:** you prefer the window after a week, and don't miss copy mode or kitty. Then (b) is worth
  doing, in stages, with status and mods first.
- **Upstream decides:** the TUI moves onto core, which could make the path far cheaper; or it's declared
  finished, which raises the cost of staying.
- **Core grows** a message queue, or a real way to resume; or the app answers
  [app#43](https://github.com/herdrdev/herdr-app/issues/43) with supported spaces.
- **The fork gets expensive:** Ghostty bumps or agent breakage regularly costing more than a ticket a week.

### For you to try

I couldn't judge these from source. Run the app apart from your live herdr, with its own core session and home,
for example `HC_SESSION=trial HERDR_APP_HOME=~/herdr-trial`, and `DO_NOT_TRACK=1`. The installer adds `herdr-app`,
`herdr-core` and `hc` to `~/.local/bin` and doesn't add a `herdr` command (`distribution/install.sh`), so it
shouldn't shadow your herdr by name. Read the installer first anyway, since I haven't run it or this setup.

1. **A keyboard-only hour.** Move between panes, tabs, spaces and agents with vim habits, using the
   [tmux-prefix mod](https://github.com/herdrdev/herdr-layout-examples/tree/HEAD/tmux-prefix). Then copy something
   out of scrollback with no mouse. This is the likeliest dealbreaker.
2. **Feel beside kitty.** Typing and scrolling in Claude and nvim, on the M5 Max and the M1 Air, including the
   Russian layout and the wheel.
3. **What overlays are worth to you.** Open the [board and mission layouts](https://github.com/herdrdev/herdr-layout-examples),
   and ask an agent for a report reader in a floating window. Would you use it daily, over the sidebar popup?
4. **Real agents.** Claude, host pi, sandbox pi and `codex --no-daemon`: do they show the right state on a
   permission dialog, a question asked in prose, and background work? Then `hc server restart` and a reboot, to feel
   what no resume means.
5. **The hard boxes.** Fedora (NVIDIA, KDE Wayland) and WSL, if the first four make you want this at all.
