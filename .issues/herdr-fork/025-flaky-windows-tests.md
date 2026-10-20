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
