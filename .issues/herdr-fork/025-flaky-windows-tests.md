---
id: herdr-fork-flaky-windows-tests
title: Stabilize or quarantine two flaky Windows CI tests
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:pC/5beb57a7-1553-47e9-9cc6-5cc5db4ad45b
---

## Question

Two tests fail intermittently on `check (windows-latest)` and pass on rerun, which costs a CI round on most fork PRs:

- `app::api::worktrees::tests::deferred_api_worktree_remove_rejects_duplicate_in_flight_request`: failed four times across fork PRs before 2026-10-20.
- `app::api::plugins::tests::running_plugin_command_keeps_its_files_after_app_teardown`: failed on PR #18 after merging master (2026-10-20), and passed on rerun.

Find each race and fix it deterministically: no sleeps or longer timeouts. Prefer operation-level tests to wall-clock waits, per AGENTS.md. Check whether upstream herdrdev/herdr already fixed either one; if so, take that fix. If a race can't be fixed in reasonable scope, mark the test as Windows-skipped with a comment that names this ticket, and say so in the resolution.

Done when both tests pass in at least five consecutive Windows CI runs of the branch, or one of them is quarantined with a reason.

## Parent

[000-map](./000-map.md)

## Resolution

Draft, pending merge of [PR #20](https://github.com/saitonakamura/herdr/pull/20). Both tests fixed, none quarantined; upstream had fixed neither.

- `deferred_api_worktree_remove_rejects_duplicate_in_flight_request`: the first request was sometimes answered by the Windows-only `git status` dirty precheck before it registered in flight, so the "duplicate" ran as a fresh request (`dirty_worktree_requires_force` 3x, timeout 1x). The first request is now forced, like the duplicate-path sibling; the duplicate stays unforced, so check-order regressions still fail. Why `git status` sometimes calls a fresh checkout dirty is not known: 40 isolated stress runs, 200 status probes and 5 full parallel suites on Windows CI never reproduced it.
- `running_plugin_command_keeps_its_files_after_app_teardown`: PR #18's failure was `ready` not written within the helper's 5s deadline; the worker was `powershell.exe`, whose cold start under load outlasted it. Now a `cmd.exe /d /c worker.cmd` script.
- Verified: both passed in 5 consecutive Windows runs of run 39581348097 (attempts 1–5). Attempt 3 failed on a third flake, `platform::windows::tests::windows_process_cwd_reads_normalized_child_launch_directory`, which upstream CI hits too (9 of its last 25 failures); not in this ticket's scope.
