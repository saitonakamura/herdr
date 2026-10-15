---
id: herdr-fork-native-tab-names
title: Tab names follow their agent natively
status: open
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
  - herdr-fork-sync-and-branch
assignee:
claimed_by:
---

## Question

Move loom 058's rule into herdr: a tab with exactly one agent follows a slug of that agent's title (max length configurable, default 24); a hand rename pins it (a pinned name); renaming to empty or a bare number unpins. Tabs with no agent or several are left alone. Pins survive restarts. Expose pinned/following on the API.

Same change in loom: `lib/names.sh` stops writing tab names and reads them instead; the title-change polling in the status hook goes. Keep loom's tests green.

## Parent

[000-map](./000-map.md)

## Blocked by

- herdr-fork-sync-and-branch
