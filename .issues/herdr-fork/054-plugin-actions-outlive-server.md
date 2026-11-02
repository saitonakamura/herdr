---
id: herdr-fork-plugin-actions-outlive-server
title: Plugin actions outlive the server that started them
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Found while fixing the test leak in [042](./042-test-plugin-process-leak.md), 2026-11-02.

A plugin action, startup hook or event hook is started by `start_plugin_command`
(`src/app/api/plugins/runtime.rs`) as a plain child of the server: it stays in the
server's process group, with stdout and stderr piped, and has no kill guard and no
timeout. Nothing stops it when the server exits, whether cleanly (the comment there
says the worker may outlive `App` during teardown) or uncleanly. It is reparented to
launchd or init and runs until it exits by itself. An action that writes output
usually dies of `SIGPIPE` once the server's pipe readers are gone; a silent one, such
as a wait loop, keeps running. 042 reproduced exactly that on the primary mac: the
`example.live` test's `sh ./read.sh action` outlived its SIGKILLed server under
launchd. Its pane twin died with the PTY's hangup.

The rest are already bounded. Status commands (`tab_bar_status`) and wait readers
(`wait_readers::run_read`) get their own process group and a `StatusCommandGuard` that
kills the group on completion, timeout or drop. That guard still does not run when the
server is SIGKILLed, but those reads are short and timed.

The fix is not a one-liner because of what an action may legitimately do:

- An action that stops or restarts herdr (`herdr server stop`, a live handoff, an
  update) would kill itself if server exit killed its group.
- A live handoff replaces the server. Should an in-flight action survive it, be killed,
  or be handed over? The update test (`plugin_updates_preserve_running_consumers_...`)
  keeps running actions alive across a plugin *update*, not a handoff.
- An unclean death cannot run any guard. Only something like a parent-death signal
  (Linux `PR_SET_PDEATHSIG`; macOS has no equivalent, so a kqueue `NOTE_EXIT` watcher)
  would cover it.

Decide: should actions end with the server, with a timeout, or stay detached by
design? Then implement it, probably as a process group plus a guard on clean exit.
Unclean death would remain a known gap unless it is worth a watcher.

## Parent

[000-map](./000-map.md)

## Resolution
