---
id: herdr-fork-spaces-repo-token-check
title: Fork's spaces repo-token diagnostic fails config check
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Found by the dotfiles terminal-clickability effort: dotfiles `scripts/herdr-work-config` fails `config check` when the fork's `herdr` is first on `PATH`.

The cause is the fork's own commit `78f421da` (compute repo facts per pane, not per workspace). It added `SpacesSidebarConfig::repo_token_diagnostic` (`src/config/sidebar.rs`), which flags `branch` / `git_status` on `ui.sidebar.spaces` rows because a workspace row has no repo facts any more. `Config::diagnostics` (`src/config.rs`) lists it alongside real errors, and `config check` fails on any diagnostic. The dotfiles `config.toml` puts `["branch", "git_status", $effort]` on spaces rows (line 413). So on a fork build that row renders empty and the check fails, while stock herdr accepts the same file and fills the row.

Decide:
- **Config.** The shared `config.toml` has to stay valid on stock herdr. So the fork-specific spaces rows (repo facts moved to `ui.sidebar.agents`) probably belong in `config.fork.toml` (`c6caa3a6`), which is a dotfiles change. Check `scripts/herdr-work-config` composes it too.
- **Severity.** Should a "renders nothing" note fail `config check`? If herdr has a warning tier that doesn't fail the check, use it. If it doesn't, decide whether to add one or to keep failing, since failing is what surfaced this.
- Check the other fork-added diagnostics (`pane_diagnostics`) for the same shape.

## Parent

[000-map](./000-map.md)

## Resolution
