---
id: herdr-fork-test-user-dirs
title: Plugin tests write the user's real herdr-dev config and state dirs
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

31 unit tests resolve plugin dirs through `crate::config::config_dir()` and
`state_dir()` with neither `XDG_CONFIG_HOME` nor `XDG_STATE_HOME` set, so they
fall through to the platform default and write the **user's real** directories:

- `~/.config/herdr-dev/plugins/config/<plugin-id>` (`plugin_paths::plugin_config_dir`)
- `~/.local/state/herdr-dev/plugins/<plugin-id>` (`plugin_paths::plugin_state_dir`)

via `ensure_plugin_user_dirs`. On the primary mac those dirs now hold 21 and 28
leftover `example.*` plugin dirs from earlier runs. They are debug builds, so it is
`herdr-dev` and not the live `herdr`, but it is still the user's home and the dev
server's state.

The tests:

- 28 in `app::api::plugins::tests` (`src/app/api/plugins/mod.rs`);
- `cli::plugin::tests::cli_user_dir_creation_seeds_legacy_config_before_printing_config_dir`
  (`src/cli/plugin.rs`);
- `app::api::worktrees::tests::deferred_api_worktree_create_preserves_event_and_plugin_context`
  and `deferred_api_worktree_remove_preserves_event_and_plugin_context`
  (`src/app/api/worktrees.rs`).

Under host pi's seatbelt all 31 fail with `plugin_user_dir_create_failed` /
`File exists (os error 17)`: `~/.config` is read-only there, and
`~/.local/state/herdr-dev` is only stat-able as an ancestor, so `create_dir_all`
gets `EEXIST`. That is deliberate on the dotfiles side and stays. The fix belongs here.

Options, roughly in order of reach:

- **A nextest setup script** (`.config/nextest.toml`, `[scripts.setup.*]`) that
  makes a per-run temp dir and exports `XDG_CONFIG_HOME`/`XDG_STATE_HOME` through
  `$NEXTEST_ENV`. It covers every test, future ones included, without touching
  them. But plain `cargo test` would still write the real dirs, and the tests in
  `src/session.rs` that `set_var("XDG_CONFIG_HOME", …)` themselves must keep
  working (they restore the previous value, so they should).
- **`#[cfg(test)]` defaults in `config_dir()`/`state_dir()`**: with no XDG var
  set, point at a per-process temp dir instead of the platform default. Covers
  `cargo test` too. The cost is a test-only branch in production code.
- **Per test**: set both vars in `test_app()` and the CLI test's setup. Narrowest,
  and the next plugin test written forgets it.

Check whether these tests are upstream before picking. If they are, keep the fork's
change small so merges stay cheap (FORK.md). Done: the 31 pass with `~/.config/herdr-dev` and
`~/.local/state/herdr-dev` read-only, and a run leaves both unchanged (compare a
listing before and after). The existing `example.*` leftovers can be deleted by
hand once nothing writes them.

## Resolution

Fixed by [PR #51](https://github.com/saitonakamura/herdr/pull/51) (`02694e0c`, merge `db66f13c`), done by guarded host pi (loom `004127b1`), committed by the cockpit. Chose the `#[cfg(test)]` default: unit-test builds fall back to config and state dirs under a per-process temp root when `XDG_CONFIG_HOME`/`XDG_STATE_HOME` are unset; explicit overrides still win and normal builds are unchanged. The helpers and the affected tests are upstream, so the change stays in `src/config/io.rs` with one regression test. It covers plain `cargo test` too, which a nextest setup script would miss. Verified: the affected group went from 31 failures to 61/61 inside the seatbelt; unsandboxed, sorted listings of `~/.config/herdr-dev` and `~/.local/state/herdr-dev` were identical before and after `just check`. The leftover `example.*` dirs (21 config, 28 state) were deleted by the user on 2026-11-03.

## Parent

[000-map](./000-map.md)
