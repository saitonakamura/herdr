---
id: herdr-fork-cross-effort-spawn
title: An agent starting work in another effort
status: open
labels:
  - wayfinder:grilling
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by:
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
