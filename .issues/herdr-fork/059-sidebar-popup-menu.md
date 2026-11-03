---
id: herdr-fork-sidebar-popup-menu
title: herdr-sidebar menu as a popup, with loom's open report, ack and close
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/831d89ca-42e4-40a0-a329-ed139e83d95a
---

## Question

Raised by the user on 2026-11-03: "can we add all possible actions with the pane to the sidebar?" Narrowed in the same conversation to loom's actions; herdr's own pane actions (split, zoom, swap, rename pane, move) are out.

Today `m` (or right-click) on a row opens an inline menu at the bottom of the sidebar pane. It is narrow, truncates labels and covers rows. The agent menu has reply, tell, close subtree, scope to subtree and open in loom ui; `r`, `t`, `c`, `a` and `p` run some of them as top-level keys. Ack (`a`) is reachable only by its key, and close is always `--outcome ok`: when `loom close` refuses a user-audience node because its last report is unread, the sidebar shows the refusal and the user has to ack and close again.

## Decided with the user (2026-11-03)

- **The menu is a herdr popup.** `m` and right-click open it through `popup.open` (the API the sidebar already uses for `[sidebar.actions]` popups), over the whole window, running a picker such as `herdr-sidebar menu <target>`. The popup has no sidebar state, so it reads what it needs through the API itself. The inline menu goes away, for agent rows and workspace headers alike.
- **The popup** shows a header (the node's name and its latest report line) and one entry per action with its key. Pressing the key, j/k and Enter, or a click runs an entry; Esc closes the popup.
- **Agent menu entries:**
  - **open report**: the latest report shown in the popup; closing it acks the report.
  - **ack**: `[sidebar.actions] ack`, as `a` runs now.
  - **reply**, **tell**: as now; their prompts live in the popup.
  - **close**: pick an outcome (ok or struggled). When loom refuses, offer force. When the only reason for refusing is the unread report, offer "ack and close", showing the report's kind and summary line so the user isn't acking blind.
  - **scope to subtree**, **open in loom ui**: as now.
- **Workspace menu entries:** rename, close, new worktree and collapse others, in the same popup.
- **Keys:** the top-level `r`, `t`, `c`, `a` and `p` go; those letters work inside the menu only. Enter on a row still focuses the pane. Navigation, folding, `/`, `f` and `m` stay.
- **Wording is "ack" everywhere**: UI labels, status lines, code names, `FORK.md` and dotfiles `docs/setup/herdr.md`. `GLOSSARY.md` renamed **Mark read** to **Ack** in the commit that filed this ticket.
- **Scope:** `tools/sidebar` and docs only. `[sidebar.actions] ack` and `close` in dotfiles `config.fork.toml` stay; only docs wording changes there.

Not chosen: herdr's pane actions in the menu; loom's show, inbox, answer, move, attach, brief, escalate, park in the menu (the user wants ack and close only); an `escalated:<to>` outcome.

## Resolution

Done by [PR #53](https://github.com/saitonakamura/herdr/pull/53) (`5001cd62`, merge `8a537c45`), loom `15c8c116`; tools/sidebar and `FORK.md` only, no server change. Dotfiles half on branch `herdr-fork/059-sidebar-popup-menu` (`d858736`, pushed, not merged): `docs/setup/herdr.md` sidebar lines and the `config.fork.toml` comments, including the prefix+alt+a description, now "loom ack: the focused pane's report".

- **Target and config.** `m` or right-click calls `popup.open` with `herdr-sidebar menu pane|workspace <id> [--refocus]` and passes `HERDR_SOCKET_PATH`, `HERDR_CONFIG_PATH` and `HERDR_SESSION`. The popup loads the same config, so it gets the same `[sidebar.actions]`, and reads the snapshot itself. On Esc it calls `session_pane.focus` before exiting, which puts focus back on the sidebar.
- **Open report.** The latest `message.list` entry from the node's agent id, whose `body` field the sidebar now reads. Closing the report view runs `[sidebar.actions] ack`. With no ack action configured, ack and the auto-ack say "no [sidebar.actions] ack configured".
- **Close.** With `[sidebar.actions] close` configured, the menu asks for an outcome (ok or struggled, with an optional why) and runs it with stderr captured. loom has no structured refusal, so the menu parses it: the header "the user still has it" plus `<name> [<id>] <reason>:` lines are a held refusal, and "still working or blocked" is a busy one. A held refusal whose only reasons are `unseen` offers "ack and close", showing the report's kind and summary; any other refusal offers force. Without a close action it falls back to herdr `agent.close`, using `agent_subtree_busy` to offer force.
- The top-level `r` `t` `c` `a` `p` keys are gone. Workspace menus carry rename, close, new worktree and collapse others; collapse others is also a new `ctl collapse-others`. The menu model is pure and unit-tested; the PR has live captures from a throwaway session.

## Parent

[herdr-fork map](./000-map.md)
