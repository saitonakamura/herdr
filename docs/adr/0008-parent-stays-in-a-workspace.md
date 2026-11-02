# Parent stays inside one workspace; a cross-effort link is a loom annotation

An agent often starts an agent in another effort: a tracker agent opens a
cockpit with `loom spawn --root`, or a worker goes to another effort with
`--effort X`. Until now loom recorded both as herdr's Parent, across
workspaces. `agent close` follows Parent, so closing a tracker agent closed every
cockpit it had opened, in every effort. The glossary already says Parent applies
within one effort (018), but nothing enforced it.

herdr now enforces it. `agent start --parent` and `agent set-parent` refuse a
parent in another workspace. A pane moved to another workspace loses its Parent,
and so do the children it leaves behind. Snapshot load and live handoff drop any
cross-workspace Parent they find.

The link that remains is a **Source** (GLOSSARY.md). loom keeps it as the
annotation `source=<agent id>` on the started agent, and herdr never reads it.
Under ADR 0007 that makes it loom's fact: herdr would do nothing with it, since
messages already reach any agent id and closing must not cascade. loom routes
reports along it, `loom move` and `loom restore` write it where herdr dropped a
Parent, and the sidebar draws it from the annotation.

## Considered Options

- **A herdr field beside `parent_agent_id`.** herdr could then turn a Parent into
  a Source itself whenever a pane moves, including a drag in the TUI. Rejected
  because herdr would hold a second lineage fact it never acts on, and the rule
  herdr enforces would no longer be the only one it knows about.
- **Leave Parent unbounded and stop `agent close` at the workspace boundary.**
  This is the smallest change, but cross-workspace Parents would stay, and every
  other reader (the sidebar, delivery, depth limits) would need the same
  exception.
- **Change nothing in herdr; loom stops setting cross-workspace Parents.** This
  doesn't cover moves, restored snapshots or callers other than loom.

## Consequences

- A pane dragged to another workspace in the TUI loses its Parent with no Source
  in its place. Only `loom move` converts the link.
- Stock herdr has neither the rule nor annotations, so loom keeps setting
  cross-workspace Parents there. It switches only when the server advertises
  `agent_annotations`.

## Amendment — 2026-10-29: destination root as Parent

[dotfiles 103 effort-root parenting](https://github.com/saitonakamura/dotfiles/blob/master/.issues/loom/103-effort-spawn-under-root.md)
amends loom's placement policy, not herdr's workspace boundary. When `spawn
--effort X` lands in a workspace with X's live root (`kind: root`), that root is
Parent and the cross-workspace caller remains Source. Source and Parent can
coexist: the tree and sidebar nest the worker under the effort's root, and depth,
sibling watches, close guards and child rows follow Parent. Leaving the worker
as a second top-level root hid this relationship.

Reports still reach Source and the effort root, once each; Parent being that
root does not add another copy. `--handoff` cuts only Source's copy. Closing
Source never closes the worker. `--root`, an effort with no live root, and stock
herdr's cross-workspace Parent behavior are unchanged. Same-workspace moves and
restore preserve both links. No herdr runtime, protocol or sidebar code changes
are required. This amendment replaces the older Source-implies-no-Parent wording
in the original decision and fork tracker.
