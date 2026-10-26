---
id: herdr-fork-flip-report-after-colours
title: Forward a light/dark flip only after the new colours have landed
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:p14/814e177d-60ce-4d73-a96d-7abaa7bd3bef
---

## Question

[043](./043-light-dark-flip-chain.md#resolution) measured the cause. When kitty
reports a flip (`CSI ?997;1|2n`), the client sends `ClientHostThemeUpdate::Appearance`
to the server at once, and only then queries kitty for the new colours. The server
writes `?997` into every subscribed pane while `host_terminal_theme` still holds the
old colours. A pane that asks `OSC 11 ; ?` straight away is told the old background
for 0–100 ms. Claude and pi ask within about 1 ms, so they lose most flips (4 of 6
samples).

Do what 043 recommends, in the client:

- **The order.** On kitty's `?997`, write the colour query first. Send `Appearance`
  to the server only once kitty's `OSC 11` reply has been forwarded, or after a
  timeout of about 250 ms if no reply comes. The measured worst case was under 50 ms.
- **No new state or wire.** The client→server stream is ordered and applied in
  sequence (`headless.rs`, `ClientShellHostTheme`). So the same messages, sent in a
  different order, are the whole fix. Keep to the stable client endpoint contract
  in AGENTS.md.
- **Keep from today.**
  - `clients.rs::set_host_appearance` avoids inferring an appearance once one is
    explicit. Make sure the explicit appearance from the report is not overridden
    by inference from the new background that now arrives first.
  - `record_host_theme_update` keeps the last update of each kind for replay. A
    reconnect must replay in the new order too.
- **Tests.** Engine and client tests for the order, the timeout, and the
  explicit-over-inferred rule.
- **Live verify.** Use the probe in
  [assets/043-flip-chain-measurements.md](./assets/043-flip-chain-measurements.md),
  in a throwaway session on a release build of the branch. The stale window must be
  0 in every sample over several flips, and a pre-flip Claude must follow each one.
  Ask the user before each flip, and leave Appearance on Auto afterwards.

Also:

- **codex.** It never follows a flip (043). Spend a few minutes on codex's issues
  and config for a theme option or 2031 support. If there's nothing, document in the
  dotfiles `docs/setup/theme.md` that a codex pane needs a restart after a flip.
- **Retire the mac nudge.** Once this is merged, the macs no longer need
  `herdr-theme-nudge`. Every mac session runs the fork, and the nudge does nothing
  there anyway (043). On a dotfiles branch:
  - remove its launchd wiring and the mac rows from `docs/setup/theme.md`;
  - keep the script and the fedora systemd units, because fedora still runs stock;
  - update `terminal-coding-flow/007` to point here;
  - write one on-box ticket per mac for unloading the agent
    (`~/dotfiles/CLAUDE.md` cross-host rules).

## Resolution

Draft until [saitonakamura/herdr#31 forward appearance after background reply](https://github.com/saitonakamura/herdr/pull/31)
merges. Branch `fork/045-flip-report-after-colours`, `d0e50067`. CI is green on
ubuntu, macOS and Windows.

**The fix, as asked, in the client** (`src/client/shell/input.rs`, `state.rs`).
On kitty's `?997`, the colour query goes out as before, but the appearance is
held. It is sent right after the next background reply has been queued. If no
reply comes, the client loop timer sends it alone after 250 ms
(`tick_held_host_appearance`, `timer_delay`). A second report while one is held
keeps the first deadline and carries the newest side. Every `?997` already sets
`query_host_theme`, including the answer to `?996n` at startup and on focus-in,
so the hold always has a reply coming. That keeps startup and focus-in at one
round trip, not 250 ms.

Two smaller changes:

- **Replay** (`src/client/state.rs::host_theme_replay`): a reconnect sends the
  retained colours before the appearance, whatever order they were recorded in.
  Without that, a later background re-query, a resize say, would be recorded after
  the appearance and replayed after it.
- **Server order** (`src/server/headless.rs`, the theme update and foreground
  promotion): the colours are applied to the panes before the appearance. The
  in-order stream already guarantees this across messages. This change also
  covers the case inside one message, where a background infers a new appearance.

The explicit-over-inferred rule is untouched on both sides. The background that
now arrives first does not override an explicit appearance, in the client
(`host_appearance_explicit`) or the server (`clients.rs::set_host_appearance`).
There is no new wire, codec, method or persisted state.

Tests: the order (`host_appearance_report_follows_the_background_reply`, and the
same in a single read), the timeout and a second report while held
(`held_host_appearance_is_sent_alone_after_the_timeout`), explicit over inferred
in the client (the updated `host_appearance_prefers_explicit_reports_over_background_inference`)
and the server (`client_shell_flip_background_before_report_keeps_the_explicit_appearance`),
and replay order (`host_theme_replay_sends_colours_before_the_appearance`). Local
`just check`: fmt and clippy are clean. The only failures are this box's known
ones: the macOS ACL test (GNU chmod), fake-ssh, `api_ping` done status, the
gpgsign tag fixtures, and `server_survives_hangup_and_logs_why_it_stops`, which
times out under full parallel load and passes alone.

**Live verify**, a release build in a throwaway session, four real flips
([assets/045-flip-measurements.md](./assets/045-flip-measurements.md)). The stale
window was 0 in all four probe samples: the first `OSC 11` after each report was
already the new side, 0–14 ms later. Both pre-flip Claude panes followed every
flip, with all 14 markers on the new side each time. Appearance was left on Auto
(`AppleInterfaceStyleSwitchesAutomatically = 1`).

**codex.** Checked on 0.164.0. It still has no mode 2031 and no re-query (the
binary holds the `OSC 11` probe and `?1004h`, but no `?2031`). Upstream tracks it
in [openai/codex#38575 ask codex to set DECSET 2031](https://github.com/openai/codex/issues/38575)
and [openai/codex#45163 startup-only palette cache](https://github.com/openai/codex/issues/45163).
There is no config option. So the dotfiles' `docs/setup/theme.md` now says a
codex pane needs a restart after a flip.

**Mac nudge retirement**, on dotfiles branch `herdr-fork/045-flip-report-after-colours`
(`ee84144`, not merged). The two launchd plists are gone. `theme.md` swaps its mac
rows and recipe for a retired note with the unload steps. The script, its swift
helper and fedora's systemd units stay. `terminal-coding-flow/007` points here.
The on-box tickets are `terminal-coding-flow/009` (unload the primary's two loaded
agents) and `010` (check the music mac, which the repo says never had them).
Merge that branch only once this PR is in the dogfood binary.

## Parent

[000-map](./000-map.md)
