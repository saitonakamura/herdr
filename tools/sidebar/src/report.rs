//! The report screen's text (060). The report is rebuilt from herdr's message:
//! loom sends its summary as the message summary, after `<kind>: ` and before
//! the ` ‖ ` rows it appends for the parent agent, and its `--body-file` as the
//! body, so the two give the report file's text without reading loom's files.
//! It is drawn as light markdown, wrapped at word boundaries.

use ratatui::style::{Modifier, Style};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// loom's report kinds, which lead its summaries as `<kind>: `.
const KINDS: &[&str] = &["done", "failed", "blocked", "asking", "handoff", "progress"];

/// The loom kind a summary leads with.
pub fn summary_kind(summary: &str) -> Option<&'static str> {
    let head = summary.trim_start();
    KINDS.iter().copied().find(|kind| {
        head.strip_prefix(kind)
            .is_some_and(|rest| rest.starts_with(':'))
    })
}

/// The summary as the report says it: no kind prefix, no rows for the parent.
pub fn summary_text(summary: &str) -> &str {
    let mut summary = summary.trim();
    if let Some(at) = summary.find(" ‖ ") {
        summary = &summary[..at];
    }
    if let Some(kind) = summary_kind(summary) {
        summary = summary[kind.len() + 1..].trim_start();
    }
    summary.trim_end()
}

/// The report: its summary, then its body after a blank line, as loom's file has them.
pub fn text(summary: &str, body: Option<&str>) -> String {
    let summary = summary_text(summary);
    match body.map(str::trim_end).filter(|b| !b.trim().is_empty()) {
        Some(body) => format!("{summary}\n\n{body}"),
        None => summary.to_string(),
    }
}

/// One drawn line: text runs and their styles.
pub type Spans = Vec<(String, Style)>;

type Chars = Vec<(char, Style)>;

/// Light markdown at `width` columns: headings bold, inline code and fenced
/// blocks dimmed, list items with a hanging indent, blank lines kept.
pub fn render(text: &str, width: usize) -> Vec<Spans> {
    let width = width.max(4);
    let plain = Style::default();
    let bold = plain.add_modifier(Modifier::BOLD);
    let dim = plain.add_modifier(Modifier::DIM);
    let mut out = Vec::new();
    // the fence run that opened the current code block
    let mut fence: Option<String> = None;
    for raw in text.lines() {
        let line = raw.replace('\t', "    ");
        let trimmed = line.trim_start();
        if let Some(open) = &fence {
            if trimmed
                .strip_prefix(open.as_str())
                .is_some_and(|rest| rest.trim().is_empty())
            {
                fence = None;
            } else {
                out.extend(hard_wrap(&line, width, dim));
            }
            continue;
        }
        if let Some(open) = fence_run(trimmed) {
            fence = Some(open);
            continue;
        }
        if trimmed.is_empty() {
            out.push(Vec::new());
            continue;
        }
        let indent = line.len() - trimmed.len();
        if let Some(title) = heading(trimmed) {
            out.extend(wrap(inline(title, bold), Vec::new(), 0, width));
            continue;
        }
        let (marker, rest) = bullet(trimmed);
        let mut prefix: Chars = " ".repeat(indent).chars().map(|c| (c, plain)).collect();
        prefix.extend(marker.chars().map(|c| (c, plain)));
        let hang = indent + marker.width();
        out.extend(wrap(inline(rest, plain), prefix, hang, width));
    }
    out
}

/// A code fence opener, ``` or ~~~ (three or more), and the run that closes it.
fn fence_run(line: &str) -> Option<String> {
    let first = line.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run: String = line.chars().take_while(|c| *c == first).collect();
    (run.len() >= 3).then_some(run)
}

fn heading(line: &str) -> Option<&str> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let title = line[hashes..].strip_prefix(' ')?;
    Some(title.trim_end_matches('#').trim())
}

/// A list item's marker with its trailing space (`- `, `* `, `+ `, `1. `, `2) `), and the rest.
fn bullet(line: &str) -> (&str, &str) {
    for marker in ["- ", "* ", "+ "] {
        if line.starts_with(marker) {
            return line.split_at(marker.len());
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if (1..=3).contains(&digits) {
        let after = &line[digits..];
        if after.starts_with(". ") || after.starts_with(") ") {
            return line.split_at(digits + 2);
        }
    }
    ("", line)
}

/// Inline markup: `code` dims and **bold** bolds, markers dropped; an unpaired one stays as typed.
fn inline(text: &str, base: Style) -> Chars {
    let mut out = Vec::new();
    for (code, part) in paired(text, "`") {
        if code {
            let dim = base.add_modifier(Modifier::DIM);
            out.extend(part.chars().map(|c| (c, dim)));
            continue;
        }
        for (strong, part) in paired(&part, "**") {
            let style = if strong {
                base.add_modifier(Modifier::BOLD)
            } else {
                base
            };
            out.extend(part.chars().map(|c| (c, style)));
        }
    }
    out
}

/// Splits on `marker`, marking the parts between a pair; an unpaired last marker is kept in its text.
fn paired(text: &str, marker: &str) -> Vec<(bool, String)> {
    let parts: Vec<&str> = text.split(marker).collect();
    let markers = parts.len() - 1;
    parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            if i % 2 == 1 && i < markers {
                (true, part.to_string())
            } else if i % 2 == 1 {
                (false, format!("{marker}{part}"))
            } else {
                (false, part.to_string())
            }
        })
        .filter(|(_, part)| !part.is_empty())
        .collect()
}

/// Greedy word wrap: `prefix` starts the first line, later lines indent by `hang`;
/// only a word wider than a whole line is cut.
fn wrap(chars: Chars, prefix: Chars, hang: usize, width: usize) -> Vec<Spans> {
    let hang = if hang < width / 2 { hang } else { 0 };
    let mut words: Vec<Chars> = Vec::new();
    let mut word = Vec::new();
    for (c, style) in chars {
        if c.is_whitespace() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push((c, style));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    let blank = || -> Chars { vec![(' ', Style::default()); hang] };
    let mut out = Vec::new();
    let mut used = prefix
        .iter()
        .map(|(c, _)| c.width().unwrap_or(0))
        .sum::<usize>();
    let mut line = prefix;
    let mut fresh = true;
    for word in words {
        let w: usize = word.iter().map(|(c, _)| c.width().unwrap_or(0)).sum();
        if !fresh && used + 1 + w > width {
            out.push(spans(std::mem::replace(&mut line, blank())));
            used = hang;
            fresh = true;
        }
        if !fresh {
            // a space inside a styled run keeps its style, so a heading or code span stays one run
            let before = line.last().map(|(_, s)| *s);
            let after = word.first().map(|(_, s)| *s);
            let style = before.filter(|b| Some(*b) == after).unwrap_or_default();
            line.push((' ', style));
            used += 1;
        }
        if used + w <= width {
            line.extend(word);
            used += w;
        } else {
            for (c, style) in word {
                let cw = c.width().unwrap_or(0);
                if used + cw > width {
                    out.push(spans(std::mem::replace(&mut line, blank())));
                    used = hang;
                }
                line.push((c, style));
                used += cw;
            }
        }
        fresh = false;
    }
    out.push(spans(line));
    out
}

/// A code line: indented two columns and cut at the width, never reflowed.
fn hard_wrap(line: &str, width: usize, style: Style) -> Vec<Spans> {
    let room = width.saturating_sub(2).max(1);
    let mut out = Vec::new();
    let mut chunk = String::new();
    for c in line.trim_end().chars() {
        if chunk.width() + c.width().unwrap_or(0) > room {
            out.push(vec![(format!("  {}", std::mem::take(&mut chunk)), style)]);
        }
        chunk.push(c);
    }
    out.push(vec![(format!("  {chunk}"), style)]);
    out
}

fn spans(chars: Chars) -> Spans {
    let mut out: Spans = Vec::new();
    for (c, style) in chars {
        match out.last_mut() {
            Some((text, last)) if *last == style => text.push(c),
            _ => out.push((c.to_string(), style)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const MESSAGE_LONG: &str = include_str!("../tests/fixtures/reports/message-119.json");
    const FILE_LONG: &str = include_str!("../tests/fixtures/reports/004127b1-done.md");
    const MESSAGE_SHORT: &str = include_str!("../tests/fixtures/reports/message-123.json");
    const FILE_SHORT: &str = include_str!("../tests/fixtures/reports/15c8c116-done.md");

    /// loom's report file without its frontmatter: what the screen should show.
    fn file_text(file: &str) -> String {
        let rest = file.strip_prefix("---\n").expect("frontmatter");
        let end = rest.find("\n---\n").expect("frontmatter end");
        rest[end + 5..].trim().to_string()
    }

    fn message_text(json: &str) -> String {
        let m: Value = serde_json::from_str(json).unwrap();
        text(m["summary"].as_str().unwrap(), m["body"].as_str())
    }

    fn plain_lines(lines: &[Spans]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|(t, _)| t.as_str()).collect::<String>())
            .collect()
    }

    fn style_of(lines: &[Spans], needle: &str) -> Style {
        lines
            .iter()
            .flatten()
            .find(|(t, _)| t.contains(needle))
            .map(|(_, s)| *s)
            .unwrap_or_else(|| panic!("no span with {needle:?}"))
    }

    #[test]
    fn the_message_gives_the_report_files_text() {
        // a long sectioned report sent with --body-file, and a one-paragraph one
        assert_eq!(message_text(MESSAGE_LONG), file_text(FILE_LONG));
        assert_eq!(message_text(MESSAGE_SHORT), file_text(FILE_SHORT));
        // the rows loom appends for the parent agent are gone
        assert!(!message_text(MESSAGE_LONG).contains('‖'));
        let m: Value = serde_json::from_str(MESSAGE_LONG).unwrap();
        assert!(m["summary"].as_str().unwrap().contains(" ‖ child "));
    }

    #[test]
    fn the_summary_loses_the_kind_and_the_parents_rows() {
        assert_eq!(
            summary_text("done: built it ‖ child x [1] running, no report yet ‖ [loom] 1 ask"),
            "built it"
        );
        assert_eq!(summary_kind("handoff: steps left"), Some("handoff"));
        // a hand-sent message keeps what it says
        assert_eq!(summary_text("done-ish: fine"), "done-ish: fine");
        assert_eq!(summary_kind("donezo: no"), None);
        assert_eq!(text("failed: x", Some("  \n")), "x");
    }

    #[test]
    fn headings_bold_code_dims_and_fences_go() {
        let md = "# Title\n\nsome `code` here and **strong**\n\n```sh\njust check\n```\nafter";
        let lines = render(md, 40);
        assert_eq!(
            plain_lines(&lines),
            [
                "Title",
                "",
                "some code here and strong",
                "",
                "  just check",
                "after"
            ]
        );
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let dim = Style::default().add_modifier(Modifier::DIM);
        assert_eq!(style_of(&lines, "Title"), bold);
        assert_eq!(style_of(&lines, "some"), Style::default());
        assert_eq!(style_of(&lines, "code"), dim);
        assert_eq!(style_of(&lines, "strong"), bold);
        assert_eq!(style_of(&lines, "just check"), dim);
        // an unpaired backtick stays as typed
        assert_eq!(plain_lines(&render("a `b", 10)), ["a `b"]);
        assert_eq!(plain_lines(&render("a `", 10)), ["a `"]);
    }

    #[test]
    fn list_items_keep_their_bullet_and_hang() {
        let lines = render(
            "- one two three four\n  - nested item\n12. numbered thing",
            12,
        );
        assert_eq!(
            plain_lines(&lines),
            [
                "- one two",
                "  three four",
                "  - nested",
                "    item",
                "12. numbered",
                "    thing"
            ]
        );
    }

    #[test]
    fn wrapping_keeps_urls_whole_where_it_can() {
        let url = "https://github.com/saitonakamura/herdr/pull/53";
        let lines = plain_lines(&render(&format!("PR {url} merged"), 47));
        assert_eq!(lines, ["PR", url, "merged"]);
        // only a word wider than the line is cut
        let lines = plain_lines(&render(url, 20));
        assert_eq!(lines.concat(), url);
        assert!(lines.iter().all(|l| l.width() <= 20));
    }

    #[test]
    fn code_lines_are_cut_not_reflowed() {
        let lines = plain_lines(&render("```\nlet a =   1; let b = 2;\n```", 12));
        assert_eq!(lines, ["  let a =   ", "  1; let b =", "   2;"]);
    }

    #[test]
    fn the_real_report_renders_within_the_width() {
        let lines = render(&message_text(MESSAGE_LONG), 48);
        let text = plain_lines(&lines);
        assert!(text.iter().all(|l| l.width() <= 48), "{text:#?}");
        assert!(text.contains(&"055 test user dirs — implementation report".to_string()));
        assert!(!text.iter().any(|l| l.starts_with('#') || l.contains("```")));
        assert_eq!(
            style_of(&lines, "Working tree"),
            Style::default().add_modifier(Modifier::BOLD)
        );
    }
}
