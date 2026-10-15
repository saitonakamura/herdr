---
id: herdr-fork-read-layer
title: Read layer: reads that agents can trust
status: blocked
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sync-and-branch
assignee:
claimed_by:
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
