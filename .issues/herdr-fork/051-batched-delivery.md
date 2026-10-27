---
id: herdr-fork-batched-delivery
title: Deliver a recipient's message backlog as one block at idle
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/w1:p19/5cd44401-169b-4d9e-9046-9a04b6182059
---

## Question

The fork side of dotfiles loom 090 (`~/dotfiles/.issues/loom/090-herdr-batched-delivery.md`), from loom 077's resolution (dotfiles `8c485f7`). The spec lives in the loom ticket; this ticket is where the fork work is tracked. Keep the two statuses in step.

Today herdr delivers one message, the oldest pending, per recipient per state change (`src/app/api/messages.rs`), so a backlog lands as N stale turns. Instead:
- When the pane goes idle or awaiting, deliver every pending message in one typed block: one line per message with sender, kind, age, preview and `herdr message show N`.
- Asks and replies keep their own line and id, so `loom reply` and `--reply-to` still work.
- Past 5 pending, the block is a single pointer line: "K messages pending from A, B, C: `loom inbox`".
- Each line carries its queue age (loom 081 item 3).
- No delivery mid-turn.

Evidence: dotfiles `.issues/loom/assets/agent-messaging-research.md` §6A. One turn instead of N recovers 95%.

Fork notes:
- Decide how a block marks its messages delivered: all at once when the block is typed, or one by one. A `withdraw` racing the block must not drop a message silently.
- `awaiting` delivers at once (fork 028), so an awaiting pane gets the block on the next tick.
- The typed block is input to the agent, so it must not set `last_user_input_at_ms` ([046](./046-false-user-input.md)). API writes already don't (029).

## Parent

[000-map](./000-map.md)

## Resolution
