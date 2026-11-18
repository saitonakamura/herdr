---
id: herdr-fork-tap-and-cutover
title: Homebrew tap, Windows install, and the per-box cutover
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-ci-releases
assignee:
claimed_by: fedora/3e88b947-4f7f-44d7-b67f-b0f613eb59d1
---

## Question

Create the public repo `saitonakamura/homebrew-tap` (approved 2026-10-15) with a formula `herdr` that installs the fork's release asset for macOS and Linux, `conflicts_with` homebrew/core's `herdr`, and a way to bump it on each fork release (a workflow in the fork or the tap).

The formula also installs `herdr-sidebar` (030) from the `herdr-sidebar-<os>-<arch>` assets of the same release, so one `brew upgrade` moves both. Windows has no sidebar asset.

In the dotfiles, in one change: switch the Brewfile(s) to `saitonakamura/tap/herdr`; rewrite the install half of `docs/setup/herdr.md` (the per-box table, WSL, windows: the fork's release asset in place of `irm https://herdr.dev/install.ps1 | iex`) and every host doc that mentions the install; drop channel/update advice that no longer applies. Then write one ticket per box under `~/dotfiles/.issues/herdr-fork/` (primary mac with its personal, work and default sessions; music mac; fedora; WSL; windows) for the on-box steps: uninstall core herdr, install the tap's, restart each session's server with the user present, then `herdr --version`, `herdr config check`, `herdr server agent-manifests`, `loom doctor`. Never run brew against herdr on this box yourself.

On-box steps other tickets have handed to the cutover, to fold into each box ticket:

- 005: `herdr server reload-agent-manifests` once the server runs the fork.
- 006: the dotfiles per-box claude-hooks tickets (`~/dotfiles/.issues/herdr-fork/001-005`).
- 008: primary mac only, ticket 023 (push the agent-skills seeder, verify seeded `worktree create`).
- 011: `herdr integration install codex` on each box that runs codex.
- **Before the box's restart, pull dotfiles to at least `883f0a2` re-claim the herdr session.** A running bridge keeps its old code until its pi relaunches, so the cutover's restart is what deploys the [066](./066-pi-lifecycle-authority-lost.md) fix, and only if the script is already there. A pi pane restored without it can read `idle` for good while it works.

## Resolution

Done 2026-10-27.

- **Release.** `v0.9.5+fork.2` (`f8ba9e5c`), the first with 006, 014 and the
  `herdr-sidebar-<os>-<arch>` assets. The macOS x86_64 job sat unassigned for want
  of a runner and was cancelled; a rerun of the failed jobs published all twelve assets.
- **Tap.** [`saitonakamura/homebrew-tap`](https://github.com/saitonakamura/homebrew-tap),
  public. `Formula/herdr.rb` is rendered by `scripts/render-formula <tag> <SHA256SUMS>`
  from the release's own checksums, and installs `herdr`, `herdr-sidebar` (a resource
  of the same release, so one `brew upgrade` moves both), `LICENSE` and the notices
  (`share/herdr/notices`). In a `homebrew/brew` container it passes
  `brew audit --strict --online`, installs, and passes `brew test`.
- **Bump.** The tap's `bump.yml` polls this repo hourly for the latest `v*+fork.*`
  release and commits a re-render when it moved; `workflow_dispatch` takes a tag.
  Polling rather than a dispatch from here, so no cross-repo token exists.
- **No `conflicts_with`.** Both formulae are named `herdr`, and with core's installed
  `brew install saitonakamura/tap/herdr` already refuses ("you must first uninstall
  the existing formula"), so the line would only name itself.
- **Finding: `brew bundle` does not cut a box over.** With core's herdr installed,
  `brew bundle` on the switched Brewfile reports `Using saitonakamura/tap/herdr` and
  `bundle check` passes, while core's stays. Each box ticket therefore runs
  `brew uninstall herdr && brew install saitonakamura/tap/herdr` and checks for
  `+fork` in `herdr --version`. After that, `brew upgrade herdr` follows the tap.
- **Dotfiles.** `5f48135`: the Brewfile takes the tap's herdr; `docs/setup/herdr.md`
  has an install section (per-box table, the bundle trap, windows on the release zip
  in place of herdr.dev's script, no channel advice). The per-box tickets are dotfiles
  `herdr-fork/013`–`017`, folding in 005 (`reload-agent-manifests`), 006 (dotfiles
  002–005 now block on them), 008 (023, primary mac) and 011 (codex integration on the
  boxes that run codex: primary mac, music, fedora) and, from `a827c32`, the 066 dotfiles
  pull before the restart. Dotfiles 006–010, loom migrate,
  block on them too.
- Unblocked 023.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-ci-releases
