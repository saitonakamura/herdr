# 005 state consistency: findings and design

Branch `fork/005-state-consistency`. Upstream issues are herdrdev/herdr numbers.

## Root causes

| Issue | Cause | Fix |
|---|---|---|
| #4690 `agent wait` misses a tab close | Waiters (`src/api/wait.rs`) poll the event hub and return only on pane-level events. `tab.close`, `workspace.close` and a layout-replace tab close emitted only `tab.closed`/`workspace.closed`. `pane.exited` never fired because the panes were gone from the layout first. | `App::emit_panes_closed` emits `pane.closed` per pane before `tab.closed`/`workspace.closed` (`tabs.rs`, `workspaces.rs`, `layouts.rs`). Worktree-removal closes shut their panes down earlier and are unchanged. |
| #4473 a focus-caused done → idle never wakes a wait | `switch_tab`, `mark_active_tab_seen` and the client-focus sync set `seen` directly. Nothing emitted `pane.agent_status_changed`, so `agent get` said idle while the waiter heard nothing. | `App.published_done_panes` holds panes last published as `done`. `sync_focus_events` (every server loop) checks only that set and emits the done → idle change through `emit_pane_state_update`. |
| #5222 no client attached → idle, not done | `outer_terminal_focus = None` means both "unsupported" and "no client". Unknown focus counts as watching, so active-tab completions were marked seen. | `AppState.viewer_attached`. The headless server sets it false at start and whenever no foreground client is attached, and true when one syncs. `pane_is_watched` = attached && active tab, used by the three completion and notification paths and the headless helper. |
| #4819 an old spinner above a completed turn reads working | `live_turn_working` scans `bottom_non_empty_lines(12)` regardless of a newer `✻ Cogitated for 17s`. | Engine: rule-level `after_last_line_regex` cuts the region to the text after the last matching line (engine 4). claude.toml: completed-turn line as the frame start on `live_turn_working` and on the `whole_recent` blockers. |
| #2868 narrow panes lose dialog footers | Claude wraps `Esc to cancel` to `Esc to` / `cancel`, and `contains` needed the exact phrase. | Engine: `contains` folds case and every whitespace run, newlines included, to one space on both sides. Words split mid-word still do not match. |
| #3467 a stale busy title outranks dialogs | `osc_title_working` 1100 > every blocker (980/850/840). | claude.toml: `live_blocked_form` and `live_blocked_form_named_session` 1150, `generic_permission_prompt` 1140. All three are current by construction (after the last rule, or `\z`-anchored). `transcript_viewer` goes to 1160 so it still beats a replayed dialog. The `whole_recent` dialog rules stay below the title: a dialog answered earlier in the same turn would otherwise read blocked mid-work. |
| #4875 / #3993 server state latched against the detector | Not reproduced live (no disagreement on the primary mac's panes on 2026-10-15). By reading, two mechanisms fit. (1) The task publishes only on change against its private `state`, while the server can reset `fallback_state` behind its back (hook clear, release, ignored updates). Once they differ, an unchanged screen never republishes. (2) An agent held as a background job (`AgentJobTracker`) skips screen detection and freezes the last state: the #4875 variant, `codex working` with a bash loop in front. | (1) `DetectionResync` re-asserts the last published result every 2s while an agent is identified. An unchanged state costs the server one lookup and no client fanout. (2) While held, a `working` claim is withdrawn to `unknown` once. |

## Decisions

- Remote manifests from herdr.dev shadow fork manifest fixes once upstream publishes a newer version. Out of scope here; that is ticket 021.
- `distribution/agent-detection/claude.toml` = bundled (validator: same version ⇒ same content).
- Mixed fleet: one dotfiles tree serves five boxes that upgrade at different times, and an engine-4 `claude.toml` would be rejected by upstream herdr (engine 3), dropping the ⏳/🔄 sentinel rules. So the fork reads `<agent>.v<N>.toml` for the newest engine N it supports before `<agent>.toml` (`override_path_in`). The dotfiles keep `claude.toml` (engine 3, unchanged rules) and add `claude.v4.toml` (same local rules on the fork's bundled 2026.10.15.1). Older builds never read the v4 file, so the dotfiles change is safe to merge at any time. **Cleanup**: once every box runs a fork release with 005, fold `claude.v4.toml` into `claude.toml` and delete it. The versioned lookup stays for the next engine bump.
- Codex override: no change needed (its local rule is a regex; codex rules gain only wrap-tolerant `contains`).

## Tests

- Engine (synthetic manifests, `src/detect/manifest/tests.rs`): wrapped `contains`, mid-word split, `not` folding; `after_last_line_regex` stale/live/no-boundary/boundary-last; engine-version gate and invalid pattern.
- `src/pane/agent_detection.rs`: `DetectionResync` interval, restart and latest-publish (explicit `Instant`s, no sleeps).
- `src/app/api/tabs.rs`: tab close event order now `pane.closed, tab.closed, workspace.closed`; the workspace-close test filters `pane.closed`.
- `src/app/actions.rs`: active-tab completion with no viewer stays done.
- `src/app/api.rs`: background done, then switch workspace and focus sync, emits idle exactly once.
- `scripts/test_agent_detection_manifest_check.py`: field type and engine gate.
- Manual fixture check of the frame regex (Python translation) against #4819 shapes and a mid-turn `✻ Thought for 3s`.

## Verification (primary mac, 2026-10-15, cargo 1.96.1, zig 0.16.0)

- `cargo fmt --check` and `cargo clippy --all-targets --locked -- -D warnings`: clean.
- `cargo nextest run`: 3904/3907 pass, run with test-only `GIT_CONFIG_GLOBAL` (name and email only). Without it, the global config signs every commit through 1Password, and baseline master and this branch fail the same 44 tests (3854/3898 and 3863/3907): 41 git and worktree tests plus the 3 below. The 3 remaining failures fail identically on baseline master: `api_ping::pane_info_and_subscriptions_expose_done_agent_status` and `cross_area::cross_area_agent_process_survives_detach_and_reattach` (their fake agent is never detected on this mac) and `platform::macos::config_file_tests::config_replacement_preserves_macos_acl`.
- `maintenance-test`, `ui-hot-path-architecture-test`, `integration-assets-test`, `docs-contract-test`: pass. `windows-lint` not run: no xwin SDK on this box, and installing one is out of scope (fork CI should cover it).
- Detection cost, Claude manifest, 66-line screen, release build, 20k evaluations: baseline 22.6 µs per evaluation, branch 22.8 µs. The first cut was 41.9 µs; folding only for rules that use `contains`, plus a reverse frame scan without allocation, brought it back to parity.
- Offline fixtures, fork debug build with the dotfiles `claude.toml` and `claude.v4.toml` side by side: the build picks `claude.v4.toml` (2026.10.15.2, no warning). #4819 finished screen → idle (`live_prompt_box`). #2868 narrow footer → blocked (`live_blocked_form`). A live spinner after an earlier completion → working (`live_turn_working`). Upstream herdr 0.9.4 on the same directory reads `claude.toml` (2026.08.13.3, no warning).
- Throwaway headless session `t005smoke` (isolated XDG config and state, no client attached, `pane report-agent --source repro`):
  - #5222: working → idle in the active tab reads `done`.
  - #4690: `agent wait --timeout 60000` returned `agent_not_running` about 0.1 s after `tab close`.
  - #4473: `agent wait --until idle` on a background-tab agent returned `idle` about 0.2 s after `tab focus` turned done into idle.
  - Session stopped and deleted afterwards.
- Live Claude smoke test (approved by the user), Claude Code 2.1.301, `--model haiku --permission-mode default`, a throwaway session with the v4 override, no client attached:
  - Trust dialog → blocked (`live_blocked_form`), and after accepting → idle (`live_prompt_box`).
  - A Bash write needing approval: working (`osc_title_working`, title `◐`) → blocked (`generic_permission_prompt`). This CLI swaps the title to `✳` once the dialog opens, so the stale busy title (#3467) did not occur live. Rejected with Esc; no file was written.
  - AskUserQuestion: working → blocked (`live_blocked_form`). With the pane narrowed to 48 columns the footer wrapped to `Esc to` / `cancel` and still read blocked (#2868 live). Cancelled → idle.
  - Finished turns read `done` with no client attached (#5222). The real completion line `✻ Churned for 2s · done 10:42 AM` has the shape the frame regex expects.
  - Session stopped and deleted, and the scratch directories removed.
- Not run: `bench-render-scale`. It does not exercise screen detection, so the cost above was measured directly.
