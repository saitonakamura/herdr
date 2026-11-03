---
id: herdr-fork-node-reference-says-who-and-where
title: A decorated agent id names the agent and its effort, right-aligned
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/w1:p27/47cc25f3-f977-461d-a34e-b20e8215042a
---

## Question

Follow-up to [061](./061-live-node-references.md), from the user on 2026-11-03 after seeing it live ("is the status misplaced or its by design?"). In a Claude transcript line naming `58608ba4` mid-sentence, the end-of-line text `idle` landed two cells after the sentence and read as part of it; only the id's colour tied the two together. And 061's pane part (`asking · wN:p2`) is no help: the user navigates by workspace and tab names, not by `w…:p…`.

An agent id also looks exactly like a git commit hash to a reader. The decoration is what tells them apart, so it should say who the id is.

Talked through and not chosen: inserting the text inline (herdr only paints cells the program laid out; shifting a line breaks mouse, selection and position-drawn TUIs), a `loom:` prefix convention, and pane ids in prose. An OSC 8 slot reserved by the program (pi) stays a possible later addition.

## Decided with the user (2026-11-03)

- **What it says:** the agent's name (the sidebar's row name: its tab label, or its own title for a second agent in a tab), then its effort or workspace label when that differs from the pane being drawn, then its state: `main-tracker · idle`, `pimig-002 · pi-migration · asking`, `closed`. The pane id goes.
- **Where:** right-aligned to the pane's last column, in the line's trailing blank cells, dimmed, so it reads as an annotation rather than as text. Keep at least two blank cells between the line's text and it.
- **When it doesn't fit**, drop the effort, then the name, then fall back to the state alone, then recolour only (061's full-line rule).
- **Several ids on one line:** 061's rule (one segment per distinct id, in line order) with the new segments, right-aligned as a group; the first that doesn't fit ends the text.
- Matching, colours, Ctrl+click, the redraw rules and `pane read` staying undecorated are 061's and stay as they are.

## Parent

[herdr-fork map](./000-map.md)
