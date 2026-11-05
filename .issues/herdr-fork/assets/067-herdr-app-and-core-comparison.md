# herdr-app and herdr-core against the TUI fork: the evidence

Detail behind [067 herdr-app and core comparison](../067-herdr-app-and-core-comparison.md).
The verdict and the plan changes are there; this file holds the tables and citations.

**Read at, 2026-11-05:**

| repo | commit | version |
| --- | --- | --- |
| `herdrdev/herdr-app` | `2a7fad1` a second rename press closes the prompt | 0.1.13 |
| `herdrdev/herdr-core` | `c6b316f` preserve core environment across planned restarts | 0.1.18 |
| `herdrdev/herdr` (the TUI) | `ee5bfb50` point agents at the source | 0.9.5 |
| `saitonakamura/herdr` (the fork) | `c897ad20` add the 066 bridge pull to the cutover steps | 0.9.5+fork.2 |
| `saitonakamura/dotfiles` | `a9684bf` loom 118 the user's decisions | |
| `badlogic/pi-mono` | `55a5c6914` branch tip through the API | |
| `sheprd-labs/sheprd` (was `penso/herdr-gpui`) | `88a7ecf` merge PR 1078 | |

**How this was read.** Source only. Neither `hc` nor the app was built or run. Claude Code's and
Codex's behaviour comes from strings in their local binaries (2.1.318, 0.166.0), which shows what the
code can do, not what a session did. Paths below are relative to each repo.

**Two leads in the brief need correcting.**

- The app's installer does not add a `herdr` command. `distribution/install.sh:89,131` links
  `herdr-app`, and `:137` installs `herdr-core` and `hc` when missing. Nothing there shadows the
  installed herdr by name. It does still write to `~/.local/bin`.
- "App and TUI don't share agents" is confirmed, and it is deliberate. The maintainer, in
  [herdr-app#63](https://github.com/herdrdev/herdr-app/issues/63): "they're separate on purpose".

## 1. The reason

### Overlays

The app has three surfaces, all driven from JavaScript (`defaults/herdr.d.ts:333-369`,
`defaults/AGENTS.md:369-393,960-974`):

- **`openOverlay`** with `kind` of `dialog` (centred, dims the window), `popup` (at the pointer or a
  point) or `window` (non-modal, draggable, remembers its spot, can reopen at start). It holds one mod,
  or a whole layout including live terminals.
- **Floating blocks** declared in a layout, anchored to an edge. They spring to the nearest edge and
  third when dragged, and `peek` makes one a drawer that slides out on hover.
- **Mod panes** (`pane.split {mod, props}`), which split, zoom and save with their tab like a terminal.

Mods get markdown (`TextView.markdown`), a code view or editor (`EditorState`), images, SVG, native
animation and proportional fonts. `terminal.peek` shows an agent over the current tab.

| | fork `popup.open` (`src/api/schema/popups.rs`, `src/app/popup.rs`) | app `openOverlay` |
| --- | --- | --- |
| what it holds | one PTY program in cells | a GPU-drawn mod, or a layout with terminals in it |
| how many | one per session; a second gets `popup_already_open` | any number, stacked |
| modality | session-modal | dialog, popup, or non-modal window |
| placement | width and height; centred | pointer, point, remembered spot, edge, drawer |
| type and images | terminal cells | fonts, images, SVG, animation |
| run from | any program through the socket | a mod, so only inside the window |

**What you could build:**

- a report reader beside the agent, in proportional type;
- an agent's live terminal in a floating window while you keep working;
- a board of efforts, or of tracker tickets through `exec`;
- an MR and PR panel with `fetch`;
- a drawer of waits and deadlines at the window's edge;
- a diff review pane.

**What you could not build:** anything on terminal text. The guide says it outright (`AGENTS.md:35-37`):
mods can't "change how a terminal itself draws text" or draw outside their block except through
overlays. Live node references and MR/PR references are decorations on terminal text, so they aren't
mods. The app's built-in equivalent is Ctrl-click on a `t7` address, which peeks at that terminal;
the address isn't coloured and carries no state text.

**How far the TUI is.** The fork could grow non-modal, stackable, positioned popups in about 2–3
tickets, because they are still PTY programs in boxes. It can't get real type or easy images. So the
gap is real, but it is a gap in kind of surface, and the fork's popup menu and report screen already
cover the daily uses. Whether the extra surface is worth anything is a taste question; try it.

### UI

**Better, from the source:**

- real panels instead of cells;
- pane headers and frames, drag-to-rearrange panes, a scroll thumb, a find bar;
- the Files and Changes panels, a command palette;
- layouts and mods that hot-reload, with the last good version kept on a broken save;
- `{light, dark}` themes following the system, so kitty's `?997` chain
  ([043 flip chain](../043-light-dark-flip-chain.md),
  [045 flip ordering](../045-flip-report-after-colours.md)) simply goes away.

**Worse, or at risk, from the source and the issue tracker:**

- **Vim and keyboard.** `keys.json` has chords and no modes; `src/keys.rs:27` calls the shipped set
  "prefix-free". A prefix is a popup mod with a 500 ms repeat window
  ([tmux-prefix recipe](https://github.com/herdrdev/herdr-layout-examples/tree/HEAD/tmux-prefix)). No
  shipped `mod.json` sets `focusable`, so the sidebar isn't keyboard-reachable until you write that
  yourself. **There is no copy mode:** no hit in the guide, the changelog or the command docs. You get
  mouse selection, `terminal.search` and `terminal.scroll`.
- **It replaces kitty.** The app is the terminal emulator. Kitty's config, hints and theme files don't
  apply inside it. It does handle kitty graphics, including Unicode placeholders, and the kitty keyboard
  protocol.
- **Open reports at 0.1.x:**
  - wheel speed ([herdr#5761](https://github.com/herdrdev/herdr/issues/5761), filed in the TUI repo);
  - GPU memory from redrawn images ([herdr#5783](https://github.com/herdrdev/herdr/issues/5783));
  - Hangul input ([app#392](https://github.com/herdrdev/herdr-app/issues/392));
  - contrast ([app#173](https://github.com/herdrdev/herdr-app/issues/173));
  - memory: the maintainer measured about 282 MB for 18 terminals
    ([app#560](https://github.com/herdrdev/herdr-app/issues/560)), mostly graphics memory, by display.
- **SSH.** The app needs a desktop session. Over SSH you get `hc attach` to one terminal, with no
  sidebar. Phone users are asking for more in [app#63](https://github.com/herdrdev/herdr-app/issues/63)
  and [discussion 5707](https://github.com/herdrdev/herdr/discussions/5707). No doc in the dotfiles
  shows the user depending on SSH into herdr, so this may cost little.
- **Fedora.** Vulkan on the NVIDIA stack under KDE Wayland. A KDE window bug
  ([app#600](https://github.com/herdrdev/herdr-app/issues/600)) and a multi-seat bug are closed.
  Nothing suggests it fails; nothing proves it works.
- **WSL.** Either the Linux app under WSLg, with
  [app#585](https://github.com/herdrdev/herdr-app/issues/585) and
  [app#580](https://github.com/herdrdev/herdr-app/issues/580) open, or the Windows app with WSL as an
  SSH machine ([app#153](https://github.com/herdrdev/herdr-app/issues/153),
  [app#172](https://github.com/herdrdev/herdr-app/issues/172)).
- **Macs.** Apple Silicon only, which both are. macOS privacy prompts belong to a separate "Herdr Core"
  app since 0.1.18, and terminals opened before an update can lose folder access until reopened
  ([core#123](https://github.com/herdrdev/herdr-core/issues/123)).
- **Defaults.** Telemetry is on by default (`docs/security.md`) and the app updates itself. Both can be
  turned off; source builds do neither.

**Could not judge without running it:** input latency, scrolling feel, font rendering next to kitty,
Russian-layout behaviour, real memory on the M1 Air, focus behaviour, Vulkan on fedora, and whether
it just feels better. These are most of the "nicer UI" question. The repo has a benchmark
(`scripts/bench.py`), but it compares app builds, not the app against the TUI in kitty.

### The mods API

**Reach:**

- live `spaces()`, `tabs`, `panes()`, `agents()`, and `terminals()` with parent and labels;
- `run` for every app command; `exec` locally or on a machine; `hc`; `fetch`;
- file reads, `watchFolder`, shared and per-copy state, overlays, keys through `nextCommand`.

Mods are unsandboxed trusted code (`docs/security.md`).

**Limits:**

- no terminal-text drawing and no HTML;
- no `setTimeout` (use `cx.timer`);
- no event for terminal output, so text comes from `hc read`;
- **a mod only runs while the window is open**, so it can't own anything that must keep running.

**Stability:**

- `AGENTS.md` says "Preserve compatibility with released versions and installed user data", but nothing
  versions the mod API and no compatibility test guards it.
- `defaults/herdr.d.ts` changed in 20 commits in three weeks. Only two commits replaced an exported
  function's signature, and both widened it: `exec` gained `timeout` (`e77f112` worktree waits), `focused()`
  fields became nullable (`31caded` focus terminal blocks). The second can still break a mod that
  assumes a pane.
- Shipped mods are replaced on every start and clones never update (`AGENTS.md:186-205`), so a cloned
  sidebar drifts from upstream's on its own.

Upstream's stated policy is that features belong in mods unless three tests fail, and that issues
asking for what a mod can do get closed (`CONTRIBUTING.md`).

## 2. Requirements

Key for "how to get it": **native** works as shipped, **labels** means core's free-form labels plus
loom, **sidecar** means a loom program that keeps running in a kept `hc` terminal, **mod** is JavaScript
in the app, **patch** needs a fork of that repo, **gone** has no sensible route.

| what the user relies on | herdr-core | herdr-app | how to get it |
| --- | --- | --- | --- |
| Parent tree across agents | `parent` field, set once at start (`terminals.rs`); `--parent`, `--no-parent` | sub-agents listed under parent | native for the tree; `agent set-parent` (loom move) has no equivalent, so labels |
| Parent closed, child becomes top-level | opposite default: children die with parent unless `--outlive-parent` | | native with the flag on every spawn, plus labels |
| Source across efforts ([ADR 0008](../../../docs/adr/0008-parent-stays-in-a-workspace.md)) | no; core has no effort concept | | labels |
| Stable agent identity | address stays through `keep`; ends with the terminal; no agent id separate from the terminal | | labels; a new identity after a closed terminal is loom's |
| Agent spawn args and env for resume | env through the API only (`RunParams.env` has no CLI flag) | | sidecar |
| loom reports up | `hc send` typed in, tagged `<hc from="t7 name">` | | native transport; report kinds, bodies and files are loom's already |
| loom asks and answers | no | | labels and loom files; a mod for the display |
| Messages with a queue, list, withdraw | no store: `send` types now or refuses | | sidecar |
| Batched delivery at a state change ([051 batched delivery](../051-batched-delivery.md)) | no; a working agent gets typed text at once (`notes.rs` `can_take_note`) | | sidecar, or accept steering |
| Parent told a child is stuck or gone | **native and better than ours in one way:** notes when a child is blocked, errored or exited (`notes.rs`) | | native |
| Holding keys while a person types | **native:** ~3 s quiet gate; keys held (`input.rs`) | | native |
| Refusing to type into a dialog | native: blocked send refuses, `--force` overrides | | native |
| Agent waits on a node | `hc wait <t>`, `hc wait --children all\|any`; 180 s check-ins | | native, without deadlines |
| Waits with deadlines and deciders ([050 agent waits](../050-agent-waits.md)) | no | | sidecar |
| Readers for MR, PR, CI, issues, Slack | no; core won't add it | a mod could poll, only with the window open | sidecar |
| `loom run` command supervision | **native and simpler:** `hc run --bg --rm --no-notify`, then `hc wait` | | native |
| Annotations ([035 agent annotations](../035-agent-annotations.md)) | labels, plus `doc.get`/`doc.set` up to 1 MiB | | native |
| Task status | no; labels fit | | labels |
| Subtree close with dry run and busy guard | no; `hc kill` per terminal, with a cascade | | loom walks the tree |
| Agent state | OSC 7501 only; no detection, by rule (`AGENTS.md`) | shows it | native for agents that speak it |
| Blocked reasons | `kind`: permission, question, auth; no form, no interrupted | | native for three; form and interrupted gone or labels |
| Prose question (`⏳` sentinel) | no screen matching, by rule | | labels through `loom ask user`; the sentinel habit goes, and hooks may fill in (try it) |
| Awaiting background work (`🔄`, [028 awaiting](../028-awaiting-vs-working.md)) | no such word | | labels through a loom wait; what Claude reports during background work is untested |
| Hook authority | none needed; the report comes down the pane's own PTY | | stops being needed for state; session ids still need a hook |
| Detection manifests, overrides | none, by rule; a non-reporting program has no state | | gone; agents without 7501 show nothing |
| Done per pane, one ack | `turn` counter | `herdr.seen=<turn>` label across clients (`docs/labels.md`) | native |
| Unread report and ack | no | | labels and a mod |
| Restore with resume after a reboot | no: a kept terminal gets a fresh shell and "never reruns a task" | "doesn't restart an agent or resume its work" (site FAQ) | sidecar, types the resume in |
| Interrupted turn shown as blocked ([053 settled status](../053-restore-settled-status.md), [063 dead blocks](../063-restore-dead-blocks-as-interrupted.md)) | no; `stopped_by_restart` etc. describe the program | | sidecar and a mod |
| Survive a server update without restarting programs | **native and better:** holders keep programs through core restart and crash | window can close or crash | native (the fork has live handoff, not on Windows) |
| Non-agent pane re-runs a command ([034 restart commands](../034-restart-pane-commands.md)) | no, on purpose | | sidecar |
| Workspace = effort | core refuses workspaces (`AGENTS.md` "The rule") | a named space fits, but naming needs the window ([app#43](https://github.com/herdrdev/herdr-app/issues/43), no maintainer answer yet) | mod or `herdr-app do` while the window is open; headless spawns land unnamed |
| Worktree as tab | no | ships worktree **spaces**, not tabs | clone the `worktree` mod |
| `post_create` guard, server-side `worktree create` | no | mod runs `git` | loom or a cloned mod; host-pi's settling needs a rework |
| Attention section | no | no; shipped `agents` lists by status | mod |
| Sidebar tree, scope, folds, collapsed mode | no | shipped `spaces` and `agents` differ; rail under 160 px | mod |
| Sidebar menu, report screen, reply, tell, close | no | overlays and `TextView.markdown` fit well | mod |
| Sidebar keyboard | no | not focusable as shipped; `focusable` exists | mod |
| Reorder efforts | no | **native**, drag in the `spaces` sidebar and `space.move` | native |
| Tab names follow their agent | no | tabs show a position unless named | clone `tabs` |
| Live node references, right-aligned | no | Ctrl-click peek on `t7` only; terminal text is off limits to mods | **patch to the app** (`src/terminal.rs`, `src/links.rs`), else gone |
| MR/PR live references (planned) | no | same | **patch**, else gone; a side panel is the mod alternative |
| Plugin link patterns ([036 link patterns](../036-plugin-link-patterns.md), [037 click modifiers](../037-link-click-modifiers.md), [038 hover status](../038-link-hover-status.md)) | no | no hook; Ghostty's URL pattern only | patch |
| OSC 8 in pane read ([040 pane read links](../040-pane-read-osc8.md)) | `read` is text | | gone without a patch; the `links` plugin is rewritten or dropped |
| Plugin actions | no | mod commands, `keys.json` | mod |
| Plugin `[[events]]`, `[[startup]]` | `events.subscribe` stream | | sidecar |
| Per-session sockets | `HC_SESSION`, `HC_SOCKET` | `HERDR_APP_HOME`; upstream's sandbox pairs the two (`scripts/sandbox:26`) | native; one window per lane |
| Lane env, work tokens | core keeps the env of whatever started it first ([app#490](https://github.com/herdrdev/herdr-app/issues/490)); `env` per run through the API | | launcher plus API; a real trap for the work lane |
| Pi sandbox state reports | **better:** OSC goes through the container's TTY with no bridge | | native (the wrapper must not filter it; unverified) |
| Pi sandbox control | API socket can run commands and write files (`docs/security.md`), so an allowlist bridge is still needed | | rewrite `scripts/herdr-bridge` and allowlists |
| Host-pi gate, native `herdr` tool | no | | rewrite the gate's command list for `hc` |
| Multi-client shared focus | n/a | focus and zoom are per screen, structure is shared | changes the model; probably fine |
| Copy mode, vim selection | no | none | gone, or a custom mod over `hc read` |
| Prefix keys | no | popup mod | mod |
| Notifications that focus the pane | no | in-app toast plus sound, click goes there; a search of `src/` found no OS notification code | mod with `exec terminal-notifier` |
| Five boxes | builds for all | Apple Silicon, Linux x86/arm, Windows x64 | native, WSL and fedora untested |
| loom on Windows | n/a | n/a | gone, as today: loom is bash, `platforms = ["linux", "macos"]` |
| Fork config overlay, shared config with stock | n/a | layouts and settings are files | replaced |
| Multi-machine sidebar | **native** (`hc machine add`) | **native** | native; the TUI has its own, and the dotfiles show no use of it |

## 3. Overlap with loom

[ADR 0004](../../../docs/adr/0004-loom-runtime-in-herdr.md) lists the runtime half the fork moved into herdr. Against
core:

| runtime fact | core | verdict |
| --- | --- | --- |
| stable identity | terminal address, kept through `keep` | partial |
| Parent | yes, immutable, kills children by default | mostly |
| spawn args and env on resume | no | missing, and out of core's stated scope |
| message queue per agent | direct send plus notes | missing |
| closing a subtree | cascade on exit; no dry run or guard | partial |
| task status | no | labels |
| annotations | labels | yes |
| waits | `hc wait` | basic; no deadlines or readers |
| reading a pane | `hc read`, plain text | yes |

So core replaces about a third of the runtime half outright and part of another third. What it adds that
we lack: notes on stuck children, holders that outlive the server, and `hc run` for slow commands.

**Is it "close enough"?** The nouns are close and the contracts differ in ways loom leans on. Three matter
most:

1. Core's model is "type it now or refuse"; ours is a durable queue delivered at a state change.
2. Core has no effort or workspace, and refuses one.
3. State is whatever the program said; ours is screen plus hooks plus task status.

**Could loom sit on `hc`?** Yes, technically. It's JSON-RPC over a socket, with a schema guarded against
breaks, and `hc` prints JSON when piped. What remains is a **rewrite of loom's herdr-facing half**, not a
port:

- `.config/herdr/plugins/loom` has 7,388 lines in `lib` and `bin`. The call surface is about 35 `herdr`
  commands, counted from `tests/stub-herdr`. About half have a direct `hc` equivalent, so a shim would hide
  semantic changes rather than solve them.
- The plugin manifest's `[[readers]]`, `[[events]]`, `[[startup]]` and `[[panes]]` have no host in core.
  They become a **sidecar**: a loom process in a kept `hc` terminal subscribing to `events.subscribe`. It
  owns the queue, deadlines and readers. It's a reasonable design, but it's the "beside herdr's own state"
  arrangement ADR 0004 moved away from, and it brings back some of the races that ADR names.

**The policy half** (efforts, trackers, routing, limits, briefs, preambles) ports as it is.

**Rust loom (`tools/loom`).** It's a client of the fork's `wait.*` API, so most of it goes. `loom run`'s
supervisor can be dropped for `hc run`, which also closes its open gap: `hc` terminals are host-supervised,
whereas the README says sandbox runs aren't supported today. The deadline and decider logic moves to the
sidecar, along with the logic of about 4,200 lines of fork server code (`src/app/api/waits.rs`,
`src/app/waits.rs`, `src/app/wait_readers.rs`).

**The host-pi and sandbox bridge (`scripts/herdr-bridge`, 533 lines).** Its state-report half goes away,
since pi talks through the PTY. Its control half stays, because core is explicit that anyone reaching its
API can run commands. The allowlist needs care: core's API also carries `machine.exec`, `file.write` and
`file.edit`.

## 4. Options, in tickets

The fork's 66 tickets (001–066) against option (b). **No Rust ports as it is:** core's rule is "Fresh code",
the APIs differ, and the designs and tests port only as specifications.

| outcome | count | tickets |
| --- | --- | --- |
| process, CI, tests, parked; redo only if forking | 13 | 001 upstream sync, 002 CI releases, 003 tap and cutover, 014 loom migrate, 020 keys parked, 021 manifest catalog, 022 dogfood session, 025 and 027 flaky Windows tests, 042 test process leak, 043 flip measurements, 055 test dirs, 056 handoff pgrep |
| stop being needed | 17 | 004 read layer, 005 state consistency, 006 Claude hooks, 007 done semantics, 016 repo facts, 019 sidebar pane, 024 dogfood identity, 026 prompt when blocked, 029 last user input, 041 token check, 044 sandbox pi restore, 045 flip ordering, 046 false input, 048 sidebar drag, 054 plugin action lifetime, 062 reorder efforts, 066 pi authority lost |
| small: labels, a command or a cloned mod | 12 | 008 post-create, 009 tab names, 013 task status, 015 workspace effort, 017 worktree tab, 023 seeded worktree, 032 loom stops mirroring, 033 cross-effort spawn, 035 annotations, 039 source links, 049 notifications, 057 worktree hardening |
| real rewrite: loom, sidecar or mod | 19 | 010 identity and Parent, 011 resume args, 012 messages, 018 sidebar content, 028 awaiting, 030 sidebar program, 031 sidebar dotfiles, 034 restart commands, 040 pane read links, 047 questions section, 050 waits, 051 batched delivery, 052 attention, 053 settled status, 058 done vs unread, 059 popup menu, 060 report view, 063 dead blocks, 064 continue |
| only by patching the app, otherwise gone | 5 | 036 link patterns, 037 click modifiers, 038 hover status, 061 node references, 065 who and where |

The counts are my judgement from titles and resolutions, and a few could shift a bucket.

### (a) Stay on the TUI fork

**Cost:** 0 tickets to stay. Add an ongoing cost that is now ours:

- the detection catalog (upstream's last change was 2026-10-13);
- vendored libghostty bumps;
- anything else the TUI would have fixed.

**Thrown away:** nothing. **Missed:** the app's surfaces, holders, the mods gallery, and upstream's attention.

### (b) Stock app and core, mods, ported loom

| workstream | tickets |
| --- | --- |
| Lanes: `HC_SESSION` and app home per session; work env; launchers (`hp`, `hw`, `hf`); telemetry and update policy | 3 |
| loom's herdr-facing half on `hc`, with `stub-herdr` and its tests rewritten | 8 |
| Sidecar: events, queue and batching, waits with deadlines and deciders, readers | 6 |
| Restart: resume after a reboot, interrupted turns, non-agent panes | 3 |
| Status: check each harness, replace the sentinels and awaiting | 2 |
| Mods: tree and attention, menu and report, close flow, worktree tab, tab names, notifications, keys, look | 6 |
| Pi: bridge, both allowlists, host gate, native tool, worktree settling | 4 |
| Docs and agent instructions: `AGENTS.md` sentinel sections, `docs/setup/herdr.md` (2,142 lines), routing, skills | 3 |
| Cutover: moving live lanes, five boxes, WSL and windows decisions | 5 |
| **Total** | **about 40** |

**Thrown away:** the fork's roughly 27,600 added lines in `src/`, `herdr-sidebar` (about 10,000 lines of
Rust, tests included), node references (or keep them through a patch, which is option c), the planned MR/PR references,
the `links` plugin as it works now, copy mode, kitty as the terminal. **Gained:** see section 1.

The fork took 66 tickets in three weeks, so 40 is not absurd. But it would all be against a 0.1.x runtime
that also hosts every agent doing the work, and it delivers parity plus a new surface.

### (c) Fork the app and/or core

(b), plus:

| extra | tickets |
| --- | --- |
| Release CI for two repos; macOS signing, since core's privacy behaviour relies on a signed bundle; updaters pointed away from `releases.herdr.dev` | 4–5 |
| Node references in `src/terminal.rs`, MR/PR references | 3–4 |
| Link hooks | 2 |
| Moving queue, waits and restore into a core fork instead of a sidecar (replaces the sidecar and restart lines, so net roughly level) | 0–3 |
| **Total** | **about 50–55** |

**Recurring cost:** merging about 20 app commits and 9 core commits a day at today's rate, into files like
`src/window.rs` (5,571 lines) and `src/terminal.rs` (3,590). Upstream's rules also run against us:
"Core is a runtime, not a multiplexer", and "this is the one most likely to be helpfully added; don't"
about detection. A fork that only patches the app's rendering is plausible; one that puts runtime in core
is a permanent fight.

### (d) Hybrids

| hybrid | size | verdict |
| --- | --- | --- |
| **TUI fork plus OSC 7501** (section 6) | 2–3 | **Do it.** Upstream's agents already speak it, so it hedges the detection risk and removes a class of pi bug, whichever way the bigger decision goes |
| **App beside the TUI, isolated** (own `HC_SESSION`, own app home) | 0–1 | **Do it as a trial.** Agents aren't shared, so it's for looking, not working |
| **TUI on core**: [andymai/herdr `feat/core-backed-panes`](https://github.com/andymai/herdr/tree/feat/core-backed-panes), +4,541/−269 over 86 files, 11 ahead and 39 behind | 10–15 on top of our fork | **Wait.** It proves panes can attach to core and share agents with the app. But our changes sit on the same restore and state code, two runtimes would hold the truth, and upstream calls the question open |
| **Sheprd over the fork's server**: third-party GPUI client that attaches to Herdr 1's binary client socket and also to core | 0–1 to try | **Optional look.** Nice UI with no migration in principle, but it's unaffiliated and busy (144 open issues), and from its README it draws its own sidebar, so it likely won't show our API's facts. Compatibility with fork builds is unverified |
| **Core with no app**: kitty plus `hc attach` | — | **No.** `hc` does one terminal at a time, with no layouts |

## 5. Upstream's direction

**Commit rate** (`git log` on each default branch):

| | window | commits | per day |
| --- | --- | --- | --- |
| TUI | 09-15 to 10-15 | 204 | 6.8 |
| TUI | 10-16 to 11-05 | 15 | 0.75 |
| TUI | since `ee5bfb50` on 10-29 | 0 | 0 |
| app | 10-23 (first public commit) to 11-05 | 283 | about 20 |
| core | 10-23 (first public commit) to 11-05 | 129 | about 9 |

TUI by ISO week, W41 to W45: 36, 29, 10, 5, 0. Its last release is 0.9.5 on 10-14, and its last merged PR
landed 10-26. Six later PRs were closed unmerged (#5821, #5809, #5754, #5753, #5743, #5605). Issues and
discussions keep arriving, and `bc8c2898` (10-28) added monthly auto-closing of stale issues.

**Do fixes flow between repos?** No. `herdr-core/AGENTS.md`: "Fresh code. Do not copy from Herdr 1… Read it
for ideas, lessons and edge cases." Core also builds fresh Ghostty, not the TUI's vendored copy. `herdr-core`
shares no crate with the TUI, and the TUI has no dependency on core. Same-shaped fixes get written twice:
TUI `3f5032c9` send appearance after background, and core 0.1.16 #46 colours on theme change.

**What the maintainer has said:**

- "whether the tui moves onto herdr-core later is still open, i'll update here when that's decided"
  ([app#63](https://github.com/herdrdev/herdr-app/issues/63), 10-29).
- The site says the TUI "keeps working". There is no promise beyond that, and no statement of end of life
  either.
- [core#115](https://github.com/herdrdev/herdr-core/issues/115), a third-party orchestrator author asking
  for the same clarity we need, had no maintainer reply when read.
- Context, not a statement about the TUI: a [$6M seed in September](https://herdr.dev/blog/herdr-raised-a-seed/)
  (hiring now closed) and YC, and the site's front door is now the app. All the visible work is there.

**Would churn hurt at 0.1.x?** Two different answers:

- **The core API is well defended.** `schema/README.md` says "The API only grows", a test compares against
  `released.openrpc.json`, and across 10 release copies no `"name"` line was ever removed. It's marked
  preview, generation 1, and it shipped 10 releases in about 13 days.
- **The behaviour is still moving.** Recent changelog lines: 0.1.16 terminals froze when the core fell
  behind; 0.1.17 the core froze every terminal while saving, and macOS quit stopped agents; 0.1.18 the core
  became its own macOS app, and terminals opened before it need reopening once. These are fast and honest
  fixes, and they are also exactly the failures a runtime hosting every agent can't afford. Expect it to
  settle, but it hasn't yet.
- **The app document** that holds spaces and tabs has no compatibility promise for outside readers
  ([app#43](https://github.com/herdrdev/herdr-app/issues/43) unanswered), so loom shouldn't write it.

## 6. OSC 7501

**The spec.** [Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status), revision
0.4, 2026-10-10, published on the Rex docs; the page names no author. In short:

- `ESC ] 7501 ; key=value:key=value ST`.
- `state` is one of idle, working, done, blocked, error, or clear.
- `kind` is permission, question or auth, with blocked only.
- Also `progress`, `app`, base64 `title` and `msg`, and a slash-separated `id` for child records.
- Each report replaces its record. The terminal drops `working` and `blocked` when the process exits or at
  OSC 133 `A`.
- `?` is a support query. Reports can't be read back.

**Who speaks it, checked:**

| | evidence | sends without a reply? |
| --- | --- | --- |
| Claude Code | 2.1.318 binary: probes `OSC 7501 ; ?` ("no reply to OSC 7501"), `app=claude-code`, reports blocked with `kind`. The maintainer says "since 2.1.295" ([app#488](https://github.com/herdrdev/herdr-app/issues/488)) | **no**, it probes |
| pi | `packages/tui/src/program-status.ts`, tui 1.1.0 on 2026-10-07; `docs/terminal-setup.md` lists the states; local pi is 1.1.0 | **no**: "only after the terminal answers" |
| Codex | 0.166.0 binary holds `]7501;state=clear`; core's manual needs `--no-daemon` | unknown |
| opencode 2 | maintainer ([app#489](https://github.com/herdrdev/herdr-app/issues/489)) | unknown |

Third-party sources also name Ghostty 1.4 and others as implementers; I didn't verify those.
([termstate-core](https://docs.rs/crate/termstate-core/latest/source/README.md),
[Optio release notes](https://github.com/jonwiggins/optio/releases/tag/v0.21.0),
[zeli.app write-up](https://zeli.app/story/49984159).)

**Does it matter if we stay? Yes, for two reasons.**

1. **Upstream's detection catalog has stopped moving.** Its last change was 10-13, against roughly weekly
   before, and a CLI redesign now breaks our screen rules with nobody upstream to fix them. Native reports
   are the way out.
2. **It removes [066 pi authority lost](../066-pi-lifecycle-authority-lost.md)'s failure
   class.** 066 comes from the claim, the generation gate and the bridge disagreeing with a process probe.
   A report down the pane's own PTY needs no claim, comes from the process in that pane by construction, and
   crosses the docker TTY with no bridge. It also answers 066's design question: for state, no separate claim
   is needed. Session ids for resume still need the existing report.

**Today neither agent reports in fork panes (inferred from source, not observed).** The vendored libghostty-vt
is `44f2a44d` from 2026-09-10. Ghostty's parser arrived in `bae2c3cd` on 2026-10-06, last changed in `dcaf6984`
on 10-21. `rg 7501 src` finds nothing in the fork, so the query goes unanswered, and Claude and pi stay silent.

**Cost: about 2 tickets, and the answer to the query is what turns it on.**

1. Parse and answer. Backport the parser as a local libghostty patch (a 7th active patch, using the existing
   patch process), or bump the vendored Ghostty past `dcaf6984` (cleaner, larger: six patches to reapply). A
   hand-rolled scanner would put new work on the per-byte PTY path; avoid it.
2. Make it a state source. Decide its order against hooks and manifests. Map `kind` onto blocked reasons,
   with `auth` as a new value (a new enum value needs an `Unknown` fallback under the endpoint contract). Keep
   form and interrupted as they are, keep the sentinel rules, and retire pi's bridge state reports in dotfiles.
   Then test Claude, pi and Codex per the live-smoke rules in `AGENTS.md`.

Neither has been built, so "2" is an estimate. The authority ordering is the part that could grow.

## 7. What was not settled by reading

- Everything under "could not judge" in section 1.
- What Claude and pi report in the turns the fork cares about: a question asked in prose, a turn ended with
  background work running, and sub-agents. If they stay `working` or go `blocked` correctly, the sentinels
  matter less in any option.
- Whether the pi sandbox wrapper or docker filters the sequence.
- Whether Sheprd connects to a fork build.
- `src/window.rs`, `src/terminal.rs` and `src/vt.rs` were sampled, not read through, so patch sizes on them
  are estimates.
