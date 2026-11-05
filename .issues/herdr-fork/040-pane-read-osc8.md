---
id: herdr-fork-pane-read-osc8
title: Keep OSC 8 links in pane read
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Requested by the dotfiles terminal-clickability effort, from its ticket 002 (dotfiles 3979537 / 03a5bdf).

`herdr pane read` drops OSC 8 hyperlinks in every source (`visible`, `recent`, `recent-unwrapped`, `screen`, `detection`) and every format (`--ansi`, `--raw`), measured on `0.9.5+fork.1`. The terminal keeps them: Ctrl+click reaches them through `visible_hyperlinks` (`src/terminal/runtime.rs`). The `links` picker (dotfiles `.config/herdr/plugins/links`, `prefix+alt+o`) reads the pane through `pane read`, so it cannot see the OSC 8 links that claude, `ls` and `rg` now emit. Only Ctrl+click can.

Either of these works, and the plugin's `scan.pl` already parses OSC 8 first, so neither needs a plugin change:

- **A.** `pane read --ansi` (and `--raw`) keep OSC 8 open/close sequences around the linked cells. The ANSI formatters live in `src/terminal/runtime.rs` (`visible_ansi`, `recent_ansi_snapshot`, `recent_unwrapped_ansi_snapshot`). A link that wraps or spans a styled run must still come out as one well-formed link.
- **B.** A pane links API that returns `uri`, `text` and position for each link in the requested source.

Decide between them:
- A is the smaller change, but it changes what existing `--ansi` callers receive. Check loom, the detection manifests (the `detection` source probably shouldn't change) and handoff history (`handoff_history_ansi`) before widening it. It may need an opt-in flag.
- B is new API surface: an advertised method, with neutral naming per the runtime/client boundary. It also gives Ctrl+hover-style resolution to non-TUI clients.
- Either way it is reachable from `pane read` only, not from render or detection. Keep it out of the multiplicative paths.

## Parent

[000-map](./000-map.md)

## Resolution

Merged 2026-11-02: [saitonakamura/herdr#46](https://github.com/saitonakamura/herdr/pull/46) as `5406ac7c`, with dotfiles `herdr-fork/040-pane-read-osc8` (`ce93692`).

**A, behind an opt-in flag.** `herdr pane read --links` (API `pane.read` with
`format: "ansi"`, `links: true`) wraps each hyperlinked run in one OSC 8 open and
one close, through soft wraps and style changes, for `visible`, `recent`,
`recent-unwrapped` and `screen`. `detection` stays plain. `--links` implies ANSI.
Servers advertise `pane_read_links`.

Why A over B:

- **The consumer already reads it.** `scan.pl` parses OSC 8 out of `pane read --ansi`
  output, so A costs the `links` plugin one guarded flag. B would have meant a new
  method plus a second stream for the plugin to merge with the text it already
  scans, by position.
- **No change for current callers, because of the flag.** Without `--links`,
  ANSI output is byte-for-byte unchanged. I checked the existing callers. loom
  never calls `pane read`. The detection manifests read the `detection` source,
  which is plain text and never reaches the new code. Handoff history
  (`handoff_history_ansi`), waits and output subscriptions all pass
  `links: false`. In the dotfiles, the only other caller is `llama-suggest`
  (`--source recent`, plain), and the `pi-work` bridge allowlist leaves
  `--links` out, which is fine because the plugin runs on the host.
- **The cost is one vendored patch.** libghostty-vt's formatter already tracked
  per-cell hyperlinks, but emitted them only for HTML. Patch
  `0009-vt-formatter-hyperlinks` adds a default-off `hyperlinks` option that
  reuses that tracking for VT. It is indexed in
  `vendor/libghostty-vt.patches.md` with its removal condition. Building OSC 8
  in Rust instead would have meant splitting formatter calls at link boundaries,
  which breaks trailing-space trimming, or reimplementing the VT formatter.
- **Wire.** The change is additive. An old server ignores `links` and reports
  success, so the stable endpoint contract calls for a separately advertised
  capability (`pane_read_links`). Stock 0.9.4's CLI rejects `--links` with
  `unknown option`, so callers probe the capability first.
- **Not in the multiplicative paths.** Only `pane.read` sets the option. Render,
  detection, parsing and fanout never do.

Known shape: when a link ends a hard line with trailing blanks, its close lands
at the start of the next row, before any cell. The stream is still well formed,
and per-line consumers such as `scan.pl` only need the open.

Consumer: dotfiles branch `herdr-fork/040-pane-read-osc8` (`ce93692`). `bin/links`
passes `--links` when `herdr status server --json` reports `pane_read_links`.
This was checked against real `pane read`s in throwaway sessions. On the fork
debug build the picker listed `https://example.com/osc8-target`, a link whose
text is "bold label". On stock 0.9.4 it ran unchanged without the flag. Both
checks also ran under `/bin/bash` 3.2.
