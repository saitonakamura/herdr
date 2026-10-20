# herdr fork

The user's fork of herdr, a terminal runtime for coding agents, reshaped around
how they run agents: spawn trees, many repos per line of work, vim keys.

## Language

### Agents

**Agent**:
A coding agent with a stable identity that outlives its pane, its sessions and server restarts.
_Avoid_: node, pane

**Parent**:
The agent that started this agent. When the parent is closed, the child becomes top-level.
_Avoid_: spawner, owner

**Message**:
Text one agent sends another through herdr: a `report` upward, a `reply` to a report, or a `tell` downward.
_Avoid_: notification, inbox item

### Agent state

**State**:
What herdr sees an agent doing in its current turn: idle, working, blocked or done.
_Avoid_: status

**Task status**:
What an agent says about its brief: starting, running, needs_parent, needs_user, complete, failed or closed.
_Avoid_: state, node status

**Done**:
The state of an agent that finished a turn which nobody has acknowledged yet.
_Avoid_: finished, unseen, completed

**Acknowledgement**:
Focusing a done agent's pane while the outer terminal has focus. One acknowledgement clears done for every client.
_Avoid_: seen, read

**Blocked reason**:
What a blocked agent waits on: a `question` asked in prose, a tool `permission`, or a `form` to fill in.
_Avoid_: block type, blocker

**Sentinel**:
The last line of an agent's turn, declaring what the agent waits on: the user (`⏳ awaiting your answer`) or background work it started (`🔄 awaiting background work`).
_Avoid_: marker, flag

### Layout and naming

**Workspace**:
A named container of tabs for one line of work. It has no directory, repo or branch of its own; those belong to its panes.
_Avoid_: space, project, repo

**Worktree tab**:
A tab opened for a Git worktree checkout, in whichever workspace asked for it. Removing the worktree closes it; closing it keeps the checkout.
_Avoid_: worktree workspace, child workspace

**Pinned name**:
A tab label set by hand, which stops the tab following its agent's title until it is cleared.
_Avoid_: custom name, manual name

**Sidebar pane**:
A terminal pane that occupies the sidebar slot in every workspace and tab, running a program the user chooses.
_Avoid_: custom sidebar, plugin sidebar
