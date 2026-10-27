---
id: herdr-fork-batched-delivery
title: Deliver a recipient's message backlog as one block at idle
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/5cd44401-169b-4d9e-9046-9a04b6182059
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

Built 2026-10-27 on `fork/051-batched-delivery` (`src/app/messages.rs`, `src/app/api/messages.rs`).

**What is typed.** At the state change that lets a pane take messages (idle or awaiting with no draft, blocked on a question with an empty composer: the gate is unchanged), herdr types everything pending for it as one submission, oldest first:

- one message: one line, `[from worker] done: tests pass (report, 12m ago, herdr message show 7)`. The `[from X] <summary>` prefix is kept, so loom's preambles still describe it; the parenthesis adds the kind (`reply to N` for a reply), the queue age (`just now`, `Nm`, `Nh` below 48h, then `Nd`, loom's footer units) and the pointer.
- two to five: a `[herdr] N messages, oldest first:` header and a line each, so every message keeps its own id.
- more than five: one line, ``[herdr] 7 messages from A, B, C, D and 2 more, the oldest sent 2h ago: `loom inbox` lists them, or `herdr message list --to <agent>` ``.

Lines are joined with newlines inside a bracketed paste. A pane that has not enabled bracketed paste gets them joined with spaces, since a typed newline would submit the block one line at a time. Still at most one block per state change, so nothing reaches a pane mid-turn.

**How a block marks its messages delivered: all at once, when the block is queued.** Right after `queue_agent_submission` accepts the block, every message in it is marked delivered with the same `delivered_at_ms`, and `message.delivered` is emitted once per message, oldest first. Marking them one by one would mean nothing: the block goes in as one write.

**A withdraw racing the block.** Building the block, queueing it and marking it run in one step on the App loop, and `message.withdraw` runs on the same loop, so they never interleave. A withdraw before that step takes its message out, and the block never names it. A withdraw after it is refused with `message_delivered`. No message is dropped silently either way. A pointer block counts as delivering every message it covers. Otherwise they would stay pending and the pointer would come back at every state change; `loom inbox` and `message list` still show them.

**Asks are not kept apart past five.** The research (§6A) wanted asks and replies to stay non-collapsible. herdr has no ask kind, though: loom sends an ask as a `tell` that says so. So the pointer covers every kind. Up to five, asks and replies keep their own line and id. Past five they are reachable through `loom inbox`, and `loom reply` and `--reply-to` work from loom's own files either way. If pointers turn out to hide asks in practice, add an `ask` kind to herdr and have loom send it.

**loom needs no change.** It mirrors pending messages from `message.queued` and `message.delivered`, still one event per message, and reads delivery from each message's `state`. It never parses the typed line. Its stub (`tests/stub-herdr`) still emits the old single-line format, which only its own tests read.

**Not set as user input.** Delivery goes through `queue_agent_submission`, which never calls `note_user_input`. Checked live: `last_user_input_at_ms` stays null after a block.

**Shared with 050 in `src/app/api/messages.rs`.** 050's waits fire as herdr messages, so a fired wait goes through `handle_message_send`/`MessageStore::push` and arrives in the decider's next block like any other message. Three places where the two meet: `delivery_line`'s `kind` match is exhaustive, so a new `MessageKind` (say, for waits) won't compile until it has a label. A wait that must reach a busy agent sooner would change `try_deliver_message_to`'s gate, which is shared with every message. And the pointer threshold `DELIVERY_BLOCK_LIMIT` counts wait messages too.

**Validated.** Unit and API tests cover the block, bracketed and unbracketed joins, the pointer, a withdraw before and after, per-message events, and the same-state hold. Live on a throwaway `repro-051` session with a debug build and a stand-in `pi` (a node script that logs its input, no model): three messages to a working pane, one withdrawn, went in as one bracketed paste of a header plus two lines plus a `reply to 1` line, all with the same `delivered_at_ms`. Withdrawing a delivered one was refused. A send in the same idle state stayed pending. Seven pending went in as one pointer line, and none were left pending. `just check` fails on this box for reasons that predate the branch and fail on master too: `git tag` in the release/docs fixture tests (`fatal: no tag message?`, the box's tag-signing config), `windows-lint` (no Windows SDK set up), and five integration tests (`api_ping`, `remote_attach`, `live_handoff` (flaky), a letta hook test, a macOS ACL test). PR CI is the authority for those.
