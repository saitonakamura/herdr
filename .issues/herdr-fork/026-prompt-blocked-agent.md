---
id: herdr-fork-prompt-blocked-agent
title: "Dogfood: agent prompt refuses an agent that is blocked only on a question it asked"
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:pE/3c265891-62d0-41eb-9b12-f97dc2d9ff1b
---

## Question

On 2026-10-21, after the fork session restarted, `herdr agent prompt <pane> "<text>"` to two paused children failed with `{"code":"agent_blocked","message":"agent w1:pB is blocked and requires interactive input"}`. Both were at a bare Claude prompt box. They were `blocked` only because their last line was the `⏳ awaiting your answer` sentinel, which the local claude manifest maps to `blocked`. The workaround was `herdr pane send-text <pane> "<text>"` plus `herdr pane send-keys <pane> Enter`.

The refusal makes sense for a real interactive control, such as a permission dialog or a trust prompt, where typed text would answer the wrong thing. It does not make sense when the agent asked in prose and its input box is empty and ready.

Decide and fix:

- Can herdr tell those two kinds of `blocked` apart, for example through `interactive_ready`, a manifest flag on the rule, or the rule id? Or should `agent prompt` take an explicit opt-in, such as `--allow-blocked`? Prefer a distinction the detection evidence supports over a flag the caller has to guess. Any manifest change follows the evidence rules in AGENTS.md.
- Check what loom does today when it tells or answers a blocked child: `loom tell`, report delivery and `herdr message` delivery (012). Fix loom in the same change if it hits the same refusal, keeping it safe on stock 0.9.4.
- Keep the refusal for real interactive controls, and cover both cases with engine-level tests (synthetic manifests), not captured Claude screens.

## Resolution

Draft, pending [PR #21](https://github.com/saitonakamura/herdr/pull/21) and dotfiles branch `herdr-fork/026-prompt-blocked`.

- **Cause.** The 006 hooks already report the sentinel as `blocked`/`question`, and `agent prompt` already answers a question block when the composer is empty. But hook authority is in memory. After a server restart the screen decides, and screen rules could not name a reason, so the block was refused as plain `agent_blocked`.
- **Fix: a manifest flag the evidence supports, not a caller opt-in.** Manifest engine 5 adds `blocked_reason` to a blocked rule. The matched rule's reason flows into the agent's `blocked_reason` while the screen decides. Unreasoned blocks still refuse with `agent_blocked`, and permission and form blocks keep their own codes. Bundled manifests are unchanged. The user's override gains `claude.v5.toml`, whose ⏳ rule says `question`. Engine-4 builds read `claude.v4.toml` and stock 0.9.4 reads `claude.toml`; both were checked.
- **Hazard found and guarded.** `bash_permission_prompt` (850) ranks under the ⏳ rule (1050), and the rule's `❯` census accepts `❯ 1. Yes`. So a bash dialog raised under the sentinel was read as a question block, and only the composer check stopped the typing. `claude.v5.toml` drops the rule when dialog chrome follows the sentinel. Verified live in a throwaway session.
- **Loom.** No code change needed. Fork builds deliver through `herdr message`, now unblocked. Stock keeps `sentinel_only_blocked`. Its cleanup note and the dotfiles docs no longer call the local rules inert under 006.
- **After merge.** Merge the dotfiles branch (it is inert until an engine-5 build runs). The bundled permission and form rules could declare their reasons too, but that would raise the bundled claude manifest to engine 5 and needs live dialog evidence.

## Parent

[000-map](./000-map.md)
