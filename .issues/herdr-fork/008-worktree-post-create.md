---
id: herdr-fork-worktree-post-create
title: Worktree post-create hook
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sync-and-branch
assignee:
claimed_by:
---

## Question

Add `[worktrees] post_create = [argv]`, run in the new checkout before it opens, for the TUI, the CLI/API and anything going through them; a failure is reported and the checkout kept.

Same change in the dotfiles: `scripts/herdr-work-config` sets `post_create` to the dispatch skill's `seed-worktree`; retire `.config/direnv/work-shims/herdr`'s refusal and loom's work-lane `--worktree` refusal (`lib/spawn.sh`); update `docs/setup/herdr.md` § worktree create, `AGENTS.work.md`, `pi-work/herdr-allowlist` and the dispatch skill's docs as needed.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sync-and-branch
