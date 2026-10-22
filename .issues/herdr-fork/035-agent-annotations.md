---
id: herdr-fork-agent-annotations
title: Agent annotations
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by:
---

## Question

Per [ADR 0007](../../docs/adr/0007-one-owner-per-fact.md). Each agent carries **annotations** (GLOSSARY.md): an opaque string-to-string map that a program keeps on the agent. herdr stores it and never interprets it.

- **Storage.**
  - Annotations live beside `agent_id`, `parent_agent_id` and `task_status` on the agent's terminal.
  - They persist across server restart and live handoff, and survive `/clear` and pane moves, as 010's identity does.
  - Closing the pane drops them.
  - Choose limits on key and value length and on count, and document them. The key syntax should match tokens (`[A-Za-z0-9_-]`).
- **API and CLI.**
  - `agent annotate <target> k=v... [--clear k]...`, where an empty value also clears.
  - The current map appears as an optional `annotations` field on `pane get/list/current` and `agent get/list`.
  - A change publishes `pane.updated`.
  - Advertise capability `agent_annotations`.
  - On the stable endpoint this is a new optional JSON field, with no codec change.
- **Sidebar rows.** Decide whether native sidebar rows may reference annotations, for example `@role`. That would help a box without `herdr-sidebar` (Windows), but it is optional.
- **Same change in loom.**
  - Probe the capability with a real call.
  - Write `effort`, `role`, `kind` and `why` at `spawn`, `attach` and `move`, and re-write them in `restore` for agents that predate this change.
  - `scripts/herdr-ticket` writes `ticket` as an annotation, keeping the token while the native rows still read `$ticket`.
  - Stock herdr keeps today's tokens.
  - Keep `tests/run.sh` green.
- **Not in scope.** loom reading its own facts back from annotations instead of its registry is [032](./032-loom-stops-mirroring.md).

## Parent

[000-map](./000-map.md)

## Resolution

Merged 2026-10-22 as `56e56478`: [PR #26](https://github.com/saitonakamura/herdr/pull/26), with dotfiles `9519ed3`.

- **Storage.** A `BTreeMap<String, String>` on `TerminalState` next to `task_status`, persisted in `PaneSnapshot`. That covers restart and live handoff, and the map follows the terminal through `/clear` and pane moves. Closing the pane drops it.
- **Limits.** Keys `^[A-Za-z0-9_-]{1,32}$`. Values are at most 512 characters with no control characters, and an agent keeps at most 32. A patch past any limit is refused whole (`invalid_annotation`, `annotation_limit`). Values are stored verbatim, with no trimming, unlike tokens.
- **API/CLI.** `agent.annotate` / `herdr agent annotate <target> [KEY=VALUE]... [--clear KEY]...` is a patch: an empty or null value clears its key, and unnamed keys stay. A pane without an agent gets an agent id. The map appears as optional `annotations` on pane and agent info, a change publishes `pane.updated`, and the capability is `agent_annotations`. The stable endpoint gets only an optional JSON field and a new method.
- **Sidebar rows: no `@key` references for now.** Native rows render tokens on the client from the private client protocol, so annotations there would widen that protocol for a fallback only Windows needs. loom and `herdr-ticket` keep writing the tokens alongside, so nothing is lost. Revisit if Windows wants loom facts natively.
- **loom.**
  - It probes with a real `agent annotate loom-capability-probe --clear loom` (`agent_not_found` means yes).
  - It writes `effort`, `role`, `kind` and `why` at spawn and through `restore_node` (attach, move, restore; restore also covers nodes from before). Current values are skipped.
  - `why` is flattened and cut to 512 characters.
  - `scripts/herdr-ticket` adds `ticket`.
  - Stock herdr is untouched. `docs/setup/herdr.md` documents it.
- **On-box.** Nothing beyond pulling dotfiles once both merge. loom then fills in annotations at the next `loom restore` (the startup hook) on a fork build.
