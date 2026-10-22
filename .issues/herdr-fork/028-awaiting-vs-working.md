---
id: herdr-fork-awaiting-vs-working
title: "Dogfood: tell an agent awaiting background work from one actively working, and deliver its messages"
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee:
claimed_by: macos/w1:pH/da6b50ba-7857-41bb-9e5b-b7a4516ae495
---

## Question

On 2026-10-21 the cockpit ended its turn on the `🔄 awaiting background work` sentinel after dispatching two children. The local claude manifest maps that sentinel to `working`. Both children's `done` reports (messages 14 and 16) then stayed `pending` for about 18 hours: herdr holds messages for a working agent, and nothing ever ended the "work". This is loom's standard pattern (dispatch, then end on 🔄), so the cockpit never gets woken by its children.

Two problems, one change:

1. **A separate state.** An agent idle at its prompt and waiting on something it started (children, CI, a background shell) is not doing work, and it is not asking the user anything. The user wants the sidebar and the API to show it as its own state, distinct from `working` and from `blocked`. Decide the name with the glossary (GLOSSARY.md), for example `awaiting`. It has to fit the stable client endpoint contract in AGENTS.md: a new value reachable from a frozen codec needs a new codec or an `Unknown` fallback, and older clients and stock 0.9.4 must keep working.
2. **Delivery.** A message to an agent in that state is delivered right away, as it is to an idle one. Its arrival is exactly what the agent is waiting for. Check how this interacts with 026's `blocked_reason` work (PR #21).

Also:

- Detection: the manifest rule that matches the 🔄 sentinel produces the new state. Follow the evidence rules in AGENTS.md, using a throwaway session.
- Dotfiles: the manifest override, the sentinel section of `.config/agents/AGENTS.md`, and loom (anything that reads `working`, such as `loom close` refusing "an agent that ends on a background-work sentinel reads as working for good"). All of it must stay safe on stock 0.9.4: fork-only manifests and config in their versioned or fork files, and real-call capability probes.
- The sidebar: how the new state shows. Follow the existing UI language.

## Resolution

Draft. [PR #23](https://github.com/saitonakamura/herdr/pull/23) merged as e9e8b8fe, and dotfiles branch `herdr-fork/028-awaiting` as 5d5d4cd. Review follow-ups are pending in [PR #24](https://github.com/saitonakamura/herdr/pull/24) and dotfiles branch `herdr-fork/028-awaiting-dot`.

- **Name and look (the user, 2026-10-22).** `awaiting`, shown as a blue ● (dots) or ◷ (symbols). It is in GLOSSARY.md beside idle, working, blocked and done.
- **Source.** The Claude `Stop` hook already reported 🔄, and a running sub-agent, as `working` with `background: true`. The effective state now maps that report to `awaiting`. The hook asset and the report API are unchanged, and hook authority keeps the report as sent, so a handoff from an older build still works. Manifest engine 6 adds `state = "awaiting"` for when the screen decides.
- **Delivery.** Message delivery treats `awaiting` like `idle`. A message held while the agent works goes out on `working → awaiting`. `agent wait` with no `--until` settles on it too. `awaiting → idle` is a completion, and `working → awaiting` is quiet. `agent close` does not count it as busy.
- **Endpoint contract.** The gen-1 snapshot is JSON, and its `agent_status` deserializer already maps unknown strings to `Unknown`. Stock clients therefore see `unknown`, and fork clients decode `awaiting`. Fixtures are untouched and `PROTOCOL_VERSION` is unchanged, because the state never goes over bincode.
- **With 026.** Engine 5 (`blocked_reason`) and engine 6 (`awaiting`) are both kept, and `awaiting` never carries a reason.
- **Dotfiles.** `claude.v6.toml` makes both background rules say `awaiting`. Loom shows it as live and blue, and its close note is scoped to builds without 028. `AGENTS.md`, `herdr.md` and pi's dots are updated too. It is inert until an engine-6 build runs.
- **Verified live.** In a throwaway session with v6, 🔄 reads `awaiting` and a message is typed at once. With v5 the same screen reads `working` and the message is held, which reproduces the bug. Swapping back to v6 delivers it. Over the hook path, `awaiting` held against an idle screen and messages were delivered.
- **Review follow-ups (#24).** A live handoff restored the hook report's raw state, so a background `working` report read `working` for a tick, and a `blocked` one lost its reason. Restore now reads the report through the same helper as the effective state. The Claude `herdr-complete` mod's dots gain `awaiting`, matching pi's.
- **After merge.** Merge #24 and the dotfiles branch `herdr-fork/028-awaiting-dot`. Once every box runs a release with 028, fold `claude.v6.toml` into `claude.toml`.

## Parent

[000-map](./000-map.md)
