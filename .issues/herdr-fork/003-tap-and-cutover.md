---
id: herdr-fork-tap-and-cutover
title: Homebrew tap, Windows install, and the per-box cutover
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-ci-releases
assignee:
claimed_by:
---

## Question

Create the public repo `saitonakamura/homebrew-tap` (approved 2026-10-15) with a formula `herdr` that installs the fork's release asset for macOS and Linux, `conflicts_with` homebrew/core's `herdr`, and a way to bump it on each fork release (a workflow in the fork or the tap).

In the dotfiles, in one change: switch the Brewfile(s) to `saitonakamura/tap/herdr`; rewrite the install half of `docs/setup/herdr.md` (the per-box table, WSL, windows: the fork's release asset in place of `irm https://herdr.dev/install.ps1 | iex`) and every host doc that mentions the install; drop channel/update advice that no longer applies. Then write one ticket per box under `~/dotfiles/.issues/herdr-fork/` (primary mac with its personal, work and default sessions; music mac; fedora; WSL; windows) for the on-box steps: uninstall core herdr, install the tap's, restart each session's server with the user present, then `herdr --version`, `herdr config check`, `herdr server agent-manifests`, `loom doctor`. Never run brew against herdr on this box yourself.

On-box steps other tickets have handed to the cutover, to fold into each box ticket:

- 005: `herdr server reload-agent-manifests` once the server runs the fork.
- 006: the dotfiles per-box claude-hooks tickets (`~/dotfiles/.issues/herdr-fork/001-005`).
- 008: primary mac only, ticket 023 (push the agent-skills seeder, verify seeded `worktree create`).
- 011: `herdr integration install codex` on each box that runs codex.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-ci-releases
