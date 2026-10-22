---
id: herdr-fork-sidebar-content
title: What the sidebar shows and how it is driven
status: in-progress
labels:
  - wayfinder:grilling
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/w1:pJ/4b860d51-4e02-4eec-abe3-7e0d71cc4486
---

## Question

Grill the user on the sidebar program's content and interaction before 019 is built: what rows it draws (agents, workspaces, the spawn tree, task status, repo facts), what clicks and vim keys do, how it relates to `loom ui` and loom's view toggles. Decided already: one sidebar pane replaces the whole expanded sidebar, its program is loom's, it redraws on events, vim keys when focused (ADR 0002).

## Parent

[000-map](./000-map.md)
