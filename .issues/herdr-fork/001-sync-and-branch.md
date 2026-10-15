---
id: herdr-fork-sync-and-branch
title: Sync the fork with upstream and set up its branch
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w6:p2/92c959f6-658c-4db1-a608-498837fc587b
---

## Question

Make fork `master` the long-lived branch at upstream `master` (merge `upstream/master`; the fork's master has no own commits, so it should fast-forward) and push it.

- Delete from `origin` every branch except `master`: the ~100 stale copies of upstream branches and the two superseded theme branches. The user approved this on 2026-10-15. List them first and keep the list in the resolution.
- Write `FORK.md`: fork-only (never issues/PRs/discussions upstream); task worktrees at `../herdr-worktrees/<slug>`, branches `fork/<NNN>-<slug>`, PRs on `saitonakamura/herdr` into `master`; how to sync upstream (merge, never rebase `master`); `just check` before committing; the live-runtime and break-nothing rules from the map; the glossary and ADRs; `.issues/herdr-fork/` as the tracker. It overrides AGENTS.md's maintainer, local-machine and external-contributor sections.
- Add one line at the very top of `AGENTS.md` (`CLAUDE.md` is a symlink to it) saying `FORK.md` overrides those sections.
- Commit the untracked `GLOSSARY.md`, `docs/adr/` and `.issues/herdr-fork/` with it.
- Record the merge recipe in FORK.md so the next sync is mechanical.

## Parent

[000-map](./000-map.md)
