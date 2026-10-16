---
id: herdr-fork-primary-mac-seeded-worktree-create
title: "Primary mac: turn on seeded worktree create in the work lane"
status: blocked
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-worktree-post-create
  - herdr-fork-tap-and-cutover
assignee: saitonakamura
claimed_by:
---

## Question

On-box steps for [008](./008-worktree-post-create.md). Only the primary mac runs the
work lane, so this is the only box with any.

1. **Push the seeder.** The agent-skills commit `ed95fcc` ("seed-worktree seeds an
   existing tree, as herdr's post_create") is local and unpushed. It sits on a
   detached HEAD in `~/code/mayflower/.worktrees/agent-skills/herdr-post-create-seed`,
   because that repo's lefthook refuses named-branch commits. You decide whether
   and when it goes to `main` on GitLab: cherry-pick it onto `main` in the main
   checkout, which holds an uncommitted `dispatch/SKILL.md` edit of yours, then push
   and remove the worktree. Until it lands, `herdr-work-config` writes no
   `post_create`, and every refusal stays on.
2. **Merge the dotfiles branch** `herdr-fork/008-worktree-post-create` once the
   herdr PR is merged. It is safe on stock herdr, so it can merge before the
   cutover.
3. **After the fork release with 008 is installed (003):** the next `hw` regenerates
   `config.work.toml` with `post_create`, and the work server picks it up at its
   next natural restart.
4. **Verify** from a work pane:
   `herdr status server --json | jq .capabilities.worktree_post_create` is `true`,
   and `rg post_create "$HERDR_CONFIG_PATH"` finds the line. Then run one
   `herdr worktree create --branch <scratch>` in a work repo: it comes back seeded
   (`.envrc` allowed, tokens present) or reports `worktree_post_create_failed`.
   Remove the scratch tree afterwards.
5. Running pi sandboxes keep the old bridge process until restarted. Until then
   they still refuse `worktree create`, and they still have the container's
   `seed-worktree`.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-worktree-post-create
- herdr-fork-tap-and-cutover
