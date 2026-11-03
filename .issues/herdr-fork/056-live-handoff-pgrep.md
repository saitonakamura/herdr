---
id: herdr-fork-live-handoff-pgrep
title: live_handoff finds the replacement server with pgrep on macOS
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by:
---

## Question

Handed over from the pi cockpit (dotfiles loom node 5ba9b9c0). Found by dotfiles [host-pi 035](https://github.com/saitonakamura/dotfiles/blob/master/.issues/host-pi/035-herdr-tests-under-seatbelt.md),
2026-11-02, on `a084d8d6`.

On macOS, `wait_for_replacement_server_pid` (`tests/live_handoff.rs`) finds the
handoff's replacement server with `pgrep -af herdr-handoff-<old_pid>.sock`, i.e. by
matching every process's argv on the box. Under host pi's seatbelt `pgrep` cannot
work. It needs `com.apple.sysmond`, which answers with every process's argv and
environment, and that stays closed on purpose (dotfiles host-pi 016, 025). So the
callers panic with "replacement server … did not appear; last pgrep output:" (empty):

- `live_handoff_carries_more_panes_than_one_scm_rights_message`
- `live_server_holds_one_pty_master_fd_per_pane`

(The third caller, `live_handoff_unknown_pane_exit_preserves_session_on_shutdown`,
is Linux-only. The Linux helper reads `/proc/<pid>/environ`.)

Searching the whole box is also wrong unsandboxed: a leaked handoff server from
another worktree's run whose old pid happens to match would satisfy it
([042](./042-test-plugin-process-leak.md) has seen such leaks).

The replacement is a child of the old server: `spawn_handoff_import`
(`src/server/handoff.rs`) starts it with `Command::spawn`, and
`detach_server_daemon_command` `setsid`s it into a new session in `pre_exec`. So the test can find it
without reading anyone's argv:

- **By parent pid**: list pids with `proc_listpids`/`sysctl kern.proc` and keep the
  one whose ppid is `old_pid` and whose executable is the herdr under test
  (`proc_pidpath`). Inside the seatbelt that works. Name, ppid and the call's own
  processes stay readable there, and only argv/environment are closed. Race: once
  the old server exits, the child is reparented to launchd, so read it while the
  old server is still up, or fall back to the next option.
- **Ask the server**: if the API or the runtime dir can report the serving pid (a
  pid file beside the socket, or a `pid` in a ping/info response), the test reads
  that instead of looking at processes at all. This is the sturdiest, and would let
  the Linux branch drop its `/proc/<pid>/environ` scan too.

Done: both tests pass under host pi's seatbelt with one `localSockets`
escalation, and unsandboxed, with no `pgrep`/`ps` in `tests/`.

## Resolution

Fixed by [PR #52](https://github.com/saitonakamura/herdr/pull/52) (`b172955f`, merge `e436c016`), done by guarded host pi (loom `207451d4`), committed by the cockpit. Tests only: on macOS the replacement is the old server's child from `proc_listpids(PROC_PPID_ONLY)`, filtered by `proc_pidpath` against the herdr under test. The handoff response comes only after the hand-over, so a wrapper passed as `import_exe` SIGSTOPs the old server and execs herdr; the test finds the child, and an RAII guard resumes the old server (killing it on panic). Linux keeps its `/proc` lookup; no `pgrep`/`ps` left in `tests/`. Asking the server for its pid was not needed. Verified: live_handoff 26/26 inside the seatbelt (only `localSockets` needed, SIGSTOP/SIGCONT asked nothing) and unsandboxed, 40/40 in a serial stress loop.

## Parent

[000-map](./000-map.md)
