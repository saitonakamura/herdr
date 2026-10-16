---
id: herdr-fork-workspace-is-effort
title: A workspace is an effort
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sync-and-branch
assignee: saitonakamura
claimed_by: macos/w6:pH/3d87f24a-ba58-420f-b5ef-157cbd75ec48
---

## Question

Per ADR 0001. A workspace is a named container with no cwd of its own and never takes its name from one. On the fork's first start, every workspace that today gets its name from its cwd keeps that name, frozen. Interactive creation asks for a name (`prompt_new_workspace_name` defaults to true); `workspace create` without `--label` names it after its starting cwd's basename, frozen. Persisted state from upstream must load. Classify as refactor-risk per AGENTS.md (characterisation tests, invariants).

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sync-and-branch
