---
id: herdr-fork-rearm-keeps-source
title: Re-arming a source wait without --on turns it into a plain deadline
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Found by the dotfiles loom cockpit, 2026-11-12, on the work server. Behaviour chosen by the user.

`5197264e color-tokens` had wait 72 on a Slack thread (`slack:D0C1KA07R29:1793887601.093959`, `--until replied`).
It fired on 2026-11-11 after three failed reads; the reader had stalled on 1Password (dotfiles loom 127). The agent
re-armed it with `herdr wait declare --rearm 72` and a 365-day deadline, without `--on` or `--until`.

`WaitBook::declare` (`src/app/waits.rs:402`) takes only `then` and `extensions` from the old wait. `on` and `until`
come from the new request, so wait 78 has no source and no condition, and never reads. It is a 365-day deadline that
won't fire when the thread gets its reply. herdr accepted it without a word, and the agent told the user it had
re-armed on the thread.

The help says "A fired wait to re-arm, keeping its extensions and --then" (`--rearm`), and the schema says the same
(`src/api/schema/waits.rs`). "Re-arm" reads as "the same wait again", so the omission is easy to miss.

## Decided

**A re-arm without `--on` inherits the old wait's `on` and `until`.** One given with `--on` replaces both, as now,
and an `--until` given alone applies to the inherited source. The help and the schema say what carries over.

A re-arm of a wait that never had a source (a deadline-only wait) stays deadline-only.

## Check

- A node source, a reader source and a run source all re-arm with their source, and the re-armed reader wait reads.
- `wait show` on the new wait shows the inherited `on` and `until`, and `rearm_of`.
- A cycle check runs on an inherited node source as it does on a given one.
- Restore: a re-armed wait saved and restored keeps its source.
- dotfiles loom (`loom wait`, `lib/wait.sh`) and `tools/loom` don't re-arm today. Say if either should change.

## Parent

[000-map](./000-map.md)

## Blocked by

Nothing.
