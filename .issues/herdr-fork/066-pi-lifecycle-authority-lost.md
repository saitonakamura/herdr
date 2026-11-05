---
id: herdr-fork-pi-lifecycle-authority-lost
title: A pi pane loses lifecycle authority silently and reads idle while it works
status: in-progress
labels:
  - wayfinder:research
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/wN/pi-migration-cockpit (loom 5454fa09)
---

## Question

Why did a running pi pane lose its state reporting, and does herdr need a separate
lifecycle-authority claim at all? The user is investigating this interactively from
the pt:pi-migration cockpit, which needs correct pi states for its
[staged acceptance](https://github.com/saitonakamura/dotfiles/blob/master/.issues/pi-migration/009-staged-pi-only-acceptance.md).

## The bug

On 2026-11-05 at about 12:48 local, loom node `0a32ecb6` in pane `wN:p9` showed pi's
`Working` spinner while herdr reported `idle`. The turn ran on for several minutes,
and the state never corrected itself. `herdr agent explain` on the three pi panes:

| pane | shown | `agent explain` |
| --- | --- | --- |
| `wN:p9` | `idle`, actually working | `rule: none`, `fallback_reason: default_known_agent_idle_fallback` |
| `wN:pM` | `working`, correct | `screen_detection_skip_reason: full_lifecycle_hook_authority` |
| `wN:pA` | `idle`, correct | `screen_detection_skip_reason: full_lifecycle_hook_authority` |

So `wN:p9` has no effective authority. Its extension keeps reporting through
`scripts/herdr-bridge`, and a report that doesn't count still gets `ok`, per the
bridge's own comment, so neither side sees a failure. `pi.toml` has no rule that
matches the spinner, so the fallback is `idle`. While a pane misreads this way,
herdr treats it as finished and may deliver queued messages mid-turn.

## Cause (2026-11-05)

**A restart doesn't remove authority. A pi pane loses it for good if, during startup,
one process probe reads the pane as `claude` and the session claim and first state
report land on either side of that probe.** After that, herdr drops every report from
that session and still answers `ok`, and the bridge never claims again. Evidence and
what's still inferred are below.

### Is it restart-related?

Not restart-caused, but restart-exposed. Restored panes keep authority on a plain
restart (see Runtime results). The window opens on any launch, before authority exists:
`wN:pZ` flipped on a fresh launch. A pane with authority and a stable process group
isn't exposed, because probes are skipped (`src/pane.rs:704`; case H below). A restart
puts every pi pane through startup at once and under load, and the one stuck pane seen
was in that situation, so a cutover is the likeliest place to hit it. That's one
observed case, so no rate is claimed.

### The mechanism, reproduced on a real server

Installed `0.9.5+fork.2`, isolated server, real API, with a stand-in job: a bash leader,
a process with argv0 `pi` and one with argv0 `claude`, so killing `pi` makes the next
probe read `claude`. Reports used the same two API calls as `scripts/herdr-bridge`.

| case | order | authority at the end |
| --- | --- | --- |
| control | Pi, claim, state | kept |
| **B** | **Pi, claim, flip to Claude, first state, back to Pi, more states** | **none, for good** |
| C | Pi, flip, claim, state, back to Pi | recovers on return to Pi |
| F | Claude first, claim, state, then Pi | recovers |
| G | Claude first, then Pi, claim, state | established |
| H | Pi, claim, state, then `pi` killed | kept; probe skipped |
| R1 | B, then the same session claimed again | **recovers** |
| R2 | B, then a new session claimed | recovers |

Case B matches the bug: `agent` is `pi`, `rule` is none, `fallback_reason` is
`default_known_agent_idle_fallback`. `TerminalState` tests of the same orders in a
throwaway worktree agree, and also get stuck when the claim hasn't landed at all
before the state report.

The code path, at `b358c4e0`:

1. With `claude` detected, `process_present` is false, so `route_full_lifecycle_hook_report`
   creates a suppression entry with reason `ProcessExit` and no `replacement_session_ref`
   (`src/terminal/state.rs:1286`), and returns `Ignore`.
2. When Pi is detected again, `clear_full_lifecycle_hook_suppression_for_detected_agent`
   keeps any `ProcessExit` entry with no replacement (`state.rs:1456`).
3. Every later state report now fails `!contains_key(source)` (`state.rs:1249`), so it
   only overwrites the pending report and gets ignored. The API still answers `ok`.
4. Only a session report with a recognized start source clears the entry, and the bridge
   sends one per session id (`scripts/herdr-bridge:384`). `/reload` keeps both the session
   id and the bridge, so nothing sends another (R1 shows one would be enough).

So the generation gate does what it was built for. Here it reads a live pi as an exited
one, and the one-shot claim makes that permanent.

### Is that what happened to `wN:p9`? Inferred, strongly

`~/.config/herdr/sessions/personal/herdr-server.log`, pane 13:

```text
10:06:16 agent changed previous_agent=None         agent=Some(Pi)     process=pi     pgid=13988
10:06:22 agent changed previous_agent=None         agent=Some(Claude) process=claude pgid=13988
10:06:27 agent changed previous_agent=Some(Claude) agent=Some(Pi)     process=pi     pgid=13988
```

- In the reproduction, a second `None -> Pi` line follows when authority is acquired. The
  healthy live panes have it. `wN:pZ` has it straight after its flip. Pane 13 doesn't, and
  neither does case B.
- Not provable: successful reports aren't logged, so `wN:p9`'s actual claim and state
  timing is unknown, and the pane has since closed. Case B needs the claim before the
  flip and the state after, which are normally milliseconds apart in one bridge call. The
  claim-less variant is the other candidate. Both end the same way.

### Still unknown: why pi is unidentifiable for that one probe

`process_priority` scores pi (argv0 differs from `node`) 3 and `claude` 2, so `claude` only
wins when pi's argv0 can't be read as `pi`. The flip lasts one `PROCESS_RECHECK_IDENTIFIED`
(5s, `src/pane.rs:428`) in both live cases. In an unloaded launch pi had its title about
800 ms before `claude` appeared, so startup order alone doesn't explain it. A failed
`process_argv0_name` read under load (`src/platform/macos.rs:563`) is a candidate, not
measured. Logging the job when `claude` is detected under a `pi-host` leader would settle
it. A fix doesn't need this answer.

### What a fix could be (not decided, nothing changed)

- **Smallest:** make the bridge claim again rather than once per session, since R1 shows
  that recovers the pane. This is a dotfiles change.
- **At the source:** in `state.rs:1456`, when the pending report's session matches the
  persisted session, treat the return to Pi as validating it, as case C does. A dead
  process's late report must stay rejected.
- **Either way:** stop answering `ok` for a dropped report (`src/app/api/panes.rs:1743`),
  and have the pi extension read the reply rather than any bytes
  (`docker/pi/herdr-agent-state.ts:59`), or the next variant is just as silent.

A stuck pane can be recovered today by a new session id (`/new` or `/resume` to another
session), which is the R2 path. A pane restart should do the same, untested.

The upstream sync (`fork/sync-upstream-ee5bfb50`, at this writing) touches neither
`src/terminal/state.rs` nor this probe logic: its `src/detect/mod.rs` change is Hermes
identification, `src/pane.rs` a test helper, and `src/terminal/runtime.rs` two lines.

## What wasn't known at filing

Not proven: nothing yet shows which of these lost `wN:p9`'s authority.

- **Server restart.** Authority lives in server memory, and the servers restarted
  at 12:05–12:06 that day. `scripts/herdr-bridge`'s `AgentReporter` caches
  `claimed_session` and re-claims only when the session id changes, so a bridge
  that outlived the restart would never claim again. But `wN:p9`'s bridge started at
  12:06:17, after that server's 12:06:13, so it didn't outlive this restart: this
  cache can't be the cause here, whatever else a restart does.
- **`hook_authority_is_effective`** (`src/terminal/state.rs:2299`) also needs
  `detected_agent` to be pi and no `recent_agent_process_exit`. Authority could be
  present but ineffective.
- **The session.** The user typed into `wN:p9` during a manual batch. A session
  change is only accepted for `herdr:pi` with source `new`, `resume` or `fork`
  (`session_report_allows_session_replacement`, `state.rs:1641`).

## Findings so far (read-only, 2026-11-05)

`wN:p9` is the only live host-pi pane that herdr restored at the 12:06 restart: its
launcher runs `pi-host --session 01a1a1d1-…`, started 12:06:15. Every pane launched
fresh afterwards reads `full_lifecycle_hook_authority`: `wN:pK`, `wN:pM`, `wN:pN`,
`wN:pR`, `wM:p1T`. That's a lead, from one restored pane, not yet a reproduction.
`wN:pA` was closed before it could be compared, so drop it as a counterexample.

The claim's purpose is in `src/terminal/state.rs:1716-1840`: it's a **generation
gate**. Once a full-lifecycle source's process exits, its reports are suppressed
(`suppressed_full_lifecycle_hook_reports`), so a late hook from a dead process can't
revive the pane. Only a report with a recognized `session_start_source` and a newer
`seq` reopens it. If the process isn't detected yet, that report is recorded as a
replacement and returns `None`. A plain state report carries no start source, and
the bridge sends its claim once per session, so if this gate stays shut nothing
reopens it. Not yet checked: whether that's what happened to `wN:p9`.

## The design question

The user's position: herdr already knows how to behave from the agent, through the
static tiers in `src/detect/mod.rs:359-385`, so why have a runtime authority at all?

What does need a runtime answer is whether a live reporter exists in this pane:
config can't say, because host pi loads the extension from `~/.pi-personal` through
a per-session bridge, and an agent can die mid-state. But a `herdr:pi` report naming
a session is already evidence of a live reporter, so it might establish authority
itself, or be rejected visibly instead of `ok` and dropped. Find what the separate
claim protects, such as stale reports from an old session, and whether anything does
need it. The generation gate above is one answer; open is whether liveness
(`process_present` plus a matching session) could replace the handshake.

## Investigation log (2026-11-05, pimig-066)

### Runtime reproduction status

**Not yet reproduced. No test server has been started by this agent.** Installed
binary confirmed as `~/.local/opt/herdr-fork/bin/herdr`, `0.9.5+fork.2`.
The native tool refused the discovery/read command
`herdr --session repro-066-0b712ca0 pane list` with:

```text
Broker endpoint/config overrides are unavailable in the personal gate
```

This is a tooling restriction, not evidence about lifecycle authority. No bash,
raw socket, alternate binary or sibling was used to bypass it. The cockpit offered
to run the experiment outside this gate; the sequence below is for that executor,
not a record of an executed reproduction.

### Isolation proof (before startup)

Use a **new `mktemp` directory** `R=/var/tmp/h066.XXXXXX`, with no symlinks or copied
live configuration, and session name `r066`. `HERDR_CONFIG_PATH` **alone is not
isolation**: `config_dir()` ignores it. Both XDG roots are essential.

| Concern | Test location / protection | Code establishing it |
| --- | --- | --- |
| Config root | `$R/config/herdr` from `XDG_CONFIG_HOME=$R/config` | `src/config/io.rs:32-36` |
| Base and fork overlay | `$R/config/herdr/config.toml`; no `config.fork.toml` copied | `src/config/io.rs`; fork config overlay contract in `FORK.md` |
| API/client sockets | `$R/config/herdr/sessions/r066/herdr.sock` and `herdr-client.sock` | `src/session.rs:156-186` |
| Restore/pane history | Same session directory, `session.json` and `session-history.json` (and atomic `.json.tmp` siblings) | `src/persist/io.rs:11-16,45-69` |
| Shutdown writes | Shutdown calls `save_session_on_shutdown()`, joining the pending writer and saving before teardown; empty sessions can clear snapshots | `src/server/headless.rs:718-719`; `src/app/session.rs:96-139` |
| Logs | Same isolated session directory, including `herdr-server.log` | `src/server/headless/bootstrap.rs:222-242`; `src/logging.rs` |
| Plugin registry and lock | `$R/config/herdr/plugins.json` (absent) and `.plugins.lock` | `src/persist/plugin_registry.rs:11-16,125-131` |
| Plugin code/config/cleanup | `$R/config/herdr/plugins/`; do not copy/link any live plugin | `src/plugin_paths.rs:5-24,44-48`; `src/plugin_installations.rs:75-100` |
| Plugin hooks | Missing registry loads an empty plugin list; startup/event hooks iterate only registered enabled plugins, so no loom startup/event reconciliation runs | `src/persist/plugin_registry.rs:125-131`; `src/app/api/plugins/runtime.rs:161-221` |
| Plugin state | `$R/state/herdr/plugins/...` from `XDG_STATE_HOME=$R/state`, not the live `~/.local/state/herdr/plugins` | `src/config/io.rs:39-43`; `src/plugin_paths.rs:50-54` |
| Pi loom extension | Normal profile still loads it; clear all inherited loom identity through `env -i`, explicitly set `LOOM_STATE_DIR=$R/state/herdr/plugins/loom` and `LOOM_LANE=r066` | dotfiles `scripts/loom-bridge:104-120,233-251,501-504`; loom `lib/core.sh:20-22,46-73` |
| Loom identity/traffic | No test node is created. `loom.ts` only calls `whoami`/`--glance` at startup/turn; no sentinel prompts. These see the empty isolated registry, never the real tree | dotfiles `pi/extensions/loom.ts:120-137,145-177` |
| Bridge destination | Test server injects its API socket/pane IDs; launcher creates a new bridge. `AgentReporter` captures exactly those values, and per-call JSON uses that socket only | `src/pane.rs:192-229`; `src/integration/env.rs:28-30`; dotfiles `scripts/pi-host:213-224`; `scripts/herdr-bridge:350-356,510-520` |
| Pi conversation files | `--session-dir $R/pi-sessions`, retained in managed launch args on restore | `src/agent_resume.rs:370-433` |
| Shell startup | Empty test `ZDOTDIR` and non-login zsh prevent the user's rc from reintroducing live paths or hooks | explicit environment/config below |

Live sessions discovered read-only: `fork`, `loom-116-check`, `personal`, `work`,
all under `~/.config/herdr/sessions/`. None is below the fresh `/var/tmp` root.
The default/debug runtime is not selected. `env -i` also clears inherited
`HERDR_SOCKET_PATH`, `HERDR_CLIENT_SOCKET_PATH`, pane/workspace/tab IDs, `LOOM_NODE`,
`LOOM_PARENT`, `HERDR_PLUGIN_*`, bridge tokens and all other inherited routing.
Preserve `HOME` solely for the normal personal profile and subscription login;
never copy or print credentials. No live config, installed binary or live plugin
registry is changed.

### Exact cockpit sequence (not run yet)

Run from an unrestricted cockpit shell, **not through the refused child gate**.
Keep `R`, `H`, `pid`, and pane IDs in that shell or a private executor record. Stop
on any unexpected path or response. Use file-writing tools for the config below.

```bash
H="$HOME/.local/opt/herdr-fork/bin/herdr"
R=$(mktemp -d /var/tmp/h066.XXXXXX)
mkdir -p "$R/config/herdr" "$R/state" "$R/cache" "$R/zsh" "$R/tmp" "$R/work" "$R/pi-sessions"
printf 'test root: %s\n' "$R"
```

Write `$R/config/herdr/config.toml` with exactly:

```toml
onboarding = false
[experimental]
allow_nested = true
[terminal]
default_shell = "/bin/zsh"
shell_mode = "non_login"
[update]
version_check = false
manifest_check = false
[session]
resume_agents_on_restore = true
startup_per_agent_delay_ms = 100
[server]
headless_cols = 120
headless_rows = 40
```

Do not copy live `plugins.json`, manifests, overlay or shell rc. Define this
allowlisted environment function; it clears all inherited routing, rather than
trying to enumerate every `LOOM_*` and bridge variable:

```bash
iso() {
  env -i HOME="$HOME" USER="$USER" LOGNAME="$USER" \
    PATH="$HOME/dotfiles/.config/agents/bin:$HOME/.local/opt/herdr-fork/bin:/opt/homebrew/opt/coreutils/libexec/gnubin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    LANG=en_US.UTF-8 TERM=xterm-256color SHELL=/bin/zsh \
    TMPDIR="$R/tmp/" ZDOTDIR="$R/zsh" \
    XDG_CONFIG_HOME="$R/config" XDG_STATE_HOME="$R/state" XDG_CACHE_HOME="$R/cache" \
    HERDR_ENV=1 HERDR_SESSION=r066 HERDR_BIN_PATH="$H" \
    HERDR_CONFIG_PATH="$R/config/herdr/config.toml" \
    PI_CODING_AGENT_DIR="$HOME/.pi-personal" PI_PERSONAL_HOME="$HOME/.pi-personal" \
    LOOM_STATE_DIR="$R/state/herdr/plugins/loom" LOOM_LANE=r066 "$@"
}
iso "$H" --version
iso "$H" config check
iso "$H" session list --json
```

Before launch, verify that the last command lists **only stopped test paths**
below `$R/config/herdr`, no live session or socket; verify no test root entry is a
symlink. The personal profile is deliberately shared, not its session directory.
Never log auth files or full process environments.

```bash
(cd "$R/work" && iso "$H" --session r066 server >"$R/server-launch.log" 2>&1) &
pid=$!
iso "$H" --session r066 pane list
iso "$H" --session r066 session list --json
```

If readiness has not arrived, inspect `$R/server-launch.log`, then repeat the read
manually with a bounded deadline. Do not start a second server. The returned API
and client socket paths and session directory must match the table above.
A headless server suffices; no outer live pane/TUI is needed.

```bash
iso "$H" --session r066 workspace create --cwd "$R/work" --label before-restart --no-focus
```

Read `.result.root_pane.pane_id` from that JSON into `before`; do not guess it.

```bash
iso "$H" --session r066 agent start pi066-before --kind pi --pane "$before" -- --host --session-dir "$R/pi-sessions"
iso "$H" --session r066 pane process-info --pane "$before"
iso "$H" --session r066 agent read "$before" --source detection --format text
iso "$H" --session r066 agent explain "$before" --json
iso "$H" --session r066 agent prompt "$before" 'Reply exactly READY066. Do not use tools.' --wait --until working --timeout 10000
iso "$H" --session r066 agent explain "$before" --json
iso "$H" --session r066 agent wait "$before" --until idle --until done --timeout 60000
iso "$H" --session r066 agent explain "$before" --json
```

Require the screen to show the default OpenAI model, and explain to show
`full_lifecycle_hook_authority` before proceeding. No model override, Claude
provider, API-credit request, tool request, loom spawn/report or sentinel is used.
The launch is the canonical `pi` shim with `--host`, so it reaches real `pi-host`.
If prompt waiting stalls, read before resubmitting; a stalled prompt is itself
evidence. Do not repeat a possibly delivered prompt.

Record a **redacted relevant subset** of the saved session: this pane's agent,
launch args, session reference and restored state, not a full environment dump.
Confirm `$R/config/herdr/plugins.json` is absent/empty and no node JSON exists in
`$R/state/herdr/plugins/loom/lanes/r066/nodes`. Registry/cache directories created
by `whoami` are expected; nodes/messages are not.

```bash
iso "$H" --session r066 server stop
wait "$pid"
iso "$H" --session r066 session list --json
(cd "$R/work" && iso "$H" --session r066 server >"$R/server-restart.log" 2>&1) &
pid=$!
iso "$H" --session r066 pane list
```

Use the restored pane returned here (may not preserve the old public pane ID) as
`restored`. Capture process-info, detection text and explain while idle. Then
launch a **fresh control in this same restarted server**:

```bash
iso "$H" --session r066 workspace create --cwd "$R/work" --label fresh-after-restart --no-focus
```

Read its returned root pane into `fresh`.

```bash
iso "$H" --session r066 agent start pi066-fresh --kind pi --pane "$fresh" -- --host --session-dir "$R/pi-sessions"
iso "$H" --session r066 agent explain "$fresh" --json
iso "$H" --session r066 agent explain "$restored" --json
iso "$H" --session r066 agent prompt "$restored" 'Without tools, list integers 1 to 100, one per line.' --wait --until working --timeout 10000
iso "$H" --session r066 agent read "$restored" --source detection --format text
iso "$H" --session r066 agent explain "$restored" --json
iso "$H" --session r066 agent prompt "$fresh" 'Without tools, list integers 1 to 100, one per line.' --wait --until working --timeout 10000
iso "$H" --session r066 agent read "$fresh" --source detection --format text
iso "$H" --session r066 agent explain "$fresh" --json
```

Observe eventual idle/done with `agent wait`; do not interpret a wait timeout as
proof the prompt was not delivered. Record timestamps for both panes' idle and
working explain results. Restored spinner + fallback idle, with fresh control
retaining authority, reproduces the symptom. Both retaining authority falsifies
this particular restart experiment; it does not prove what happened in the live
pane. Explain alone cannot distinguish private rejection gates: if it reproduces,
next use temporary targeted tracing in a separate worktree/target directory.
Do not infer the winning gate from code or the absence of authority alone.

Cleanup after evidence capture, including on a failed setup:

```bash
iso "$H" --session r066 server stop
wait "$pid"
iso "$H" --session r066 session list --json
iso "$H" session delete r066 --json
iso "$H" session list --json
```

Check no test server remains running and preserve only redacted findings in this
ticket before removing the **exact** fresh `$R` directory. No live pane is closed,
no live server is signalled, no debug/default session is touched. If stop fails,
report the exact owned PID and error; never use broad process matching/`pkill`.

### Supplemental probe limitation

An optional temporary characterization worktree could not be created:

```text
git worktree add --detach ../herdr-worktrees/066-authority-probes HEAD
fatal: could not create leading directories of '../herdr-worktrees/066-authority-probes/.git': Operation not permitted
```

No retry via another path, tool or escalation was made. No tracing build or
supplemental custom test was run. This is a second host-pi dogfood friction point,
not a lifecycle result. The source remains unchanged.

### Contract evidence (not the live cause)

`just test-one 'terminal::state::tests::'`: **112 passed**, 4273 skipped, 0.132 s
execution after a 76 s test-profile build on shared master. No temporary tracing,
PTY/socket reproduction or production change was made. In particular:

- `late_full_lifecycle_hook_with_same_session_after_process_exit_does_not_reacquire_authority`
  demonstrates dead-generation state cannot revive a pane.
- `fresh_detected_process_keeps_old_session_suppressed_after_process_exit` and
  `process_exit_discards_unclaimed_buffered_state_from_that_generation`
  demonstrate that detecting another pi process is not enough to adopt old
  reports. Even a resumed conversation may share its session with a dead process;
  session identity is not process-generation identity.
- `rapid_restart_replays_reports_that_arrive_before_process_evidence` demonstrates
  supported pre-detection buffering and later reconciliation.
- `pi_non_replacement_reports_preserve_full_lifecycle_authority` demonstrates that
  startup/reload without a permitted native session transition cannot steal a
  different live session; `pi_resume_reactivates_a_previously_stale_session`
  demonstrates intentional resume can reclaim it and late reports from the
  replaced session stay rejected.

The existing passing test
`live_full_lifecycle_hook_rejects_different_session_ref_for_same_source` seeds a
persisted session using `anchor_full_lifecycle_session()`, then accepts `Working`
without any separate session report. That helper sets process presence and the
persisted anchor directly. This tests the existing **matching persisted session +
process present + no suppression** acceptance path (`state.rs:1206-1264`), not a
full server restore. A separate claim is therefore already unnecessary in that
specific state. A blanket replacement that ignores suppression still loses
process-generation protection: a restarted process can resume the same session,
so an old report can satisfy the same kind/session checks.

`agent explain` (`src/app/api/agents.rs:451-518`) exposes whether authority is
currently effective, but not whether it was absent, generation-suppressed,
ineffective by process evidence, or session-rejected. Returning fallback idle
therefore cannot identify the failing gate.

Both report handlers call `report_agent_resume()` after the mutation;
`src/app/api/panes.rs:1743-1745` returns `Ok {}` without checking application when
no `resume_argv` was provided. The bridge treats any non-error reply as a successful
claim and caches its session (`scripts/herdr-bridge:384-394`). Separately, the pi
extension treats **any response bytes** as delivered without parsing `ok`/`error`
(`docker/pi/herdr-agent-state.ts:59-60`). Thus visible rejection requires both a
truthful server/bridge outcome and extension response handling; a server error
alone would not currently become a user-visible signal. These are code-confirmed
contract properties, not a demonstrated cause of the reported loss.

## Runtime results (cockpit, 2026-11-05)

Run outside the seatbelt from the pt:pi-migration cockpit, following the sequence
above, with the installed `0.9.5+fork.2` binary. `pimig-066` (loom 0b712ca0) was
closed by the user at 12:37Z, partway through these runs, so this analysis is the
cockpit's and hasn't been reviewed. No live server or pane was touched.

**Restart alone doesn't reproduce it.** In the isolated server:

| action | pane | authority, idle and during a turn |
| --- | --- | --- |
| first launch, one turn | `w1:p1` | kept |
| server restart, restored with `--session`, one turn | `w1:p1` | kept |
| fresh control after the restart, one turn | `w2:p1` | kept |
| `/reload`, one turn | `w1:p1` and `w2:p1` | kept |
| `/new`, one turn | `w2:p1` | kept |
| two more restarts | `w1:p1` and `w2:p1` | kept; `Claude` never detected |

The ten-pane batch didn't test load: only the two panes with saved sessions came
back as pi. This server had none of the live restart's other restoring panes.

**The live log shows a flip.** `~/.config/herdr/sessions/personal/herdr-server.log`,
read-only, times in Z. `wN:p9` is numeric pane 13, matched through shell pid 12001.
The server started at 10:06:13, with about ten Claude Code panes restoring.

```text
10:06:14 pane spawned, shell 12001
10:06:16 agent changed previous_agent=None         agent=Some(Pi)     process=pi     pgid=13988
10:06:22 agent changed previous_agent=None         agent=Some(Claude) process=claude pgid=13988
10:06:27 agent changed previous_agent=Some(Claude) agent=Some(Pi)     process=pi     pgid=13988
```

Its bridge started at 10:06:17, inside this sequence. Later live panes:

| pane | ever read as `Claude` | authority now |
| --- | --- | --- |
| `wN:p9` (13) | yes, 6s after first Pi detection | **none** |
| `wN:pZ` (38), Claude model | yes, 12:24:35 to 12:24:40, 16s after first Pi detection | kept |
| `wM:p1T`, `wN:pK`, `wN:pM`, `wN:pT` | no | kept |

A pi pane's foreground job holds the `bash pi-host` leader, the pi node process (name
`node`, argv0 `pi`), bridges, and `claude` from the gate judge. `process_priority`
scores pi 3 and claude 2, so claude can only win while pi isn't identifiable.

**Hypothesis, not proven:** `claude` masks pi during startup, and the bridge's single
claim lands in that window. The claim gets recorded and dropped, `ok` comes back, the
bridge caches `claimed_session`, and nothing claims again. The `wN:pZ` control fits,
since its authority was established before its flip. Two panes aren't proof, and the
`None` transition could trip the generation gate instead.

**This weakens the lost-claim form of the hypothesis.** `pimig-066`'s last message, delivered after the closure, notes that `route_full_lifecycle_hook_report`
(`src/terminal/state.rs:1206-1264`) accepts a plain state report with no separate
claim when the process is present, the persisted session matches and no suppression
holds. `live_full_lifecycle_hook_rejects_different_session_ref_for_same_source`
covers that path. `wN:p9` has its session persisted, so a dropped claim alone
shouldn't cost it authority once pi is detected again. That points at
suppression from the `None` transition, or a session mismatch. Not yet tested.

**Next, to settle it:**

1. A deterministic `TerminalState` test of this order: Pi detected, Claude detected,
   claim report, Pi detected, plain state reports. Add the claim-before-flip control.
2. From the code: why pi is unidentifiable in that window, including whether
   `process=pi` at 10:06:16 was the `pi` shim rather than the node process.
3. The bridge and extension order of claim versus process title.

If this holds, it bears directly on the design question: a one-shot claim that a
transient misdetection can eat for good, with `ok` returned, is the actual defect.

## Parent

[Fork map](./000-map.md)

## Blocked by

Nothing.
