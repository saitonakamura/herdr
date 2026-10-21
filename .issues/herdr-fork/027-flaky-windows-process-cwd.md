---
id: herdr-fork-flaky-windows-process-cwd
title: Stabilize the flaky Windows process cwd test
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:pF/09a8703a-0d0e-481b-9297-6202613ab131
---

## Question

`platform::windows::tests::windows_process_cwd_reads_normalized_child_launch_directory` fails intermittently on `check (windows-latest)`. It failed on attempt 3 of the 025 verification loop (run 39581348097), and accounts for 9 of upstream herdrdev/herdr's last 25 Windows CI failures.

Find the race and fix it deterministically, with no sleeps or longer timeouts, same rules as [025](./025-flaky-windows-tests.md). Check upstream for a fix first and take it if one exists. If it can't be fixed in reasonable scope, skip the test on Windows with a comment naming this ticket.

Done when it passes in at least five consecutive Windows CI runs of the branch, or it is quarantined with a reason.

## Parent

[000-map](./000-map.md)
