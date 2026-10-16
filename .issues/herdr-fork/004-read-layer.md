---
id: herdr-fork-read-layer
title: Read layer: reads that agents can trust
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sync-and-branch
assignee: saitonakamura
claimed_by: macos/w6:p6/89d59791-506a-4eb1-8841-fa352a1ac144
---

## Question

Fix the pane-read failures found on 2026-10-15 (all still open upstream):

- `--source visible` follows the human's scroll position (#2900). Add a source pinned to the bottom of the live screen regardless of any scroll offset and make it the default for agent-facing reads; keep the old behaviour available.
- `revision` is hard-coded to 0 on every read (#2805; `src/app/api/panes.rs`, `agents.rs`, `server/alt_screen_read.rs`). Return the terminal's content sequence.
- On the alternate screen, `recent` reads take the last N physical rows unanchored, so small `--lines` reads are blank. Anchor them at the last content row or cursor, like the primary screen.

Leave the wheel-scroll alt-screen history read alone. Switch loom's reads (`lib/spawn.sh`, `lib/report.sh`) to the pinned source in the same change.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sync-and-branch

## Resolution

_Draft, until [PR #4](https://github.com/saitonakamura/herdr/pull/4) merges._

- `--source screen` on `pane read` / `agent read`: the live screen, bottom-pinned whatever a client has scrolled to. `visible` still follows the client's scroll, and docs/next, the CLI help and the API schema now say so and point agents at `screen`.
- **No default changed**, which corrects the question's "make it the default for agent-facing reads" (and the map's wording). `agent read` keeps `recent`, because that is how agents read another agent's reply: it needs the scrollback and the alt-screen history read, and `recent` was already bottom-anchored, so it was never the #2900 bug.
- `revision` is the terminal content sequence, read under the content write lock so it matches the returned text (#2805).
- Alt-screen `recent` ends at the last content row, or at the cursor on a blank screen. The wheel-scroll history harvest is untouched.
- loom: `pane_screen` in `lib/core.sh`, used by `spawn.sh` and `report.sh`, falls back to `visible` where herdr refuses `screen` (dotfiles `a976679`). On-box: each box gets `screen` once a fork build carrying this is installed and its server restarted. Until then loom reads `visible`, as before.
