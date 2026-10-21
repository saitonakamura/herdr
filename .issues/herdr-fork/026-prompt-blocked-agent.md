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

## Parent

[000-map](./000-map.md)
