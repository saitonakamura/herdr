---
id: herdr-fork-dogfood-identity-findings
title: "Dogfood: spawned children get no herdr Parent, and an attached agent takes a stale name"
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by:
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
