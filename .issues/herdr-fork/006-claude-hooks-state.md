---
id: herdr-fork-claude-hooks-state
title: Hooks decide Claude's state; blocked reason and delivery
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-state-consistency
assignee:
claimed_by:
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
