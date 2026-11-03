---
id: herdr-fork-restore-dead-blocks-as-interrupted
title: A restart restores a block whose prompt died with the old process as interrupted
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/w1:p23/52dbb064-5732-49f0-93df-694474c707a8
---

## Question

Seen by the user on 2026-11-03 after restarting `hp` on `43892c78`: `pimig-002` (pi, `wN:p3`, agent `7753f26e`) stayed red with nothing on its screen asking anything.

pi has full lifecycle hook authority (`agent explain`: `screen_detection_skip_reason = full_lifecycle_hook_authority`). Before the restart it reported `blocked` with no reason, most likely while its tool gate waited on an approval for an `edit` call. [053](./053-restore-settled-status.md) restores `blocked(r)` as `blocked(r)`. The restart killed the old pi process and its pending approval; the resumed pi sits at its prompt and reports nothing, and a hook-authority agent's state is never re-read from the screen. So the restored `blocked` points at a prompt that no longer exists, and nothing clears it until the user types there.

## Decided with the user (2026-11-03)

- **A block reported through lifecycle hooks restores as `interrupted`**, like a cut-off turn: the prompt it waited on lived in the old process. The user decides whether to continue; herdr still never types anything.
- **Check the screen-detected reasons too, and apply the same rule where it holds.** A Claude `permission` or `form` dialog is also process state; if a resumed Claude doesn't redraw it, restoring `blocked(permission|form)` has the same flaw. `question` (a prose question ending on the `⏳` sentinel) survives, since the transcript still shows it, so it stays `blocked(question)`. Verify with a throwaway session per reason rather than assuming, and record which reasons map to `interrupted` and why.
- Everything else in 053 stands: what clears a restored status, messages held for an interrupted pane, `agent prompt` still reaching it.

## Parent

[herdr-fork map](./000-map.md)

## Resolution

Draft, pending review of [PR #59](https://github.com/saitonakamura/herdr/pull/59).

- A saved block restores as `blocked`/`interrupted` unless it is a `question` from an agent whose screen herdr still reads. Full-lifecycle hook agents (pi, omp, opencode, kilo, kimi, mastracode) restore every block as `interrupted`; hook authority is read from the saved `agent_session`, so no snapshot field was added and 053-era snapshots are covered.
- Live per reason (throwaway `repro-063-162309`, Claude Code 2.1.317, haiku, manual permissions, `herdr session stop` then resume):
  - `permission`: a resumed claude does not redraw the dialog; the transcript reads "User rejected write to b.txt". Restores as `interrupted`.
  - `form` (AskUserQuestion): not redrawn; "User declined to answer questions". Restores as `interrupted`. In manual mode it was reported as `permission`, the notification overwriting PreToolUse's `form`.
  - `question`: the transcript still ends on the sentinel and the screen matches `awaiting_user_answer`. Stays `question`. The 053 build actually lost it to `idle`: the screen match cleared the overlay and claude's resume idle hook won. A screen match naming the restored reason now confirms the overlay instead.
  - no reason: only a screen-detected dialog or a reasonless hook agent produces it, so it restores as `interrupted`.
- pi was covered by a unit test, not driven live.
