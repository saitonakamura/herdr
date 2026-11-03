---
id: herdr-fork-worktree-add-no-repo-code
title: Run the server's git worktree add without repo-supplied hooks or fsmonitor
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Handed over from the pi cockpit (`1321130f`) on 2026-11-03, as a hardening follow-up to dotfiles [host-pi 023](https://github.com/saitonakamura/dotfiles/blob/master/.issues/host-pi/023-herdr-bridge-worktrees.md) (`d1d83c8`).

Host pi now creates worktrees through `herdr worktree create`. Its gate allows a create by rule only after checking that nothing git reads or runs during the add can be written by the session: config files and include targets, the effective hooks dir, fsmonitor, filter programs and `config.worktree` (023's Resolution, condition 2). The add itself then runs in the herdr server, outside the seatbelt. Between the gate's checks and the server's `git worktree add` there is a short window in which a background job could swap `.git` or a ref, so the unsandboxed git would run repo-supplied code that the gate never saw.

Ask: run the server's add with hooks and fsmonitor off, so it never runs repo-supplied code. For example:

```sh
git -c core.hooksPath=/dev/null -c core.fsmonitor=false worktree add ...
```

Where it goes: `repository_git_args` in `src/worktree.rs`, as used by `build_worktree_add_new_branch_command` and `build_worktree_add_existing_branch_command`. The `trust_repository` flag already prepends `-c safe.directory=…` there. `worktree add` runs the `post-checkout` hook, which is the main repo-supplied code on this path.

Things to settle:

- **Filters.** `filter.<driver>.smudge`/`process` also run during the checkout, and `-c` can't disable a driver without knowing its name. The options are to leave filters to the gate, to add `--no-checkout` and then run a checkout with the filters off, or to read `git config --get-regexp '^filter\.'` and blank each one. Decide which of these is in scope.
- **Cost to legitimate hooks.** Turning hooks off also drops the user's own `post-checkout` hooks for TUI creates, git-lfs's for one. Should this always apply, be a `[worktrees]` setting, or apply only to API creates? Note that 023's `post_create` guard already runs on every create.
- **`/dev/null` on Windows.** `core.hooksPath` needs a path that doesn't exist or holds no hooks on every platform. Pick one that works in the Windows build, or gate the choice through `src/platform/`.
- **Upstream code.** `src/worktree.rs` is upstream (`0148c137`). Keep the fork change small so merges stay cheap (FORK.md).

Done: an add from the API runs no `post-checkout` hook and no fsmonitor, even when the repo's config sets them. A test shows a hook that writes a marker file leaves no marker. The 023 gate's condition list can then say which checks are defence in depth and which are still load-bearing.

## Parent

[000-map](./000-map.md)
