---
id: herdr-fork-osc-7501-parse-and-answer
title: Answer the OSC 7501 support query and keep each pane's program-status records
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/c3c4ded8-3c27-491b-83ae-b5b5ca88b9dd
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

## Resolution

Merged 2026-11-24 on the user's word: [saitonakamura/herdr#65](https://github.com/saitonakamura/herdr/pull/65) as `a6aa5019` (head `5bf59f6b`), all CI checks green.

- **Answer:** herdr replies to `OSC 7501 ; ?` in every pane with the full rev 0.4 form, the query's terminator echoed, at the query's byte offset, so it precedes the DA reply to a later `CSI c` in the same chunk.
- **Records:** `src/pane/program_status.rs`, fed from a `7501` arm in `AgentOscStateTracker`. `agent explain` shows them as `program_status`.
- **Live, in a throwaway debug session:** Claude Code 2.1.331, host pi 1.2.0, sandbox pi 1.1.0 and Codex 0.166.0 all reported. `kind=permission`, `kind=auth` and child ids were not seen live; engine tests cover them.
- **Per-chunk cost:** one byte compare per ESC outside a string, one slice compare per completed OSC body that is not 0, 2 or 9. Nothing per ordinary byte.

What this ticket got wrong or left loose, for [071](./071-osc-7501-state-source.md):

- **Claude drops the spinner glyph from its OSC 0 title once the query is answered.** `claude.toml`'s `osc_title_working` and `osc_title_idle` no longer match; Claude panes run on screen rules until 071. Codex's title rules still match.
- Over-limit or undecodable `title`/`msg` voids the whole report, as the spec says; nothing is truncated.
- The cap is 64 records per pane including the root, least recently updated evicted.
- An `id` pair with a byte outside the value set voids the report instead of being skipped, which would address the root.
- When the pane's agent goes away `idle` is dropped too; only `done` and `error` stay.
- `agent explain` needs a detected agent, so a shell pane's records cannot be read.
- Records do not survive a live handoff or a server restart, and programs do not re-send.
- The spec's "stop mapping OSC 9;4 after a 7501 report" is not done.

## Parent

[000-map](./000-map.md)
