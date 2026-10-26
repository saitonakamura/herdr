---
id: herdr-fork-sidebar-questions-section
title: Show agents' open questions for the user as a herdr-sidebar section
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Filed 2026-10-26 from dotfiles loom 018 (loom node `82e20989`), at the user's request. A plugin can't add
a sidebar section, so this half of 018 lives here.

**What exists on the loom side.** An agent files a question for the user with `loom ask user "<q>"` and
carries on. Several can be open per agent, and nothing goes to its parent. loom keeps:

- `loom questions --json` (this lane only): `[{node, name, effort, pane, file, at, waiting_s, question}]`,
  oldest first, with closed nodes left out. This is the feed for the section.
- a pane token `questions=?N` under source `loom:questions` on the asking agent's pane, cleared at 0. The
  dotfiles `config.toml` / `config.fork.toml` rows already carry `$questions`, so the agent row shows `?N`
  today without this ticket.
- answering: the user types the answer in the agent's pane (the agent then closes the record), or runs
  `loom answer <node|ask file> "<answer>"`, which types `[loom answer from user] …` into the pane.

**Wanted here.** A "questions" section in `herdr-sidebar`, for the session's lane. Show one row per open
question: agent name, minutes waiting, the question cut to width. `enter` focuses the agent's pane. An
optional key runs `loom answer <file>` in a popup, the way `[sidebar.actions] close` runs `loom close`. The
section is hidden when there are none.

**To decide when building.** Poll `loom questions --json` (how often, and how to stay cheap under Jamf's
per-launch cost: see dotfiles loom 070) or watch the `loom:questions` token changes the server already sees
and only re-read then. The token route needs no polling.

Docs: dotfiles `docs/setup/herdr.md` § "Questions for the user are records", and
`.issues/loom/018-ask-user-queue.md` § Resolution.

## Parent

[herdr-fork map](./000-map.md)

## Blocked by

## Resolution
