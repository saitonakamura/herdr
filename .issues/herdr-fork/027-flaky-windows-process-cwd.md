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

## Resolution

Draft, pending [PR #22](https://github.com/saitonakamura/herdr/pull/22). Quarantined narrowly, not fixed: the race could not be reproduced.

- **Signature.** All 6 upstream failure logs inspected, plus ours, fail the same assertion: `SysWOW64\cmd.exe live cd`. The x86 shell's launch cwd reads correctly, then after `cd /d …\Changed` the cwd still reads as the launch dir for the full 5s. System32 cmd and the WOW64 launch-cwd read never failed. Upstream had no fix.
- **Ruled out.** `writeln!` splitting the line into several pipe writes: cmd waits for the newline even with a 300ms gap mid-line (1,290 runs, 0 fails). Latency under load: 16 workers + 4 CPU burners, 192 runs, 0 fails, max `cd` latency 743ms on both archs.
- **No repro.** The real test, instrumented (cmd stderr, 30s late re-poll, liveness probe), passed in 24 full Windows suite runs (8 shards × 3, 3,575 tests each, run 39605806264). The race is real but rarer than CI can loop at reasonable cost.
- **Quarantine.** Only the SysWOW64 live-cd half is skipped, with a comment naming this ticket. The WOW64 launch-cwd assertion still covers the 32-bit PEB read, and System32 keeps all three assertions.
- **To lift it:** capture a failure with cmd's stderr and a late re-poll. That tells "cd never ran" (cmd) apart from "cd ran but the Peb32 read is stale" (herdr).
