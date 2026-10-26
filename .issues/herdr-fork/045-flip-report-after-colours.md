---
id: herdr-fork-flip-report-after-colours
title: Forward a light/dark flip only after the new colours have landed
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by:
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

## Parent

[000-map](./000-map.md)
