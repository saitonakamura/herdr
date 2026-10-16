# 006: hooks decide Claude's state

Design notes, findings and verification for [006](../006-claude-hooks-state.md), per
[ADR 0003](../../../docs/adr/0003-hooks-decide-claude-state.md).

## What Claude Code 2.1.301 sends

Captured with a throwaway `claude -p --settings <capture-only hooks>` (haiku, outside herdr):

- `Stop` carries `last_assistant_message` (the full final text) and `background_tasks`:
  `[{id, type: "subagent"|"shell", status: "running", description, …}]`. No transcript
  parsing is needed on current versions; the hook falls back to the transcript tail for
  older ones.
- A background sub-agent's hand-back and a background shell's completion both re-enter the
  parent through `UserPromptSubmit` (prompt `<agent-message …>`), so the parent reads
  `working` again without any screen help, and the next `Stop` re-decides.
- Sub-agent hooks (`PreToolUse`, `PostToolUse`, `SubagentStop`) carry `agent_id`.
- An Esc interrupt fires no `Stop`, by Claude Code's documented design.

## Mapping (integration v12)

| Event | Report |
| --- | --- |
| `SessionStart` `startup`/`resume`/`clear` | session, then `idle` (`compact` can start mid-turn: session only) |
| `UserPromptSubmit`, `PostToolUse` | `working` |
| `PreToolUse` matcher `AskUserQuestion` | `blocked`, `form` |
| `Notification` `permission_prompt` / `elicitation_dialog` | `blocked`, `permission` / `form` |
| `Stop`, last line `⏳ awaiting your answer` | `blocked`, `question` |
| `Stop`, last line `🔄 awaiting background work`, or a running `subagent` task | `working`, `background` |
| other `Stop`, `StopFailure` | `idle` |

Sub-agent events are dropped, except `Notification`: a permission dialog raised for a
sub-agent waits in the parent pane. Background shells deliberately do not count as live
work (a parked dev server must still read done); the 🔄 sentinel says so when it matters.

## Server side

- `herdr:claude` leaves the reserved session-only list and gets a third authority tier,
  **screen-assisted** (`detect::screen_assisted_hook_authority`): hook state wins, as for
  custom sources, plus two ways the screen may end a reported state:
  - a live turn (`working` without `background`) whose screen has read `idle` for 3 s
    since the report ends: that is the interrupt case;
  - a reported `permission`/`form` whose dialog was seen and is gone falls to the screen
    (`idle` if cancelled, else `working`).
- A screen dialog overrides a hook `working` only; after a stop, every dialog that can
  appear comes with its own hook, so a screen blocker there is stale (this is what keeps
  the dotfiles' `awaiting_user_answer` rule inert under hooks).
- Other non-full-lifecycle sources keep upstream behaviour exactly.
- `pane.report_agent` gains optional `blocked_reason` and `background`; `AgentInfo`,
  `PaneInfo` and `pane.agent_status_changed` gain an optional `blocked_reason`, present
  only while blocked for that reason. Unknown fields are ignored by older servers.
- A session replacement (`/clear`, `/resume`, compaction) moves a screen-assisted
  authority's session ref along; otherwise the authority would keep resuming the old one.
- `agent prompt`: `blocked` + `question` + an empty composer delivers; `permission` and
  `form` refuse with `agent_blocked_permission`/`agent_blocked_form`; a draft or a missing
  composer refuses with `agent_composer_not_empty`/`agent_composer_not_found`; a reasonless
  block keeps `agent_blocked`. The composer is the bottom prompt box (between the last two
  rules) read from the live ANSI screen, faint text ignored (`detect::composer`).

## Mixed fleet

- Upstream 0.9.4 and pre-006 fork servers treat `herdr:claude` state reports as reserved
  (session ref only) and ignore the new params, and their integration status reads v12 as
  current (`>=`), so a v12 profile shared with them changes nothing there.
- Dotfiles `claude.toml` (0.9.4) and `claude.v4.toml` (005 builds) stay. Under 006 hooks
  the three local rules are inert. Delete both once every box runs a 006 release and every
  profile has v12.
- loom keeps its typing fallback only for a plain `agent_blocked`; the named refusals are
  final.

## Verification

See the PR description.
