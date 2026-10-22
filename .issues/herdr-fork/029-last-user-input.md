---
id: herdr-fork-last-user-input
title: "Say when a client last typed into a pane, so loom can tell a pane the user is using"
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:pN/f4915a47-096c-4908-8d98-76080f5d667e
---

## Question

loom 068 (dotfiles `.issues/loom/068-done-with-user-steps-left.md`): a parent agent closed, or offered to close, a child's pane the user was still using. loom's rule is that a pane the user has typed into since the child's last report is *user-held*, and `loom close` refuses it without `--force`. Only herdr knows when a person typed into a pane. The screen can't tell: Claude draws a suggested reply as grey ghost text in its input box, and that looks the same as typed text.

Neither herdr exposes it today (checked 2026-10-22). Stock 0.9.4 has no input field on `pane get`/`agent get` and no input event, and neither does the fork. The fork's only tracking is upstream's `interactive_input_deadlines` (`server/headless/render.rs`), a render-latency window that is never exposed.

Add it:

- **Field.** `last_user_input_at_ms` (unix epoch milliseconds, optional, omitted when unknown) on pane info (`pane get`, `pane list`) and agent info (`agent get`, `agent list`). It is the last time a *client* forwarded keyboard input to that pane's terminal: a key press or repeat (not a release), text, or a paste. Clipboard-image paste and terminal-attach input count. Mouse, scroll, focus and popup input don't.
- **Not API writes.** `pane send-text`, `pane send-keys`, `pane send-input`, `agent prompt`, message delivery, `agent start`'s prompt and resume input never set it. That separation is the point: loom types into panes through those.
- **Persisted** next to `task_status` in the session snapshot, so a restart or live handoff keeps it. An old snapshot without it restores as unknown.
- **No event.** A per-keystroke `pane.updated` would flood subscribers. Readers poll `pane get`.
- **Capability.** `status server --json` advertises `pane_user_input: true`. loom gates on that real call, never on `--help`. Stock builds report it absent, which reads as false.
- **Endpoint contract** (`CLAUDE.md`): optional fields only. No frozen codec or gen-1 fixture changes. If the field would reach a bincode codec, keep it to JSON or add it the way 013's `task_status` was added.

Template: 013's `2e1979eb` (`task_status`) touched the same places, from schema, info builders and snapshot through capability and docs.

Verify with a throwaway session: type into a pane from an attached client and `pane get` shows a fresh value. `pane send-text` and `agent prompt` into another pane leave it unset.

## Resolution

Draft. Branch `fork/029-last-user-input`, [PR #25](https://github.com/saitonakamura/herdr/pull/25), not merged.

- **As shipped, contract unchanged.** Field `last_user_input_at_ms` (unix epoch ms, omitted when unknown) on pane info and agent info. Capability `pane_user_input` under `capabilities` in `herdr status server --json`. No event.
- **What counts.** In `src/server/headless.rs`: `ClientShellPaneInput` key press/repeat or text (the existing `keyboard_forwarded`) or a non-empty paste. Clipboard image paste to a pane or a direct terminal attach. Terminal-attach `ClientInput` that parses (`parse_framed_input`) as a key, text or paste, so the host terminal's focus and mouse reports don't count. Popup input and API writes never set it.
- **Persistence.** Stored in `PaneSnapshot` beside `task_status`, restored by both restore paths, so a handoff keeps it. A keystroke schedules a save only when none is pending.
- **Verified live** on a throwaway `t029` session: client typing set p1; `pane send-text`, `pane send-keys` and `agent prompt` left p2 unset; the values survived a server restart.

## Parent

[000-map](./000-map.md)
