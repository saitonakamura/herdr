---
id: herdr-fork-cross-effort-spawn
title: An agent starting work in another effort
status: closed
labels:
  - wayfinder:grilling
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/1e4f1472-5622-4ee9-a091-2801a149bf2b
---

## Question

Grill the user on what happens when an agent in one effort spawns an agent in another effort, or opens a new effort. Settled in 018:

- **It is not Parent.** Parent now applies within one effort (GLOSSARY.md).
- **It is still a link to keep.** The user calls it "a spawn source", recorded per effort.
- **Communication differs** from the tree inside an effort.
- **The sidebar currently draws the new agent as a root in its own effort's section, with no marker.**

To decide:

- **Name it in the glossary.** It has to fit beside Parent and Effort.
- **What the link joins:** agent to agent, agent to effort, or effort to effort.
- **Who owns it.** By [ADR 0007](../../docs/adr/0007-one-owner-per-fact.md), it is either a herdr fact or a loom annotation or policy.
- **How the two sides talk:**
  - Do reports flow back?
  - Does the source get told when the work finishes?
  - Can it close the other side?
- **What the sidebar shows**, if anything, on either side.
- **How `loom spawn` expresses it**, and whether opening a new effort (`--root`) from an agent is the same thing.

## Parent

[000-map](./000-map.md)

## Resolution

Grilled with the user on 2026-10-22 over five rounds. Vocabulary: GLOSSARY.md adds **Source**, and Parent says that an agent started from another effort has a Source instead. The decision with trade-offs is [ADR 0008](../../docs/adr/0008-parent-stays-in-a-workspace.md). The build is [039](./039-source-links.md), blocked by 035.

**Before this.** loom recorded every cross-effort spawn as herdr's Parent across workspaces. `agent close` follows Parent (`src/app/agent_tasks.rs`, `subtree_members`), so closing `dotfiles` would also have closed the `local-llm` cockpit's tree. loom's history shows two shapes:

- `--root` cockpits opened from tracker agents: `plan-week` opened 5, `work-tracker` 6, and `dotfiles` opened `herdr-fork` and `local-llm`.
- `--effort X` workers sent into another effort: `prepare-discovery-q4-be` started 3 TypeScript-migration analysis workers.

**One link for both shapes.** Work started in another effort could just as well have opened a new effort, so `--root` and `--effort X` make the same link. The only difference is placement.

**What it joins.** Agent to agent. It lives on the started agent and names the agent that started it. The effort-level record falls out of it, and loom's effort log already has it.

**Who owns what.**

- herdr owns Parent and bounds it to one workspace:
  - It refuses a cross-workspace parent at `agent start --parent` and `agent set-parent`.
  - A moved pane loses its Parent, and so do the children it leaves behind.
  - Snapshot load and live handoff drop cross-workspace Parents.
- loom owns Source, as the annotation `source=<agent id>`. herdr does nothing with it.
- Rejected: a herdr `source` field, an unbounded Parent with a close that stops at the workspace boundary, and a fix in loom alone (ADR 0008).

**Parent or Source.** loom decides by the workspace the new agent lands in, not by effort ids. An unfiled caller still has a workspace.

- `--effort X` with no workspace for X opens one, and the agent is top-level in it.
- `loom move` turns the Parent herdr drops into a Source, both ways.
- `loom restore` writes `source` from its registry.
- Stock herdr, without `agent_annotations`, keeps today's cross-workspace Parent.

**How the two sides talk.**

- **Reports go to both.** A started agent reports to its Source and to its effort's live root (`kind: root`), unless that root is itself.
- **Labels.** Each copy says what the recipient is to the sender, in the summary text loom writes: `(started by you)` or `(from <effort> via <source>)`. herdr's message is unchanged.
- **`--handoff`.** `spawn --handoff` keeps the Source for provenance and cuts only its copy, for spawners that don't care about the reports.
- **Source gone.** With the Source's pane gone, only the effort copy goes out. Nothing is rerouted up the Source's own tree.
- **Replies.** Either side may reply and tell. The first reply withdraws the other copy if it is still pending, and a later reply goes through with a warning.
- **Close.** Closing the Source never closes the started agent. Either side may still close it by name.

**What the views show.**

- **Sidebar.** On the started agent, a dim, clickable `← <source>` token, drawn only while the Source is live. Nothing on the Source's side. Added to [030](./030-sidebar-program.md).
- **`loom tree` and `loom ui`.** They draw the started agent as a root of its own effort with `← <source>`, and depth counts from 0 per effort.
