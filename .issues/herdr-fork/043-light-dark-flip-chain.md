---
id: herdr-fork-light-dark-flip-chain
title: Re-measure the light/dark flip chain, kitty to herdr to each harness
status: closed
labels:
  - wayfinder:research
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/4a781e51-3692-40d3-bf38-99874d1afc9d
---

## Question

A light/dark flip still leaves agent panes on the old side. Dotfiles
`terminal-coding-flow/007` (`~/dotfiles/.issues/terminal-coding-flow/007-claude-theme-lags-through-herdr.md`)
measured this for Claude Code in August, on stock herdr. It found that herdr
forwards the `CSI ?997` colour-scheme report to panes before it has re-read the
host's colours, so an `OSC 11 ; ?` sent right after the report gets the old
background. Fork master still has that order (`src/app/theme_sync.rs`). The user
suspects codex has the same problem, and pi is unknown.

Measure again, on the fork build, before anything is fixed. Go in this order and
stop at the first link that breaks:

1. **kitty.** On a macOS appearance flip, does kitty switch palettes and send
   `CSI ?997;1|2n` to the herdr client? Is a client in a kitty window that was
   unfocused during the flip handled too?
2. **herdr itself.** Does herdr's own UI switch between `tokyo-night` and
   `tokyo-night-day` (`auto_switch`)? Does the sidebar pane (`herdr-sidebar`)
   follow? After the flip, what does a pane get for `OSC 10/11 ; ?` and for
   `CSI ?996n`, read raw with `stty raw` at intervals from the flip? Bisect the
   window where the answer is still stale, which 007 bounded only to (0, 1000] ms.
   Does a pane get the `?997` report only if it enabled mode 2031?
3. **Each harness**, one pane each, started *before* the flip:
   - Claude Code with `theme: "auto"`.
   - codex.
   - pi, which on the mac runs in its linux container through
     `~/dotfiles/.config/agents/bin/pi`.

   For each, record whether it enables mode 2031, what it queries after the
   report and when, and which side it ends up on. Judge the side by counting
   colours across the whole visible screen against ground truth from a forced
   theme, the way 007 does. Do not judge it by eye or from a few lines. Also
   record whether a pane started *after* the flip comes up right.
4. **The nudge.** Does `~/dotfiles/scripts/herdr-theme-nudge` rescue each of the
   three, or only Claude?

Use the methods in 007, and avoid the three it lists as failed. Do the work in a
throwaway named session on the installed fork binary, attached in its own kitty
window. Stop the session afterwards. A flip changes the user's whole desktop, so
ask the user before each flip, or ask them to flip it.

## Answer wanted

A table of every link above for each harness: works, broken, or partly. For each
broken link, name the cause with evidence, and say whether herdr can fix it.
Then give a recommendation for the fix that the follow-up task will carry out.
The candidates so far are below, and the measurements may rule any of them out:

- hold the `?997` report until the new colours land, with a timeout;
- keep the last theme seen for each appearance and apply it before forwarding;
- push `OSC 10/11` into panes together with the report.

Also say what each harness needs beyond herdr, if anything.

## Resolution

Measured 2026-10-26 on the primary mac across two real flips (dark → light, light →
dark), in four throwaway sessions on `0.9.5+fork.1`. Each session had its own kitty
window and a logging relay between kitty and the client. The raw timelines, the
census markers and the probe script are in
[assets/043-flip-chain-measurements.md](./assets/043-flip-chain-measurements.md).

**kitty and herdr's own switch work.** The stale `OSC 10/11` window is still there on
the fork. It loses the race for both Claude and pi in most flips, so they keep the
pre-flip side. codex never follows at all, and pi as the user stores it is pinned.

| link | Claude (`auto`) | codex | pi |
| --- | --- | --- | --- |
| 1. kitty reports the flip to the herdr client, focused or not | works | works | works |
| 2a. herdr UI `auto_switch` (tokyo-night ↔ tokyo-night-day) | works | works | works |
| 2b. sidebar pane follows | works | works | works |
| 2c. `?997` only to panes with mode 2031 | works | works | works |
| 2d. `CSI ?996n` after the report | works | works | works |
| 2e. `OSC 10/11` after the report | **partly**: stale for 0–100 ms after the report in 4 of 6 samples | same | same |
| 3a. harness enables 2031 | yes | **no** | **no** as stored (`"light"`); yes with `light/dark` |
| 3b. harness re-queries after the report | `OSC 11` within 1 ms | **never** (not on focus-in either) | `OSC 10/11/4` at once (with `light/dark`) |
| 3c. side after a flip, pane started before | **partly**: right when herdr won the race (flip 1, A), wrong in all three panes on flip 2 | **broken**: stays on its startup side | **broken** as stored (always light); **partly** with `light/dark`, like Claude |
| 3d. pane started after the flip | works | works | works (as stored: light only) |
| 4. `herdr-theme-nudge` rescues it | yes | **no** | yes with `--pane`, for `light/dark` only; the script picks `claude` panes only |
| 4'. the installed launchd nudge runs on this mac | **broken** for all three, see below | | |

Notes on each row:

- **1.** In both flips kitty sent `?997;1|2n` to all four clients within 8 ms of
  each other. No kitty window had focus at either flip; no focus-in shows in any
  relay log. So the unfocused case is measured and works. A *focused* window was
  not captured: the user did not get into window A in time.
- **2a–2b.** herdr's palette switched within 60 ms of the report. The sidebar draws
  the terminal-default background and ANSI indices, which kitty resolves, so those
  flip with it. Its configured hexes (`#757ead`, brand colours) stay as set, by design.
- **2c–2d.** A probe with 2031 off received no report in either flip. `?996n` gave the
  new side in the same round as the report.
- **2e.** These are the bisected windows that 007 bounded to (0, 1000] ms. The stale
  `OSC 11` lasted 74–98, 41–65 and 26–39 ms after the report, ≤4 ms in a fourth
  sample, and 0 in two. Claude and pi ask within about 1 ms, so any non-zero window
  loses. Session A won flip 1 (Claude and pi moved to light) and lost flip 2 (every
  pre-flip Claude and pi pane in A and B stayed light on a dark desktop).
- **3c.** A Claude that wins keeps its static welcome block in the old colours (22
  `#999999` spans after the light switch). That is Claude's static output and not
  herdr's to fix.

**Cause of 2e, with evidence.** When kitty's `?997` reaches the client, the client
does two things. It sends `ClientHostThemeUpdate::Appearance` to the server right
away. Only then does it write the colour query to kitty (1–39 ms later in the relay
logs). The server applies the appearance at once: `set_host_terminal_appearance_state`
→ `apply_host_terminal_appearance_to_panes` writes `?997` into every subscribed pane.
At that moment `host_terminal_theme` still holds the old colours, and they are what
the pane's `OSC 11 ; ?` is answered from (`pane/terminal.rs::default_color_query_response`).
kitty's `OSC 10/11` replies reach the client 26–48 ms after its report. The client
forwards each one as it parses it, and the pane sees the new background up to 107 ms
after the reply reaches the client (at most 14 ms in the fastest sample).

So the window is one round trip to kitty plus server time, racing a report that
itself takes 34–64 ms to reach the pane. It is **not** the 258-reply palette wait
that 007 and the nudge's header describe: in three samples the new background
reached the pane before kitty had finished the 256 `OSC 4` replies. herdr can fix
this fully, because it alone decides when to forward the report.

**What each harness needs beyond herdr.**

- **Claude:** nothing. It already re-queries and themes from the answer.
- **codex:** live following has to come from codex upstream (honour 2031, or
  re-query on some event). herdr cannot make it re-read; only restarting the pane
  does. Today a codex pane is right only until the next flip.
- **pi:** the stored `"theme": "light"` in `~/.pi-work/settings.json` pins it. It
  needs `"light/dark"` (or `"system"`), which is the user's call. After that it
  behaves exactly like Claude and is fixed by the same herdr change.
- **The nudge, while it exists:** on the primary mac it does nothing today, three
  ways over:
  1. its `--watch` path logged nothing for either flip, and why was not found;
  2. `mac_side()` reads `AppleInterfaceStyle`, which is absent under Auto
     appearance, so it reports light on a dark desktop, and that made flip 1 a no-op;
  3. it reaches only the default herdr session (all 54 log lines say
     `nudged 0 claude pane(s)`), while the agents live in `fork`, `personal` and
     `work`.

  These are dotfiles bugs and not this tracker's. Once the herdr fix lands, the
  nudge should be deleted, as its own header says.

**Recommendation for the follow-up task: hold the report until the new colours have
landed, with a timeout.** Do it in the client: on kitty's `?997`, write the colour
query first and send `Appearance` to the server only after kitty's `OSC 11` reply
has been forwarded, or after a timeout of about 250 ms if no reply comes (the
measured worst case was under 50 ms). The client→server stream is ordered and each
update is applied in sequence (`headless.rs`, `ClientShellHostTheme`), so a pane's
re-query then always finds the new background. This needs no new server state and
no wire change; the same messages are just sent in a different order.

Things to keep when building it:

- Today a background update already avoids inferring an appearance once one is
  explicit (`clients.rs::set_host_appearance`). Order it so the explicit appearance
  from the earlier report is not overridden by inference from the new background.
- `record_host_theme_update` keeps the last of each kind for replay, so a reconnect
  replays in the new order too.
- Only the foreground client's theme reaches the server (`headless.rs`). That holds
  either way and was not exercised here.
- Verify with the probe in the assets file: the stale window must be 0 in every
  sample, over several flips.

The other two candidates are ruled out:

- **Keeping the last theme seen per appearance** fixes only the second and later
  flips of a server's life. It answers a cached value rather than the truth, so it
  goes wrong after a kitty theme-file change. It is at most a fallback for the timeout.
- **Pushing `OSC 10/11` with the report** cannot work on its own, because herdr does
  not have the new colours when the report arrives. It would still need the hold, and
  it adds unsolicited replies into panes, which only clients that discard them
  tolerate.

The fix is a follow-up task. This ticket only measured.

## Parent

[000-map](./000-map.md)
