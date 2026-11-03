---
id: herdr-fork-readable-report-view
title: herdr-sidebar's open report shows the report itself, readably
status: in-progress
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/w1:p1W/a020bfec-075c-4171-b000-41c629bc47db
---

## Question

Raised by the user on 2026-11-03 after trying 059's popup ("this is not entirely readable honestly"), on `popup-menu-059`'s done report.

The report screen shows the herdr message's `summary`: loom's one-line notification to the parent, which carries the report's summary plus a "‖ child <name> [<id>] <state>, last …" row for each of the parent's other children. Those rows are for the parent agent and are noise to the user. The report itself is a markdown file loom keeps (`loom show <node> --json` → `.last.file`, under `~/.local/state/herdr/plugins/loom/lanes/<lane>/inbox/<node>/`), often with sections, code blocks and lists; the herdr message has no `body` today. The header repeats the summary line, and the whole body is bold, one unbroken block.

## Decided with the user (2026-11-03)

- **Show the report, not the notification.** The body is the report file's text with its frontmatter dropped. With no file, fall back to the message summary with loom's "‖ child …" rows removed.
- **Header:** name, the kind's glyph in its colour, kind, age, e.g. `popup-menu-059 ✓ done · 4m`. No summary in it.
- **Body:** normal weight; light markdown: headings bold, inline `code` and fenced blocks dimmed, list bullets and blank lines kept, long lines wrapped at word boundaries (URLs not split mid-word where it can be helped).
- **Unchanged:** scrolling (j/k, ctrl-d/u), and leaving the screen acks the report.

To settle in the work (not with the user): how the sidebar gets the report text without hard-coding loom. It already reaches loom only through `[sidebar.actions]` (`ack`, `close`); a `report` action whose stdout is the body fits that, as would loom putting the body on the herdr message, if delivery to the parent agent stays the summary only.

## Parent

[herdr-fork map](./000-map.md)
