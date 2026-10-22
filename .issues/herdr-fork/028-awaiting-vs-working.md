---
id: herdr-fork-awaiting-vs-working
title: "Dogfood: tell an agent awaiting background work from one actively working, and deliver its messages"
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by:
---

## Question

On 2026-10-21 the cockpit ended its turn on the `🔄 awaiting background work` sentinel after dispatching two children. The local claude manifest maps that sentinel to `working`. Both children's `done` reports (messages 14 and 16) then stayed `pending` for about 18 hours: herdr holds messages for a working agent, and nothing ever ended the "work". This is loom's standard pattern (dispatch, then end on 🔄), so the cockpit never gets woken by its children.

Two problems, one change:

1. **A separate state.** An agent idle at its prompt and waiting on something it started (children, CI, a background shell) is not doing work, and it is not asking the user anything. The user wants the sidebar and the API to show it as its own state, distinct from `working` and from `blocked`. Decide the name with the glossary (GLOSSARY.md), for example `awaiting`. It has to fit the stable client endpoint contract in AGENTS.md: a new value reachable from a frozen codec needs a new codec or an `Unknown` fallback, and older clients and stock 0.9.4 must keep working.
2. **Delivery.** A message to an agent in that state is delivered right away, as it is to an idle one. Its arrival is exactly what the agent is waiting for. Check how this interacts with 026's `blocked_reason` work (PR #21).

Also:

- Detection: the manifest rule that matches the 🔄 sentinel produces the new state. Follow the evidence rules in AGENTS.md, using a throwaway session.
- Dotfiles: the manifest override, the sentinel section of `.config/agents/AGENTS.md`, and loom (anything that reads `working`, such as `loom close` refusing "an agent that ends on a background-work sentinel reads as working for good"). All of it must stay safe on stock 0.9.4: fork-only manifests and config in their versioned or fork files, and real-call capability probes.
- The sidebar: how the new state shows. Follow the existing UI language.

## Parent

[000-map](./000-map.md)
