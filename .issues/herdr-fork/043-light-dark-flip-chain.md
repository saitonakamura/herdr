---
id: herdr-fork-light-dark-flip-chain
title: Re-measure the light/dark flip chain, kitty to herdr to each harness
status: in-progress
labels:
  - wayfinder:research
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:p13/4a781e51-3692-40d3-bf38-99874d1afc9d
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

## Parent

[000-map](./000-map.md)
