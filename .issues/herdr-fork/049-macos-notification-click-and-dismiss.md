---
id: herdr-fork-macos-notification-click-and-dismiss
title: macOS system notifications focus their pane on click and go away once handled
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:p16/559651f7-b169-4e70-9c0a-761596c0063b
---

## Question

Filed 2026-10-27 at the user's request: "have macos notifications dissappear when
the thing that triggered it was handled as well as if its possible to click on
notification to focus the need pane/tab".

Today, with `[ui.toast] delivery = "system"` (the user's config), the client calls
`platform::show_desktop_notification` (`src/platform/macos.rs`). It runs
`terminal-notifier` if installed, else `/usr/bin/osascript`. This box has no
`terminal-notifier`, so every notification is an osascript one: it shows as Script
Editor, a click does nothing useful, and it stays in Notification Center for good.

Windows already has the click half. `notification_policy.rs` attaches a
`ClientSystemNotificationTarget` (endpoint, boot id, pane) on Windows only, and
`ClientLoopEvent::NotificationActivated` focuses that pane in the client that
showed it, then raises the host window. Bring macOS to the same place and add
dismissal.

- **Click.** A click focuses the pane (and its tab/workspace) in the client that
  showed the notification, and brings the host terminal (kitty here) to the front.
  Reuse the Windows target and `activate_system_notification` path; don't add a
  second focus mechanism. A target that is gone (pane closed, server restarted)
  just raises the terminal.
- **Dismiss.** A notification goes away once its cause is handled: the pane is
  seen (the same "seen" that turns done → idle), or the agent leaves the state
  that notified (blocked → working, an answered question). Whichever comes first.
  Also when its pane closes. A new notification for the same pane replaces the old
  one rather than stacking.
- **The helper.** macOS needs a program that can be clicked back and removed.
  Evaluate on this box (macOS 26.6) before picking: `terminal-notifier`
  (`-group`/`-remove`/`-execute`/`-activate`; unmaintained since 2017), `alerter`
  (blocks until clicked, prints the action, `-remove`), or anything else that works
  without an app bundle. Say which and why. How a click reaches the right client
  (a blocking child per notification, a callback command through the server, …)
  follows from that choice; any new API method keeps to the stable client endpoint
  contract and uses a neutral name.
- **Fallback.** Without the helper, keep today's osascript behaviour, and say in
  the docs that click and dismiss need it.
- **Install.** The helper goes in the dotfiles `Brewfile` for both macs. Per
  `~/dotfiles/CLAUDE.md` cross-host rules: update the host docs in the repo, and
  write one dotfiles ticket per mac for the on-box install (primary, music).
- Windows keeps its behaviour. Windows dismissal is out of scope.
- **Tests.** Client tests for the target now attached on macOS, for which state
  changes dismiss which notification, and for the helper's command lines (the
  existing fake-command tests in `macos.rs` show the pattern).
- **Live verify** with a throwaway session on a build of the branch,
  `delivery = "system"`: make an agent pane go blocked/done in a background tab,
  click the notification (lands on the pane), then repeat and handle it in herdr
  instead (the notification disappears).

## Parent

[000-map](./000-map.md)

## Resolution

Draft until [PR #37](https://github.com/saitonakamura/herdr/pull/37) merges.

- **Helper: terminal-notifier 3.x**, through `-execute`, not a waiting process.
  Tested on macOS 26.6 with 3.1.0. macOS hands a click to whichever
  terminal-notifier instance is running, and that instance acts on it as its own
  (no identity check in `AppDelegate.m`), so one waiting helper per notification
  routes clicks to the wrong one. A body click with `-activate` never reaches
  stdout either. `-execute` rides in the notification's `userInfo`, so the
  clicked notification's own command runs: it sends the group to
  `$TMPDIR/herdr-notify-<pid>.sock`, which the client that posted listens on.
  `-group` replaces, `-remove` clears, and `-activate` still raises kitty when the
  client is gone. alerter is only in its author's tap and has the same routing.
- **Click** reuses the Windows target and `activate_system_notification`, now
  built on every platform.
- **Dismiss**: the client removes a notification when the pane's tab is seen
  with the outer terminal focused, when the agent leaves blocked/done, or when
  the pane or its server is gone. Live-verified in a throwaway session for click,
  blocked→working, done→seen and pane close.
- **Permission**: on the primary mac terminal-notifier never prompted; it exited 3
  until allowed in System Settings. That is in the dotfiles `docs/setup/herdr.md`.
- **Dotfiles**: branch `herdr-fork/049-mac-notify` (Brewfile, `config.toml`
  comment, `docs/setup/herdr.md`, tickets 011 primary — installed during this
  ticket — and 012 music).
