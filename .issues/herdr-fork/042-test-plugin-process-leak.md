---
id: herdr-fork-test-plugin-process-leak
title: Test harness leaks plugin processes
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/ab3a1583-d381-4077-be25-464f623a16da
---

## Question

Reported from the dotfiles pane on 2026-10-22. Two `sh ./read.sh action` loops outlived their test by six days. They came from the `example.live` plugin, with cwd `/private/tmp/hcli-*/config/herdr-dev/plugins/github/example.live-*`, and were orphaned under launchd after their test session and temp dir were gone. Each one ran `sleep`, which is GNU `gsleep` on that `PATH`, in a tight loop, about 20 processes a second between them. On the primary mac Jamf Protect inspects every exec, which cost about 200% CPU and about 600 `Failed to get proc info` log lines a second. Both were killed by hand on 2026-10-22.

Source: `plugin_updates_preserve_running_consumers_and_failed_activation` in `tests/cli/plugins.rs`, an upstream test (`b977116c`, #4245). Its `read.sh` loops `while [ ! -e "$HERDR_PLUGIN_ROOT/release" ]; do sleep 0.05; done`. If the test panics or times out before it writes `release`, or the root is deleted, nothing ends the loop. Teardown doesn't either: `SpawnedServerProcess::drop` (`tests/cli/harness.rs`) SIGKILLs the server process alone, so the server never gets to clean up its plugin action and pane processes.

- **Teardown.** Test teardown kills the whole plugin process group or session, not just the server pid. Reaping every spawned server's descendants in the harness, beside `unregister_spawned_herdr_pid`, would cover every test, not only this one.
- **The example loop.** It exits when its root disappears or the server is gone (for example, `[ -d "$HERDR_PLUGIN_ROOT" ]` and a deadline). Grep `tests/` for other `while … sleep` plugin scripts with the same shape.
- **Production.** Check whether a real server that dies uncleanly orphans plugin actions the same way. Status commands get their own process group and a kill guard (`StatusCommandGuard`, `src/platform/unix_common.rs`). Check whether plugin actions get the same.
- The test is upstream code. Fix it here, fork-only, kept small so upstream merges stay cheap (FORK.md).

- **Handoff servers leak too.** Seen 2026-11-02: six debug `herdr server --handoff-import /tmp/hlh-*/config/herdr-dev/herdr-handoff-*.sock` processes, two each from the `028-gate-probe`, `loom-100-sources-run` and `041-spaces-repo-token-check` worktrees, still running 1.5–3 hours after their `just check` runs (one worktree already deleted). These are the import side of a handoff test, which the harness doesn't track as a spawned pid, so the teardown fix should reap them as well.
- **Every full sandboxed run leaks two.** Reported by the pi cockpit on 2026-11-02 (dotfiles host-pi 035): each full herdr test run under host pi's seatbelt leaves two handoff servers behind. [056](./056-live-handoff-pgrep.md) is the pgrep half of the same tests.

## Parent

[000-map](./000-map.md)

## Resolution

Fixed in [PR #49](https://github.com/saitonakamura/herdr/pull/49), merge commit `6cf5523a`
(change `be94a58b test: reap every process a test server leaves behind, on macos too`), 2026-11-03. Tests only.

- **Root cause.** The harness's process scan read `/proc` only, so on macOS teardown
  found nothing and reaped nothing beyond the server pid it SIGKILLed. Two leaks
  followed, both reproduced on the primary mac with the old harness: a forced
  live_handoff failure left `herdr server --handoff-import /tmp/hlh-…` under launchd,
  and the `example.live` update test, which fails naturally on macOS after `read.sh`
  starts, left `sh ./read.sh action` under launchd with cwd in `/private/tmp/hcli-…`.
- **Teardown, once, in the shared harness.** New `tests/support/processes.rs` gives
  every pid's exe, argv and environment on Linux (`/proc`) and macOS
  (`proc_listallpids`, `proc_pidpath`, `KERN_PROCARGS2`). `cleanup_test_base` and
  `cleanup_registered_herdr_pids` now stop the servers first, then SIGTERM, and
  after 400 ms SIGKILL, every process whose `XDG_RUNTIME_DIR` or `HERDR_SOCKET_PATH`
  points into a test's runtime dir: plugin actions, hooks and orphaned handoff imports.
  The worktree-server scan uses the same table, so it works on macOS too.
- **The example loop.** `read.sh` exits when its plugin root is gone or after 600
  iterations (30 s); the `example.race` build loop got the same bound. No other
  `while … sleep` plugin scripts in `tests/`.
- **Verified** on the primary mac. After the forced handoff failure, the natural and
  forced plugin failures and a full `cargo nextest run --no-fail-fast` (4264/4267,
  the three known local failures), `pgrep -fl` showed nothing left behind. CI was green
  on all five jobs, macOS included. Untested under host pi's seatbelt, where `sysctl`
  on other pids may be denied (see [056](./056-live-handoff-pgrep.md)).
- **Killed by hand.** 14642, 14905, 65910 and 66144 were handoff imports from the
  `028-gate-probe` worktree binary, orphaned about 1.5 h. 14656 (a handoff import)
  and 23462 (`sh ./read.sh action`) came from my own baseline runs. Every one was a
  `herdr-worktrees/*/target/` binary or had cwd in a test temp dir.
- **Production.** Plugin actions and hooks have no process group, guard or timeout
  and outlive both clean and unclean server exits; a silent one runs forever. Not a
  small fix, filed as [054](./054-plugin-actions-outlive-server.md).
- **Aside.** `tests/cli.rs` is compiled out on macOS
  (`#![cfg(all(unix, not(target_os = "macos")))]`) and macOS CI excludes
  `binary(live_handoff)`, so macOS CI never runs the plugin or handoff tests. The leak
  reproduced only in local runs with the gate lifted.
