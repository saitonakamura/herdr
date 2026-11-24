---
id: herdr-fork-osc-7501-parse-and-answer
title: Answer the OSC 7501 support query and keep each pane's program-status records
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/w1:p2B/c3c4ded8-3c27-491b-83ae-b5b5ca88b9dd
---

## Question

Claude Code and pi already speak OSC 7501, the
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status) (rev 0.4,
2026-10-10). Programs report their state down their own PTY, but **only once the terminal answers a support
query**, and herdr doesn't answer, so they stay silent in herdr panes. Background and the agent survey:
[067 § 6](./assets/067-herdr-app-and-core-comparison.md).

This ticket makes herdr answer the query and keep what each pane reports. **It does not change any pane's
state.** Making the records a state source is [071](./071-osc-7501-state-source.md).

## The protocol, as it matters here

- **Report:** `ESC ] 7501 ; key=value:key=value…` ended by ST or BEL.
  - `state`: idle | working | done | blocked | error | clear
  - `kind`: permission | question | auth, with blocked only
  - also `progress`, `app`, `title` and `msg` (base64), and `id`. A slash-separated `id` makes child
    records; the root record has no id.
  - Each report replaces its record.
  - An `id` that breaks the grammar voids the whole report.
- **Query:** `ESC ] 7501 ; ?`. Reply
  `ESC ] 7501 ; ?:states=idle,working,done,blocked,error:kinds=permission,question,auth`, ended with
  **the query's own terminator**. That is what Ghostty sends (`b7207b2e`); pi's regex and Claude's
  `startsWith("?")` both accept it.
- **Answer order:** the reply must reach the program **before the DA reply to a `CSI c` in the same
  chunk**. pi sends the kitty query, the 7501 query and DA in one write (`pi-tui/dist/terminal.js:206-211`).
  If DA answers first, pi decides 7501 is unsupported.
- **When records go away:**
  - on process exit and on OSC 133 `A`, drop `working` and `blocked`; `idle` may go too
  - `done` and `error` survive both
  - RIS clears everything; DECSTR doesn't

## Where it goes

**Use herdr's own OSC scanner, not libghostty.** The user decided on 2026-11-24 to stay on upstream herdr's
libghostty-vt pin (`44f2a44d`), so there's no vendor bump and no new patch.

- `OscStreamCollector` (`src/pane/osc.rs:352`) already runs on every PTY chunk in `process_pty_bytes`
  (`src/pane/terminal.rs:1411-1421`). It already feeds OSC 0/2 titles and OSC 9 progress into
  `AgentOscStateTracker` (`osc.rs:501`).
  - Add a `7501` arm there.
  - The only new per-byte work is a prefix compare that this arm adds. That corrects 067's "hand-rolled
    scanner" worry.
- Reply through `write_pty_bytes_with_ordered_responses` (`terminal.rs:1512`). It already places herdr's
  own OSC 10/11 and XTGETTCAP replies at their byte offsets, ahead of libghostty's
  `pending_pty_responses`. Check that the 7501 reply lands ahead of a DA reply to a `CSI c` later in the
  same chunk.
- **Nesting (user, 2026-11-24):**
  - herdr answers for its own panes, keeps their records, and never forwards a pane's query or reports
    to the outer terminal.
  - A herdr running inside a herdr pane answers for its own panes the same way.
  - Reporting herdr's own aggregate upward is out of scope.

## Records

- **One set per pane:**
  - a root record plus child records, keyed by `id`
  - each holds state, kind, progress, app, and decoded title and msg
- **Bounded:**
  - cap the number of children (Claude sends at most 32)
  - cap decoded string lengths
  - the collector's 4096-byte buffer cap still applies
- **Clearing:**
  - a `clear` removes that record and its children
  - RIS clears the pane
  - on the pane's agent or process exit, drop `working` and `blocked`
- **No OSC 133:** herdr doesn't consume OSC 133 today. Don't add it unless a live test shows a
  shell-prompt race. All three agents send `clear` on exit.
- **Visible:** show the records in `herdr agent explain <pane>` (JSON and text). Keep them in server
  state, not in the TUI.

## Done

- **Engine tests** (no agent screens) for:
  - query reply text and its terminator
  - reply order against DA in one chunk
  - sequences split across chunks
  - BEL and ST endings
  - an invalid id voiding the report
  - child records and `clear`
  - caps and RIS
- **The per-chunk path:** say what the new arm adds to it. If it adds more than a prefix compare, profile
  with `just bench-render-scale` at 1 and 15 panes.
- **Live check, recording versions:** in a throwaway session, `agent explain` shows records from:
  - **Claude Code** (2.1.331 now). It probes with a 2 s timeout. Its gates are
    `CLAUDE_CODE_DISABLE_TERMINAL_TITLE`, background workers and VTE < 53.
  - **host pi 1.2.0** and **sandbox pi 1.1.0** (`docker/pi/package.json:7`). `PI_PROGRAM_STATUS=1|0`
    forces it either way.
  - **Codex 0.166.0**, which sends without probing.

  Also check Claude's OSC 0 titles. `claude.toml` has `osc_title` rules, and Claude may change titles
  once 7501 is answered.
- **Review:** the PR touches server state, so it comes to the cockpit for review.

## Parent

[000-map](./000-map.md)
