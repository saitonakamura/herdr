# Each fact has one owner; loom's per-agent facts live in herdr as annotations

Mirrored facts have caused the fork's identity bugs. loom's registry and herdr
each kept their own copy of Parent, name and state, and the copies drifted (024).
Messages and `agent close` act on herdr's copy, so a view drawn from loom's copy
can show one tree while herdr closes another. The rule is now that every fact
has one owner, and readers never join two copies:

- herdr owns everything about a live agent: identity, Parent, state, task status,
  repo facts, names and messages.
- loom owns policy and history: efforts and trackers, routing, limits, briefs,
  closed nodes and outcomes.

loom's facts about a live agent (effort id, role, kind, why, ticket) still need a
home that survives restarts. They go into herdr as **annotations**: an opaque
key-value map per agent, persisted with the agent the way `agent_id` and
`task_status` are. herdr stores annotations and never interprets them. This
extends ADR 0004 without reversing it, since the meaning stays in loom.

## Considered Options

- **Keep loom's facts in metadata tokens.** Tokens are display hints. They are
  not persisted, so every server restart blanks them until `loom restore`
  re-applies them. Persisting them would mean changing upstream's
  `metadata_tokens.rs`, which changes often.
- **Move loom's history into herdr as well.** That would mean herdr keeping
  tombstones of closed agents, which edges into the "all of loom in herdr" option
  0004 rejected.

## Consequences

loom's registry shrinks to policy and history. A live node becomes a herdr agent
plus its annotations, and `loom tree`, `loom ui` and the sidebar all read the
same facts from the same place. The one divergence that remains is deliberate:
when a parent's pane closes, herdr makes the child top-level, while loom's
history still records who started it.
