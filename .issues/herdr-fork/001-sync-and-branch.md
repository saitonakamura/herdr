---
id: herdr-fork-sync-and-branch
title: Sync the fork with upstream and set up its branch
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/92c959f6-658c-4db1-a608-498837fc587b
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

## Resolution

- Fork `master` fast-forwarded to `upstream/master` (`916fcf8d`), plus the setup commit `1fbb4226` (FORK.md, AGENTS.md top line, GLOSSARY.md, docs/adr/ un-ignored and committed, this tracker). Pushed.
- FORK.md also overrides AGENTS.md's Release Channels section: releases are 002's to define.
- Deleted every branch on `origin` except `master` (102, by the user from this pane after the auto-mode classifier refused the push):

  - `1862-perf`
  - `akbash/1004-worktree-remove-focus-parent`
  - `akbash/1034-remote-ssh-error`
  - `akbash/1066-option-shift-punctuation`
  - `akbash/1067-windows-clipboard`
  - `akbash/1088-alt-shift-letter`
  - `akbash/1098-wrapped-pane-links`
  - `akbash/1168-collapsed-sidebar-agent-order`
  - `akbash/1337-forward-focus-events`
  - `akbash/1349-horizontal-trackpad-scroll`
  - `akbash/1360-reap-shell-children`
  - `akbash/1404-wezterm-emoji-input`
  - `akbash/1405-copy-mode-line-end`
  - `akbash/1435-cli-protocol-guard`
  - `akbash/1439-wait-pane-close`
  - `akbash/1471-preserve-selection`
  - `akbash/1563-codex-static-title-fallback`
  - `akbash/1573-unknown-config-keys`
  - `akbash/1594-preserve-plugin-argv`
  - `akbash/1603-cache-reported-cwd`
  - `akbash/1622-bound-audio-players`
  - `akbash/1665-reject-large-paste`
  - `akbash/1752-forward-host-palette`
  - `akbash/2015-vscode-remote-clipboard`
  - `akbash/2020-vibe-shift-input`
  - `akbash/2066-preserve-claude-settings`
  - `akbash/2171-hide-worktree-json`
  - `akbash/2216-collapsed-workspace-status`
  - `akbash/2242-copy-mode-direct-bindings`
  - `akbash/2255-sidebar-scroll-multi-client`
  - `akbash/2292-ignore-legacy-agent-panel-scope`
  - `akbash/2297-pane-layout-current`
  - `akbash/2302-preserve-kitty-flag-3-releases`
  - `akbash/2309-default-mouse-encoding`
  - `akbash/2314-kitty-clipboard-replace`
  - `akbash/2317-single-tab-navigator-search`
  - `akbash/2325-worktree-source-checkout`
  - `akbash/2393-restore-keyboard-reporting`
  - `akbash/2416-refresh-appearance-on-focus`
  - `akbash/2424-hungup-stderr`
  - `akbash/2449-no-focus-pane-input`
  - `akbash/2450-opencode-session-switch`
  - `akbash/2453-forward-pane-bel`
  - `akbash/2478-restore-remote-stream-blocking`
  - `akbash/2550-fast-scrollbar-state-read`
  - `akbash/2556-shift-tab-settings`
  - `akbash/2657-bare-repo-name`
  - `akbash/2670-large-terminal-redraw`
  - `akbash/2674-shifted-backslash`
  - `akbash/2681-grok-session-new`
  - `akbash/2697-order-default-accent`
  - `akbash/2707-claude-spinner-title`
  - `akbash/2708-mosh-selection-highlight`
  - `akbash/2711-reload-current-manifest`
  - `akbash/2733-edit-scrollback-logical-lines`
  - `akbash/2760-claude-half-circle-spinner`
  - `akbash/2769-silent-python-hook-failure`
  - `akbash/2773-agent-start-shell-readiness`
  - `akbash/2788-reject-blocked-prompts`
  - `akbash/828-remote-drag-image-path`
  - `codex/ci-flake-diagnostics`
  - `feat/743-pane-scrollbars`
  - `feat/shared-pane-frames-kiss`
  - `feat/tab-bar-status`
  - `feature/windows-integrations`
  - `feature/windows-remote-attach`
  - `fix/1864-mouse-motion-render`
  - `fix/active-row-surface`
  - `fix/alt-screen-scrollbar`
  - `fix/copy-mode-arrow-repeat`
  - `fix/fast-alt-screen-read`
  - `fix/terminal-theme-surfaces`
  - `issue/1330-windows-notifications`
  - `issue/1533-conpty-repro`
  - `issue/1561-shift-tab`
  - `issue/1694-sidebar-tree-layout`
  - `issue/1838-git-sidebar-polling`
  - `issue/1982-process-detection-mode`
  - `issue/2008-windows-openssh-detach`
  - `issue/2064-headless-agent-resume`
  - `issue/2072-windows-agent-start`
  - `issue/2107-windows-git-bash-detection`
  - `issue/2206-disconnected-mount-freeze`
  - `issue/2260-colorblind-status-indicators`
  - `issue/2435-key-encoding-architecture`
  - `issue/2452-unknown-theme-config`
  - `issue/2514-fish-alt-bindings`
  - `issue/2594-root-repo-label`
  - `issue/2612-shutdown-priority`
  - `issue/2642-windows-idle-detection`
  - `issue/288-herdr-wait-agent-status-never-returns-when-the-p`
  - `issue/306-spurious-characters-written-to-command-line-of-n`
  - `issue/316-mobile-width-threshold-should-be-configurable`
  - `issue/714-color-scheme-propagation`
  - `movable-tab-bar`
  - `plugin-v1`
  - `qwen-code`
  - `refactor/input-key-encoding`
  - `right-click-pane`
  - `shared-pane-terminal-browser`
  - `update-pty-ownership`
  - `windows`
