---
id: herdr-fork-worktree-post-create
title: Worktree post-create hook
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sync-and-branch
assignee:
claimed_by: macos/w6:pE/16c210d5-75bb-40ff-b9a8-ba8fec0d59c2
---

## Question

Add `[worktrees] post_create = [argv]`, run in the new checkout before it opens, for the TUI, the CLI/API and anything going through them; a failure is reported and the checkout kept.

Same change in the dotfiles: `scripts/herdr-work-config` sets `post_create` to the dispatch skill's `seed-worktree`; retire `.config/direnv/work-shims/herdr`'s refusal and loom's work-lane `--worktree` refusal (`lib/spawn.sh`); update `docs/setup/herdr.md` § worktree create, `AGENTS.work.md`, `pi-work/herdr-allowlist` and the dispatch skill's docs as needed.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sync-and-branch

## Resolution

Draft, pending merge of [PR #11](https://github.com/saitonakamura/herdr/pull/11).

- herdr: `[worktrees] post_create = [argv]` runs in each checkout `worktree.create`
  makes, before its workspace opens. The TUI, the CLI and the socket API share that
  one path. The hook gets `HERDR_WORKTREE_PATH`, `HERDR_WORKTREE_SOURCE` and
  `HERDR_WORKTREE_BRANCH`. If it fails, the request returns
  `worktree_post_create_failed` with the hook output; the checkout is kept and not
  opened. The new server capability `worktree_post_create` shows in
  `herdr status server --json`.
- dotfiles branch `herdr-fork/008-worktree-post-create` (`c8ef04f`), safe on stock
  herdr:
  - `herdr-work-config` writes `post_create` only when the binary accepts it and
    the seeder has `--herdr-post-create`.
  - The shim, loom `spawn --worktree` and the pi bridge (new
    `#?requires <capability>: <pattern>` allowlist line) let `worktree create`
    through only when the running server has the capability and the config sets
    the hook.
  - Docs updated.
- agent-skills: `seed-worktree --existing <path>` / `--herdr-post-create`
  (local `ed95fcc`, unpushed).
- On-box steps: [023](./023-primary-mac-seeded-worktree-create.md).
