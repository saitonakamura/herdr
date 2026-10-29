---
id: herdr-fork-test-plugin-process-leak
title: Test harness leaks plugin processes
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: claude-cloud-trial (dotfiles root 504cf489)
---

## Question

Reported from the dotfiles pane on 2026-10-22. Two `sh ./read.sh action` loops outlived their test by six days. They came from the `example.live` plugin, with cwd `/private/tmp/hcli-*/config/herdr-dev/plugins/github/example.live-*`, and were orphaned under launchd after their test session and temp dir were gone. Each one ran `sleep`, which is GNU `gsleep` on that `PATH`, in a tight loop, about 20 processes a second between them. On the primary mac Jamf Protect inspects every exec, which cost about 200% CPU and about 600 `Failed to get proc info` log lines a second. Both were killed by hand on 2026-10-22.

Source: `plugin_updates_preserve_running_consumers_and_failed_activation` in `tests/cli/plugins.rs`, an upstream test (`b977116c`, #4245). Its `read.sh` loops `while [ ! -e "$HERDR_PLUGIN_ROOT/release" ]; do sleep 0.05; done`. If the test panics or times out before it writes `release`, or the root is deleted, nothing ends the loop. Teardown doesn't either: `SpawnedServerProcess::drop` (`tests/cli/harness.rs`) SIGKILLs the server process alone, so the server never gets to clean up its plugin action and pane processes.

- **Teardown.** Test teardown kills the whole plugin process group or session, not just the server pid. Reaping every spawned server's descendants in the harness, beside `unregister_spawned_herdr_pid`, would cover every test, not only this one.
- **The example loop.** It exits when its root disappears or the server is gone (for example, `[ -d "$HERDR_PLUGIN_ROOT" ]` and a deadline). Grep `tests/` for other `while … sleep` plugin scripts with the same shape.
- **Production.** Check whether a real server that dies uncleanly orphans plugin actions the same way. Status commands get their own process group and a kill guard (`StatusCommandGuard`, `src/platform/unix_common.rs`). Check whether plugin actions get the same.
- The test is upstream code. Fix it here, fork-only, kept small so upstream merges stay cheap (FORK.md).

## Parent

[000-map](./000-map.md)

## Resolution
