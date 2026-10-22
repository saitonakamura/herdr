---
id: herdr-fork-link-hover-status
title: Hover hook and overlay for live link status
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Requested by the dotfiles terminal-clickability effort ([its map](https://github.com/saitonakamura/dotfiles/blob/master/.issues/terminal-clickability/000-map.md), 366efd2), for its `links` plugin. Nothing in that effort waits on this ticket: peek covers the same information on demand.

The plugin has one status-line command per link kind: Jira status and assignee, MR state and pipeline, a pane's agent state, a loom node's state, a commit's subject and author. Peek uses them now. This ticket lets herdr show the same line while the pointer rests on a link.

- **Hook.** When Ctrl+hover settles on a link a plugin handler claims (OSC 8, http(s), or [036](./036-plugin-link-patterns.md)'s text patterns), herdr runs the handler's declared status command with the link in the env and reads back one line.
- **Overlay.** It draws that line next to the link, a tooltip that is TUI presentation state and lives client-side only. It goes away when the pointer moves or Ctrl is released.
- **Cost.** Hover is a hot path, and these commands hit the network. Debounce the hook, run it off the render thread, cache per link text for a short time, cancel stale runs, and draw nothing until the reply comes in. A slow or failing command must never stall a frame.
- **Boundary.** The hook and its result are a runtime fact that the server can expose through the API. The overlay's placement and style are client state. Use neutral names.
- **Compatibility.** As in [037](./037-link-click-modifiers.md): a new advertised method or capability, so a stock server simply shows no overlay.
- **Consumer, ready (dotfiles terminal-clickability 004, 3d86e34).** The `links` plugin's `links-status [--kind K] [--cwd DIR] TARGET` prints one status line per link: url, localhost, path and stack frame today, with hashes and ids coming in its 005/006. That is the contract the hook calls. `--cwd` is the pane's cwd, which herdr has to pass, because the plugin runs in herdr's env, not the pane's.

## Parent

[000-map](./000-map.md)

## Resolution
