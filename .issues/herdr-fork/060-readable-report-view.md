---
id: herdr-fork-readable-report-view
title: herdr-sidebar's open report shows the report itself, readably
status: closed
labels:
  - wayfinder:task
parent: herdr-fork-map
blocked_by:
assignee: saitonakamura
claimed_by: macos/a020bfec-075c-4171-b000-41c629bc47db
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

## Resolution

Done in [PR #54](https://github.com/saitonakamura/herdr/pull/54) (a5fcfab5, merge d9892435). It touches only tools/sidebar and FORK.md, and there is no dotfiles branch because nothing there changes.

- **Source of the text: the herdr message, not loom's files.** loom already sends a report's `--body-file` as the herdr message body (`herdr_send … --body-file`), and herdr types only the summary line and a `herdr message show` pointer into the recipient. So the summary without `<kind>: ` and the ` ‖ ` rows, a blank line, then the body, is byte for byte the report file minus its frontmatter. A unit test checks this against two real reports (004127b1's sectioned done report, message 119; 15c8c116's one paragraph, message 123), copied into `tools/sidebar/tests/fixtures/reports/`. No `[sidebar.actions] report` was added: it would have needed a `loom show --json | jq | cat` pipeline in the dotfiles for the same text, and loom's `last_report` can disagree with herdr's latest message. A report without a body falls back to its summary minus the rows through the same code. With stock dotfiles or 0.9.4 there is no herdr message, so there is no open report entry, as before.
- **Screen.** The header is name, glyph in its colour, kind, then a dim age (`popup-menu-059 ✓ done · 4m`). The body is normal weight, in light markdown (`tools/sidebar/src/report.rs`): headings bold with the `#`s dropped, inline code and fenced blocks dimmed (fence lines dropped, code cut rather than reflowed), `**bold**`, bullets with a hanging indent, blank lines kept. Lines wrap at word boundaries, and a word is cut only when it is wider than the line. Scrolling and ack-on-leave are unchanged. The menu's first-line view drops the `‖` rows too.
- **Verified** live in a throwaway session (captures in the PR). CI is green on all three OSes. The local failures were the known ones: cross_area, api_ping done status, the macOS chmod ACL test, `tag.gpgsign` vs lightweight fixture tags, and no Windows SDK.

## Parent

[herdr-fork map](./000-map.md)
