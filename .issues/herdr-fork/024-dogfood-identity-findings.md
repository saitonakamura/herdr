---
id: herdr-fork-dogfood-identity-findings
title: "Dogfood: spawned children get no herdr Parent, and an attached agent takes a stale name"
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:p7/f8a0c8a8-50b6-407f-866e-27225a1bb5f1
---

## Question

Found on 2026-10-20 when the cockpit moved into the `hf` side session (herdr session `fork`, build
master a7fd581e, dotfiles 6480927). There are two defects in agent identity from 010 and in loom's half of it.

1. **`loom spawn` left herdr's Parent unset.** The cockpit (82993002) spawned three claude children:
   994e7fb2, 58523660 and 8d5c7a5b. Loom's registry has `parent: 82993002` for each. But `herdr agent list`
   showed no `parent_agent_id` for any of them until the cockpit ran `herdr agent set-parent <pane> 82993002`
   by hand, which succeeded. Loom's `herdr_parent` (lib/core.sh) swallows set-parent errors with
   `2>/dev/null`, so it either wasn't called on the spawn path or failed silently. Also, the nodes' `agent_id`
   is `null` in the loom node files.
2. **The attached cockpit took a stale name.** The cockpit was a `claude --resume` in a fresh `hf` tab, then
   `loom attach --effort pt:herdr-fork --name herdr-fork`. It got herdr agent id 82993002, but its name
   (in `agent list` and loom) is `sleep-15-smoke-test`. That looks like a name left over from an earlier agent
   or a smoke test in that pane or session, perhaps 022's verification run. Find where the name came from.
   If an identity was reused for a different agent, decide what the rule should be. The glossary says an
   Agent's identity outlives its pane, so a new agent in an old pane should not inherit the old agent's identity.

Reproduce each in a throwaway session on the fork build, then fix them in herdr and/or loom with tests.
`--name` given to spawn and attach should be honoured.

## Parent

[000-map](./000-map.md)

## Resolution

Draft, in progress: [PR #17](https://github.com/saitonakamura/herdr/pull/17) (branch
`fork/024-dogfood-identity-findings`) and dotfiles branch `herdr-fork/024-dogfood-identity-findings`.

1. **Parent.** herdr's `agent start` without `--parent` reset `parent_agent_id` to none (`start_agent` passed
   `Some(None)` to `assign_agent_identity`), wiping the Parent that loom's spawn had just set on the shell pane.
   Reproduced on `0.9.5+fork.1` in a throwaway session (`1e51e345` → `null`). The PR keeps the pane's Parent unless
   `--parent` is given. loom's spawn also sets it again after the start, so the deployed build is covered, and it now
   warns when herdr refuses a herdr id as Parent. The loom node files have no `agent_id` field by design: since 010
   a node's `id` is the herdr agent id.
2. **Stale name.** No identity was reused. The session snapshot from just before the attach shows no pane with an
   `agent_id`, and 82993002 was minted by the attach. The name came from tab `w1:t1`'s label. On 2026-10-16, loom
   wrote that label with `tab rename` while following a smoke-test claude's title under a pre-009 build. Herdr
   persisted it as `custom_name`, which 009 reads as a pin, and loom's names sync handed the pin to the newly attached
   node over `--name herdr-fork`. On a herdr that names tabs, loom's names sync now unpins a pinned label equal to
   the one it recorded writing. Reproduced and fixed live with `loom attach` in a throwaway session.
3. **`--name`:** still to decide. It depends on how an explicit name should interact with tabs following titles.
