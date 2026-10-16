---
id: herdr-fork-native-tab-names
title: Tab names follow their agent natively
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sync-and-branch
assignee:
claimed_by: macos/42d898a1-1af6-4377-8ab9-69653d636787
---

## Question

Move loom 058's rule into herdr: a tab with exactly one agent follows a slug of that agent's title (max length configurable, default 24); a hand rename pins it (a pinned name); renaming to empty or a bare number unpins. Tabs with no agent or several are left alone. Pins survive restarts. Expose pinned/following on the API.

Same change in loom: `lib/names.sh` stops writing tab names and reads them instead; the title-change polling in the status hook goes. Keep loom's tests green.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sync-and-branch

## Resolution

Merged 2026-10-16 as `069d0745`: [saitonakamura/herdr#9](https://github.com/saitonakamura/herdr/pull/9).

- herdr: a tab with exactly one agent follows a slug of its title (`ui.tab_name_max_length`, default 24); a hand rename pins it (`custom_name`); an empty or bare-number rename unpins. The followed name is persisted beside the pin, so neither drops back to the tab number across a restart. `TabInfo.label_source` = `pinned | following | number`; `tab.renamed` fires when a following tab takes a new name.
- loom (dotfiles branch `herdr-fork/009-native-tab-names`, merge with the PR): detects native naming by `label_source` in the startup sync (`names.native` per lane), then writes no tab labels, drops the status-hook title polling and takes node names from tab labels. Stable herdr keeps the old path, which can go once every box runs the fork (014).
