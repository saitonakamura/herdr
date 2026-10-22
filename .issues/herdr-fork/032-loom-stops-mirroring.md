---
id: herdr-fork-loom-stops-mirroring
title: loom stops mirroring herdr's runtime facts
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:pY/c367ceff-719f-4fae-b12b-dedd58587cf2
---

## Question

Per [ADR 0007](../../docs/adr/0007-one-owner-per-fact.md), each fact has one owner. Today loom's registry still keeps its own copies of several facts about live agents: Parent, name, `.live` state, a combined state computed in `tree.jq`, and its message inbox. 024 was those copies drifting from herdr's.

On a fork herdr with 035, loom changes as follows:

- **Live nodes come from herdr.** A live node is `agent list` plus annotations. Parent, name, state and task status are read from herdr and never stored.
- **The registry keeps only what herdr cannot know.** That is policy and history: efforts, routing, limits, closed nodes, outcomes and escalations. Decide how a node becomes history when its pane closes. The deliberate 010 divergence stays: herdr makes a child top-level, while loom's history keeps who started it.
- **The inbox reads herdr's queue as the record** (`message list`). That way messages the sidebar sends straight to herdr (018: `r`/`t`) appear in `loom ui` and `loom inbox`.
- **Readers.** `loom tree`, `loom ui`, `loom view`, `whoami`, `close` and `doctor` read from the new sources.
- **Stock herdr** keeps the current registry path, behind a real-call capability probe, until every box runs the fork.
- **Migration.** Existing lanes need no migration step beyond what `restore` does.

Keep `tests/run.sh` green, and add stub-herdr cases for the fork path.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-agent-annotations

## Resolution

Draft, awaiting merge: dotfiles branch `herdr-fork/032-loom-stops-mirroring` (`9ceaabb`, master merged in as
`f10dbb0`). No herdr change was needed: `agent list`, `pane list` and `message list --to/--from` already carry
every fact loom reads.

- **Live nodes come from herdr.** On a herdr that answers the 035 annotations probe, `all_nodes` joins the registry
  with `agent list` (`lib/live.jq`). Name, Parent, state, task status, pane, tab, workspace and session are herdr's;
  effort, role, kind and why are the annotations'. Task status maps back to loom's words, and `needs_user` is
  `asking` or `handoff`, whichever loom last reported. An annotated agent with no node file is a node too, and gets
  a file on its first write.
- **The registry keeps policy and history.** `update_node` shows a filter herdr's facts but writes only what the
  filter changed, never `live`; a change to an annotated field goes to herdr. `parent_sync` and the status hook's
  `live` copy stop on these builds, and `attach` names the agent only when herdr has no name for it.
- **How a node becomes history.** When herdr no longer has its agent. `pane.closed` names only a pane herdr has
  already forgotten, so it sweeps open nodes against `pane list`; `restore` matches panes by agent id first. The
  closed node keeps the name, parent and status loom last recorded, so a rename made outside loom is not in its
  history. A child herdr made top-level when its parent closed still shows under that parent (the 010 divergence).
- **Inbox.** `loom inbox` and `loom ui`'s thread read `message list --to`, so the sidebar's `r`/`t` show, plus
  reports herdr never had and progress. Inbox files no longer copy herdr's delivery time.
- **Readers.** `tree`, `ui`, `view`, `whoami`, `close` and `doctor` read through `all_nodes`. `doctor` warns on a
  live node herdr holds without loom's annotations.
- **Stock herdr** keeps the registry path. **Migration:** none; `loom restore` fills in missing annotations.
- **Verification.** `tests/run.sh`: 419 passed, with ten stub-herdr cases in `tests/test-live.sh`; a copy with the
  join disabled fails seven of them. Live smoke on a throwaway session (fork `0.9.5+fork.1` at 378bba24, two idle
  claude agents): herdr's rename, Parent and `needs_user` showed in `loom tree`/`whoami`, a `--user` reply showed in
  `loom inbox`, and closing the pane closed the node through the sweep. The smoke found that `attach` no longer named
  the agent; fixed before commit.
