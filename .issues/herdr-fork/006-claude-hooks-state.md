---
id: herdr-fork-claude-hooks-state
title: Hooks decide Claude's state; blocked reason and delivery
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-state-consistency
assignee: saitonakamura
claimed_by: macos/w6:pG/3cda3beb-8635-4e03-97cf-3dbf827aee72
---

## Question

Per ADR 0003: make Claude a full lifecycle integration. `herdr integration install claude` installs hooks that report state (UserPromptSubmit → working; Stop → idle, or blocked when the last message ends on the ⏳ sentinel, or working when it ends on 🔄 or sub-agents are live; Notification → blocked). Screen rules stay only for what hooks cannot see.

- Add `blocked_reason` (`question`, `permission`, `form`) to AgentInfo and its events.
- `agent prompt` delivers to a pane blocked on `question` whose composer is empty; refuses `permission` and `form`.
- Same change in the dotfiles: shrink `.config/herdr/agent-detection/claude.toml` to what hooks don't cover; replace loom's Claude delivery checks in `lib/report.sh` with `agent prompt` (codex and pi keep theirs); update `docs/setup/herdr.md`; per-box tickets for reinstalling the integration in every Claude profile (`CLAUDE_CONFIG_DIR=~/.claude-personal` and `~/.claude-work` on the primary mac).
- Mind the integration-version rule in AGENTS.md.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-state-consistency

## Resolution

Draft, pending [PR #12](https://github.com/saitonakamura/herdr/pull/12). Design and findings: [assets/006-claude-hooks-state.md](./assets/006-claude-hooks-state.md) (on the PR branch).

- Claude integration v12: hooks report state with a blocked reason; the `Stop` hook reads the ⏳/🔄 sentinels off `last_assistant_message` and treats a running sub-agent as working.
- `herdr:claude` is a screen-assisted hook authority: hook state wins; the screen ends an interrupted turn (3 s idle) and a dialog that was answered, and blocks a working turn on a visible dialog.
- `blocked_reason` on AgentInfo, PaneInfo and `pane.agent_status_changed`. `agent prompt` answers a `question` into an empty composer and names every other refusal.
- Dotfiles branch `herdr-fork/006-claude-hooks-state`: loom takes the named refusals as final and keeps its typing fallback for plain `agent_blocked` (0.9.4, pre-006); `docs/setup/herdr.md`; per-box reinstall tickets `.issues/herdr-fork/001`–`005`. `claude.toml`/`claude.v4.toml` stay unchanged (inert under hooks).
- **Cleanup once every box runs a fork release with 006 and every profile has v12:** delete the dotfiles' `claude.toml` and `claude.v4.toml`, and drop loom's claude branch of `sentinel_only_blocked`.
