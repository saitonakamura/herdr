---
id: herdr-fork-spaces-repo-token-check
title: Fork's spaces repo-token diagnostic fails config check
status: closed
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

Merged 2026-11-02: [PR #45](https://github.com/saitonakamura/herdr/pull/45) as `9c754d9c` (head `799a32e5`). Built by guarded host pi (`config-warn-041`); the cockpit signed, pushed and opened the PR, since host pi can't sign as the human in a fork.

- **Severity.** The user decided: a note that a configured token renders nothing is a warning, not an error. A new `Config::collect_warnings()` sits beside `collect_diagnostics()`. `config check` prints `warning: …` lines after its usual header, and its exit code still counts only real errors. The spaces repo-token note is the only warning so far.
- **Pane diagnostics stay errors.** `ui.sidebar.pane` with no program and a zero `pane_collapsed_width` reject invalid values; they are not "renders nothing" notes.
- **Config.** No dotfiles change needed. The dotfiles `config.fork.toml` already cuts spaces rows to `$effort` and puts repo facts on agent rows, and `scripts/herdr-work-config` already copies it beside the generated work config.
- **Verified** by the cockpit, because the host-pi gate denies `herdr config`. A config with `rows = [["workspace"], ["branch", "git_status"]]`: stock 0.9.4 `config: ok` exit 0; the fork before this `issues found` exit 1; this branch `config: ok` plus the warning, exit 0. All four live configs pass on all three binaries. CI green on every job.
- **Host-pi findings** from this run are in dotfiles `.issues/host-pi/assets/herdr-fork-041-gate-findings.md`, filed as host-pi 028 and a note on 019.
