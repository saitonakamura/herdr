# loom's runtime half lives in herdr; its policy half stays in loom

loom began as a herdr plugin that keeps its own registry of which agent spawned
which, the args and env to resume each one with, and the messages between them.
Keeping that beside herdr's own state cost races on restore, tokens lost on a
server restart, screen checks before typing into a pane, and a bash-only tool
with no Windows port. The fork moves those runtime facts into herdr: a stable
agent identity, Parent, spawn args and env restored on resume, a message queue
per agent, closing a subtree, and a task status the agent reports. loom keeps
the policy (efforts and trackers, routing and limits, briefs, what it draws) and
its command names, as a thin layer over herdr's API.

## Considered Options

- **Keep loom entirely a plugin, adding only Parent.** Rejected: every runtime
  workaround above would stay.
- **Move all of loom into herdr, trackers included.** Rejected: the trackers and
  routing are the user's policy, change weekly, and mean nothing to a runtime.

## Consequences

Upstream plans its own agent nesting (#3153); when it ships, expect to reconcile
it with the fork's Parent. Task status uses its own values (needs_parent,
needs_user, complete, ...) so it never reuses a state word.
