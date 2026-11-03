//! Items to screen lines at a given width: narrowing, the narrow mode, and the
//! click zone under every cell. Pure, so it is tested on fixed-size buffers.

use ratatui::style::{Color, Modifier, Style};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::config::Config;
use crate::glyphs::Meaning;
use crate::model::{AgentItem, AttentionItem, HeaderItem, Item, LoomMark, Mark};

/// Under this many columns rows are drawn narrow (ADR 0005).
pub const NARROW_BELOW: u16 = 16;

/// What a click on a cell does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    FocusPane(String),
    /// A Source token: focus that agent.
    FocusAgent(String),
    FocusWorkspace(String),
    ToggleSection(String),
    /// Unfold a folded node (its agent id).
    ToggleFold(String),
    NewWorkspace,
    ClearScope,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: Style,
    pub target: Option<Target>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub spans: Vec<Span>,
    /// The item this line belongs to (`Item::key`).
    pub key: String,
    /// The target of cells no span claims.
    pub target: Option<Target>,
}

impl Line {
    #[cfg(test)]
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// The target under column `col`.
    pub fn target_at(&self, col: u16) -> Option<Target> {
        let mut x = 0u16;
        for span in &self.spans {
            let width = span.text.width() as u16;
            if col >= x && col < x + width {
                return span.target.clone().or_else(|| self.target.clone());
            }
            x += width;
        }
        self.target.clone()
    }
}

fn span(text: impl Into<String>, style: Style) -> Span {
    Span {
        text: text.into(),
        style,
        target: None,
    }
}

fn fg(color: Option<Color>) -> Style {
    match color {
        Some(color) => Style::default().fg(color),
        None => Style::default(),
    }
}

/// Cuts `text` to `width` cells, ending in `…` when anything was dropped.
pub fn cut(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

pub fn age_text(seconds: u64) -> String {
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}

pub fn lines(items: &[Item], width: u16, config: &Config) -> Vec<Line> {
    let narrow = width < NARROW_BELOW;
    let mut out = Vec::new();
    let mut in_section = false;
    let mut seen_header = false;
    for item in items {
        let key = item.key();
        let section_item = matches!(item, Item::AttentionHeader(_) | Item::Attention(_));
        if in_section && !section_item {
            // a dead row between the attention section and the trees
            out.push(Line {
                spans: Vec::new(),
                key: "gap".into(),
                target: None,
            });
        }
        in_section = section_item;
        if matches!(item, Item::Header(_)) {
            if seen_header {
                // a dead row between one effort's tree and the next
                out.push(Line {
                    spans: Vec::new(),
                    key: "gap".into(),
                    target: None,
                });
            }
            seen_header = true;
        }
        match item {
            Item::AttentionHeader(count) => {
                let label = if narrow { "!" } else { "attention" };
                let count = format!(" {count}");
                let mut spans = vec![span(
                    cut(label, width as usize),
                    Style::default().add_modifier(Modifier::BOLD),
                )];
                if label.width() + count.width() <= width as usize {
                    spans.push(span(count, fg(Some(config.muted))));
                }
                out.push(Line {
                    spans,
                    key,
                    target: None,
                });
            }
            Item::Attention(row) => out.extend(attention_lines(row, width, narrow, config, key)),
            Item::Scope(label) => out.push(Line {
                spans: vec![span(
                    if narrow {
                        cut(&format!("◎ {label}"), width as usize)
                    } else {
                        cut(&format!("◎ scope: {label}  esc clears"), width as usize)
                    },
                    Style::default().add_modifier(Modifier::BOLD),
                )],
                key,
                target: Some(Target::ClearScope),
            }),
            Item::Search(query) => out.push(Line {
                spans: vec![span(
                    cut(&format!("/{query}"), width as usize),
                    Style::default(),
                )],
                key,
                target: None,
            }),
            Item::Header(header) => out.push(header_line(header, width, narrow, config, key)),
            Item::Agent(agent) => {
                if narrow {
                    out.push(narrow_agent_line(agent, width, config, key));
                } else {
                    out.extend(agent_lines(agent, width, config, key));
                }
            }
            Item::Pane(pane) => {
                let glyph = config.glyphs.get(Meaning::OtherPane);
                let lead = if narrow { "" } else { " " };
                let room = (width as usize).saturating_sub(lead.width() + glyph.text.width());
                out.push(Line {
                    spans: vec![
                        span(lead, Style::default()),
                        span(glyph.text.clone(), fg(Some(config.muted))),
                        span(cut(&pane.name, room), fg(Some(config.muted))),
                    ],
                    key,
                    target: Some(Target::FocusPane(pane.pane_id.clone())),
                });
            }
            Item::New => {
                // a dead row, so a click just under the last agent doesn't open a workspace
                out.push(Line {
                    spans: Vec::new(),
                    key: "gap".into(),
                    target: None,
                });
                out.push(Line {
                    spans: vec![span(
                        if narrow { "+" } else { " + new" },
                        fg(Some(config.muted)),
                    )],
                    key,
                    target: Some(Target::NewWorkspace),
                });
            }
        }
    }
    out
}

fn header_line(
    header: &HeaderItem,
    width: u16,
    narrow: bool,
    config: &Config,
    key: String,
) -> Line {
    let arrow = if header.collapsed { "▸" } else { "▾" };
    let toggle = Target::ToggleSection(header.workspace_id.clone());
    let focus = Target::FocusWorkspace(header.workspace_id.clone());
    let label_style = if header.focused {
        Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default()
    };
    if narrow {
        return Line {
            spans: vec![
                Span {
                    text: arrow.into(),
                    style: Style::default(),
                    target: Some(toggle),
                },
                span(
                    cut(&header.label, (width as usize).saturating_sub(1)),
                    label_style,
                ),
            ],
            key,
            target: Some(focus),
        };
    }
    let counts: Vec<Span> = header
        .counts
        .iter()
        .flat_map(|(meaning, n)| {
            let glyph = config.glyphs.get(*meaning);
            [
                span(" ", Style::default()),
                span(format!("{}{n}", glyph.text), fg(glyph.color)),
            ]
        })
        .collect();
    let counts_width: usize = counts.iter().map(|s| s.text.width()).sum();
    let mut spans = vec![
        Span {
            text: format!("{arrow} "),
            style: Style::default(),
            target: Some(toggle),
        },
        span(header.label.clone(), label_style),
    ];
    if let Some(effort) = &header.effort {
        spans.push(span(format!(" {effort}"), fg(Some(config.muted))));
    }
    let used: usize = spans.iter().map(|s| s.text.width()).sum();
    let width = width as usize;
    if used + counts_width <= width {
        let gap = width - used - counts_width;
        spans.push(span(" ".repeat(gap), Style::default()));
        spans.extend(counts);
    } else {
        // the effort goes before the label is cut; counts go first of all
        if header.effort.is_some() && 2 + header.label.width() <= width {
            spans.pop();
        }
        let label_room = width.saturating_sub(2);
        spans[1].text = cut(&header.label, label_room);
    }
    Line {
        spans,
        key,
        target: Some(focus),
    }
}

fn guides_text(agent: &AgentItem) -> String {
    if agent.depth == 0 {
        return String::new();
    }
    let mut out = String::new();
    for continues in &agent.guides {
        out.push_str(if *continues { "│ " } else { "  " });
    }
    out.push_str(if agent.last { "└ " } else { "├ " });
    out
}

// the focused row is bold and underlined: underline alone is too faint to find
fn name_style(failed: bool, focused: bool, mark: Option<Mark>) -> Style {
    let mut style = if failed {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };
    if focused {
        style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    }
    if mark == Some(Mark::Loud) {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

/// A wide row's first cell: a bar in the state's colour when loud, else blank.
/// Reverse video on the glyph broke on nerd icons, which overflow their cell.
fn edge_span(color: Option<Color>, mark: Option<Mark>) -> Span {
    if mark == Some(Mark::Loud) {
        span("▌", fg(color))
    } else {
        span(" ", Style::default())
    }
}

/// The loom mark after a row's name: coloured while its report is unread, muted once acked.
fn loom_mark_span(mark: Option<LoomMark>, config: &Config) -> Option<Span> {
    let mark = mark?;
    let glyph = config.glyphs.get(mark.meaning);
    let color = if mark.unread {
        glyph.color.or(Some(config.muted))
    } else {
        Some(config.muted)
    };
    Some(span(glyph.text.clone(), fg(color)))
}

fn questions_span(questions: &str, config: &Config) -> Span {
    span(
        questions.to_string(),
        fg(config.glyphs.get(Meaning::BlockedQuestion).color),
    )
}

fn agent_lines(agent: &AgentItem, width: u16, config: &Config, key: String) -> Vec<Line> {
    let width = width as usize;
    let muted = fg(Some(config.muted));
    let state = config.glyphs.get(agent.meaning);
    let brand = agent.harness.as_deref().map(|h| config.glyphs.brand(h));
    let focus = Target::FocusPane(agent.pane_id.clone());

    let guides = guides_text(agent);
    let mut lead = vec![
        edge_span(state.color, agent.mark),
        span(guides.clone(), muted),
        span(state.text.clone(), fg(state.color)),
    ];
    if let Some(brand) = &brand {
        lead.push(span(brand.text.clone(), fg(brand.color)));
        lead.push(span(" ", Style::default()));
    }
    let lead_width: usize = lead.iter().map(|s| s.text.width()).sum();

    let name_style = name_style(agent.failed, agent.focused, agent.mark);

    // (token, span); `name` is always kept and cut last.
    let mut tokens: Vec<(&str, Span)> = Vec::new();
    for token in &config.line1 {
        let piece = match token.as_str() {
            "name" => Some(span(agent.name.clone(), name_style)),
            "effort" => agent.effort.as_ref().map(|e| span(e.clone(), muted)),
            "task" => agent.task.map(|t| {
                let style = if agent.failed {
                    Style::default().fg(Color::Red)
                } else {
                    muted
                };
                span(t, style)
            }),
            "ticket" => agent.ticket.as_ref().map(|t| span(t.clone(), muted)),
            "messages" => (agent.pending > 0).then(|| {
                let glyph = config.glyphs.get(Meaning::Messages);
                span(format!("{}{}", glyph.text, agent.pending), fg(glyph.color))
            }),
            "questions" => agent
                .questions
                .as_deref()
                .map(|q| questions_span(q, config)),
            "source" => agent.source.as_ref().map(|(id, name)| Span {
                text: format!("← {name}"),
                style: muted.add_modifier(Modifier::DIM),
                target: Some(Target::FocusAgent(id.clone())),
            }),
            "age" => agent.age.map(|age| span(age_text(age), muted)),
            _ => None,
        };
        if let Some(piece) = piece {
            tokens.push((token.as_str(), piece));
        }
    }
    if let Some(dot) = loom_mark_span(agent.loom_mark, config) {
        let at = tokens
            .iter()
            .position(|(t, _)| *t == "name")
            .map_or(0, |i| i + 1);
        tokens.insert(at, ("unread", dot));
    }
    if agent.folded && agent.hidden_below > 0 {
        let fold = Span {
            text: format!("▸ +{}", agent.hidden_below),
            style: muted,
            target: agent.agent_id.clone().map(Target::ToggleFold),
        };
        let at = tokens
            .iter()
            .position(|(t, _)| *t == "name")
            .map_or(0, |i| i + 1);
        tokens.insert(at, ("fold", fold));
    }

    let total = |tokens: &[(&str, Span)]| -> usize {
        let body: usize = tokens.iter().map(|(_, s)| s.text.width()).sum();
        lead_width + body + tokens.len().saturating_sub(1)
    };
    // 018: a narrowing row drops age first, then the ticket, then line 2.
    let mut dropped_line2 = false;
    for drop in ["age", "ticket"] {
        if total(&tokens) > width {
            tokens.retain(|(t, _)| *t != drop);
        }
    }
    if total(&tokens) > width {
        dropped_line2 = true;
    }
    while total(&tokens) > width && tokens.len() > 1 {
        let last = tokens.iter().rposition(|(t, _)| *t != "name").unwrap_or(0);
        if tokens[last].0 == "name" {
            break;
        }
        tokens.remove(last);
    }
    if total(&tokens) > width {
        if let Some((_, name)) = tokens.iter_mut().find(|(t, _)| *t == "name") {
            let room = width.saturating_sub(lead_width);
            name.text = cut(&name.text, room);
        }
    }

    let mut spans = lead;
    let age_at = tokens.iter().position(|(t, _)| *t == "age");
    let mut age = age_at.map(|i| tokens.remove(i).1);
    for (index, (_, piece)) in tokens.into_iter().enumerate() {
        if index > 0 {
            spans.push(span(" ", Style::default()));
        }
        spans.push(piece);
    }
    if let Some(age) = age.take() {
        let used: usize = spans.iter().map(|s| s.text.width()).sum();
        let gap = width.saturating_sub(used + age.text.width()).max(1);
        spans.push(span(" ".repeat(gap), Style::default()));
        spans.push(age);
    }
    let mut out = vec![Line {
        spans,
        key: key.clone(),
        target: Some(focus.clone()),
    }];

    if let (false, Some(repo)) = (dropped_line2, &agent.repo) {
        let mut parts: Vec<String> = Vec::new();
        for token in &config.line2 {
            match token.as_str() {
                "branch" => parts.push(format!(
                    "{}{}",
                    config.glyphs.get(Meaning::Branch).text,
                    repo.branch.as_deref().unwrap_or("detached")
                )),
                "ahead_behind" => {
                    let mut ab = String::new();
                    if let Some(ahead) = repo.ahead.filter(|n| *n > 0) {
                        ab.push_str(&format!("↑{ahead}"));
                    }
                    if let Some(behind) = repo.behind.filter(|n| *n > 0) {
                        ab.push_str(&format!("↓{behind}"));
                    }
                    if !ab.is_empty() {
                        parts.push(ab);
                    }
                }
                "dirty" => {
                    if let Some(dirty) = repo.dirty.filter(|n| *n > 0) {
                        parts.push(format!("±{dirty}"));
                    }
                }
                "worktree" => {
                    if let Some(worktree) = &repo.worktree {
                        parts.push(format!(
                            "{}{worktree}",
                            config.glyphs.get(Meaning::Worktree).text
                        ));
                    }
                }
                _ => {}
            }
        }
        if !parts.is_empty() {
            // line 2 sits under the name, past the guides and glyphs
            let indent = format!(" {}", continuation(agent));
            let state_width = state.text.width() + brand.as_ref().map_or(0, |b| b.text.width() + 1);
            // the guide down to this row's children starts under its glyph
            let pad = if agent.has_kids {
                format!("│{}", " ".repeat(state_width.saturating_sub(1)))
            } else {
                " ".repeat(state_width)
            };
            let room = width.saturating_sub(indent.width() + pad.width());
            out.push(Line {
                spans: vec![
                    span(indent, muted),
                    span(pad, muted),
                    span(
                        cut(&parts.join(" "), room),
                        muted.add_modifier(Modifier::DIM),
                    ),
                ],
                key,
                target: Some(focus),
            });
        }
    }
    out
}

/// The guides under a row: its ancestors' columns, then its own if siblings follow.
fn continuation(agent: &AgentItem) -> String {
    if agent.depth == 0 {
        return String::new();
    }
    let mut out = String::new();
    for continues in &agent.guides {
        out.push_str(if *continues { "│ " } else { "  " });
    }
    out.push_str(if agent.last { "  " } else { "│ " });
    out
}

/// An attention row: glyph, name, workspace, `?N` and the time waited, then the
/// reason under the name. Narrow, just the glyph and the name.
fn attention_lines(
    row: &AttentionItem,
    width: u16,
    narrow: bool,
    config: &Config,
    key: String,
) -> Vec<Line> {
    let width = width as usize;
    let muted = fg(Some(config.muted));
    let mark = Some(row.mark);
    let state = config.glyphs.get(row.meaning);
    let focus = Target::FocusPane(row.pane_id.clone());
    let name_style = name_style(false, row.focused, mark);
    let glyph = span(state.text.clone(), fg(state.color));
    if narrow {
        let room = width.saturating_sub(state.text.width());
        let name: String = {
            let mut out = String::new();
            let mut used = 0;
            for ch in row.name.chars() {
                let w = ch.width().unwrap_or(0);
                if used + w > room {
                    break;
                }
                out.push(ch);
                used += w;
            }
            out
        };
        return vec![Line {
            spans: vec![glyph, span(name, name_style)],
            key,
            target: Some(focus),
        }];
    }
    let lead = vec![edge_span(state.color, mark), glyph];
    let lead_width: usize = lead.iter().map(|s| s.text.width()).sum();
    let mut tokens: Vec<(&str, Span)> = vec![("name", span(row.name.clone(), name_style))];
    if let Some(dot) = loom_mark_span(row.loom_mark, config) {
        tokens.push(("unread", dot));
    }
    tokens.push(("workspace", span(row.workspace.clone(), muted)));
    if let Some(questions) = &row.questions {
        tokens.push(("questions", questions_span(questions, config)));
    }
    if let Some(waited) = row.waited {
        tokens.push(("age", span(age_text(waited), muted)));
    }
    let total = |tokens: &[(&str, Span)]| -> usize {
        let body: usize = tokens.iter().map(|(_, s)| s.text.width()).sum();
        lead_width + body + tokens.len().saturating_sub(1)
    };
    for drop in ["age", "workspace", "questions"] {
        if total(&tokens) > width {
            tokens.retain(|(t, _)| *t != drop);
        }
    }
    if total(&tokens) > width {
        let fixed: usize = total(&tokens) - tokens[0].1.text.width();
        tokens[0].1.text = cut(&row.name, width.saturating_sub(fixed));
    }
    let mut spans = lead;
    let age = tokens
        .iter()
        .position(|(t, _)| *t == "age")
        .map(|i| tokens.remove(i).1);
    for (index, (_, piece)) in tokens.into_iter().enumerate() {
        if index > 0 {
            spans.push(span(" ", Style::default()));
        }
        spans.push(piece);
    }
    if let Some(age) = age {
        let used: usize = spans.iter().map(|s| s.text.width()).sum();
        let gap = width.saturating_sub(used + age.text.width()).max(1);
        spans.push(span(" ".repeat(gap), Style::default()));
        spans.push(age);
    }
    let mut out = vec![Line {
        spans,
        key: key.clone(),
        target: Some(focus.clone()),
    }];
    if let Some(reason) = &row.reason {
        // under the name, past the glyph
        let indent = " ".repeat(lead_width);
        out.push(Line {
            spans: vec![
                span(indent, Style::default()),
                span(cut(reason, width.saturating_sub(lead_width)), muted),
            ],
            key,
            target: Some(focus),
        });
    }
    out
}

fn narrow_agent_line(agent: &AgentItem, width: u16, config: &Config, key: String) -> Line {
    let state = config.glyphs.get(agent.meaning);
    let indent = " ".repeat(agent.depth);
    let room = (width as usize).saturating_sub(indent.width() + state.text.width());
    let style = name_style(agent.failed, agent.focused, agent.mark);
    let name: String = {
        let mut out = String::new();
        let mut used = 0;
        for ch in agent.name.chars() {
            let w = ch.width().unwrap_or(0);
            if used + w > room {
                break;
            }
            out.push(ch);
            used += w;
        }
        out
    };
    Line {
        spans: vec![
            span(indent, Style::default()),
            span(state.text.clone(), fg(state.color)),
            span(name, style),
        ],
        key,
        target: Some(Target::FocusPane(agent.pane_id.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::from_table;
    use crate::model::fixtures::snapshot;
    use crate::model::{build, Extras, Inputs, UiState};

    fn unicode() -> Config {
        from_table(&"glyphs = \"unicode\"".parse().unwrap())
    }

    fn render(width: u16, extras: &Extras) -> Vec<Line> {
        let snapshot = snapshot();
        let items = build(&Inputs {
            snapshot: &snapshot,
            extras,
            ui: &UiState::default(),
            search: None,
            attention: true,
        });
        lines(&items, width, &unicode())
    }

    #[test]
    fn wide_rows_take_two_lines_with_the_tree_and_age_flush_right() {
        let mut extras = Extras::default();
        extras.ages.insert("w1:p1".into(), 125);
        let lines = render(40, &extras);
        let text: Vec<String> = lines.iter().map(Line::text).collect();
        assert!(text[0].starts_with("▾ dogfood pt:herdr-fork"), "{text:?}");
        assert_eq!(text[1].width(), 40);
        assert!(text[1].starts_with(" ✓✻ cockpit pt:herdr-fork"), "{text:?}");
        assert!(text[1].ends_with(" 2m"), "{text:?}");
        assert!(text[2].contains("⎇master ↑1 ±2"), "{text:?}");
        // the cockpit's children hang off a guide that runs through its line 2
        assert!(text[2].starts_with(" │  ⎇"), "{text:?}");
        assert!(
            text.iter().any(|l| l.starts_with(" ├ ●✻ alpha fork-030")),
            "{text:?}"
        );
        assert!(
            text.iter().any(|l| l.starts_with(" │ └ ○✻ alpha-one")),
            "{text:?}"
        );
        assert!(text.iter().any(|l| l.starts_with(" └ ●✻ beta")), "{text:?}");
        assert!(
            text.iter().any(|l| l.contains("remote ← alpha")),
            "{text:?}"
        );
        assert_eq!(text.last().unwrap(), " + new");
    }

    #[test]
    fn a_blank_row_separates_one_effort_from_the_next() {
        let header = |id: &str| {
            Item::Header(HeaderItem {
                workspace_id: id.into(),
                label: id.into(),
                effort: None,
                collapsed: true,
                focused: false,
                counts: Vec::new(),
            })
        };
        let lines = lines(
            &[header("one"), header("two"), header("three")],
            30,
            &unicode(),
        );
        let rows: Vec<(String, String)> = lines.iter().map(|l| (l.key.clone(), l.text())).collect();
        let keys: Vec<&str> = rows.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(
            keys,
            ["ws:one", "gap", "ws:two", "gap", "ws:three"],
            "{rows:?}"
        );
        assert!(rows
            .iter()
            .filter(|(key, _)| key == "gap")
            .all(|(_, text)| text.is_empty()));
    }

    #[test]
    fn narrowing_drops_age_then_ticket_then_line_two() {
        let mut extras = Extras::default();
        extras.ages.insert("w1:p3".into(), 5);
        let at = |width: u16| -> Vec<String> {
            render(width, &extras)
                .iter()
                .filter(|l| l.key == "pane:w1:p3")
                .map(Line::text)
                .collect()
        };
        assert!(at(40)[0].ends_with("5s"));
        let without_age = at(21);
        assert!(!without_age[0].ends_with("5s") && without_age[0].contains("fork-030"));
        let without_ticket = at(18);
        assert!(
            !without_ticket[0].contains("fork-030"),
            "{without_ticket:?}"
        );
        // alpha has no repo, so check the root's second line instead
        let root = |width: u16| {
            render(width, &Extras::default())
                .iter()
                .filter(|l| l.key == "pane:w1:p1")
                .count()
        };
        assert_eq!(root(40), 2);
        assert_eq!(root(16), 1);
    }

    #[test]
    fn only_the_focused_workspace_and_agent_are_bold() {
        let mut snapshot = snapshot();
        snapshot.workspaces[0].focused = true;
        for pane in &mut snapshot.panes {
            pane.focused = pane.pane_id == "w1:p3";
        }
        let extras = Extras::default();
        let items = build(&Inputs {
            snapshot: &snapshot,
            extras: &extras,
            ui: &UiState::default(),
            search: None,
            attention: true,
        });
        let focused: Vec<String> = items
            .iter()
            .filter(|item| match item {
                Item::Header(h) => h.focused,
                Item::Agent(a) => a.focused,
                _ => false,
            })
            .map(Item::key)
            .collect();
        assert_eq!(focused, ["ws:w1", "pane:w1:p3"]);
        for width in [10, 40] {
            let mut bold: Vec<String> = lines(&items, width, &unicode())
                .into_iter()
                .filter(|l| {
                    l.spans
                        .iter()
                        .any(|s| s.style.add_modifier.contains(Modifier::BOLD))
                })
                .map(|l| l.key)
                .collect();
            // a wide agent row spans two lines under one key
            bold.dedup();
            assert_eq!(bold, focused, "width {width}");
        }
    }

    #[test]
    fn narrow_mode_draws_one_cell_per_depth_the_glyph_and_the_name() {
        let lines = render(10, &Extras::default());
        let text: Vec<String> = lines.iter().map(Line::text).collect();
        assert_eq!(text[0], "▾dogfood");
        assert_eq!(text[1], "✓cockpit");
        assert_eq!(text[2], " ●alpha");
        assert_eq!(text[3], "  ○alpha-o");
        assert!(text.iter().all(|line| line.width() <= 10), "{text:?}");
        assert_eq!(text.last().unwrap(), "+");
    }

    #[test]
    fn click_zones_split_headers_and_source_tokens() {
        let lines = render(40, &Extras::default());
        assert_eq!(
            lines[0].target_at(0),
            Some(Target::ToggleSection("w1".into()))
        );
        assert_eq!(
            lines[0].target_at(4),
            Some(Target::FocusWorkspace("w1".into()))
        );
        let remote = lines.iter().find(|l| l.key == "pane:w2:p1").unwrap();
        let at = remote.text().find('←').unwrap();
        let col = remote.text()[..at].width() as u16;
        assert_eq!(remote.target_at(col), Some(Target::FocusAgent("a".into())));
        assert_eq!(remote.target_at(2), Some(Target::FocusPane("w2:p1".into())));
        // past the end of the text still focuses the row's pane
        assert_eq!(
            remote.target_at(39),
            Some(Target::FocusPane("w2:p1".into()))
        );
        assert_eq!(
            lines.last().unwrap().target_at(1),
            Some(Target::NewWorkspace)
        );
    }

    #[test]
    fn cut_ends_in_an_ellipsis() {
        assert_eq!(cut("abcdef", 4), "abc…");
        assert_eq!(cut("abc", 4), "abc");
        assert_eq!(cut("abc", 0), "");
    }

    fn render_attention(width: u16, extras: &Extras) -> Vec<Line> {
        let snapshot = crate::model::fixtures::attention_snapshot();
        let items = build(&Inputs {
            snapshot: &snapshot,
            extras,
            ui: &UiState::default(),
            search: None,
            attention: true,
        });
        lines(&items, width, &unicode())
    }

    fn span_with<'a>(line: &'a Line, text: &str) -> &'a Span {
        line.spans
            .iter()
            .find(|s| s.text == text)
            .unwrap_or_else(|| panic!("no span {text:?} in {:?}", line.text()))
    }

    #[test]
    fn the_attention_section_heads_the_list_two_lines_a_row() {
        let mut extras = Extras::default();
        extras.waited.insert("w1:p3".into(), 125);
        extras
            .reports
            .insert("a1".into(), "shipped the parser".into());
        let lines = render_attention(40, &extras);
        let text: Vec<String> = lines.iter().map(Line::text).collect();
        assert_eq!(text[0], "attention 4");
        assert_eq!(text[1].width(), 40, "{text:?}");
        assert!(text[1].starts_with("▌●alpha ? dogfood ?2"), "{text:?}");
        assert!(text[1].ends_with(" 2m"), "{text:?}");
        assert_eq!(text[2], "  which db?");
        assert!(text[3].starts_with("▌!beta dogfood"), "{text:?}");
        assert_eq!(text[4], "  permission");
        assert!(text[5].starts_with(" ✓cockpit dogfood"), "{text:?}");
        assert_eq!(text[6], "  merge it");
        assert!(text[7].starts_with(" ○alpha-one ✓ dogfood"), "{text:?}");
        assert_eq!(text[8], "  done: shipped the parser");
        // a dead gap, then the trees as before
        assert_eq!(text[9], "");
        assert_eq!(lines[9].target, None);
        assert!(text[10].starts_with("▾ dogfood"), "{text:?}");
        assert_eq!(lines[1].key, "attention:w1:p3");
        assert_eq!(lines[2].target, Some(Target::FocusPane("w1:p3".into())));
    }

    #[test]
    fn loud_bars_the_edge_and_bolds_the_name_steady_does_not() {
        let lines = render_attention(40, &Extras::default());
        let alpha = &lines[1];
        assert_eq!(alpha.spans[0].text, "▌");
        assert_eq!(alpha.spans[0].style.fg, span_with(alpha, "●").style.fg);
        assert!(!span_with(alpha, "●")
            .style
            .add_modifier
            .contains(Modifier::REVERSED));
        assert!(span_with(alpha, "alpha")
            .style
            .add_modifier
            .contains(Modifier::BOLD));
        let cockpit = &lines[5];
        assert_eq!(cockpit.spans[0].text, " ");
        assert!(!span_with(cockpit, "cockpit")
            .style
            .add_modifier
            .contains(Modifier::BOLD));
        // the same marks in the tree
        let tree_alpha = lines.iter().find(|l| l.key == "pane:w1:p3").unwrap();
        assert_eq!(tree_alpha.spans[0].text, "▌");
        assert!(
            tree_alpha.text().ends_with("fork-030 ?2"),
            "{}",
            tree_alpha.text()
        );
    }

    #[test]
    fn the_loom_mark_is_coloured_while_unread_and_muted_once_read() {
        let lines = render_attention(40, &Extras::default());
        for key in ["attention:w1:p5", "pane:w1:p5"] {
            let line = lines.iter().find(|l| l.key == key).unwrap();
            assert_eq!(span_with(line, "✓").style.fg, Some(Color::Cyan), "{key}");
        }
        assert!(lines
            .iter()
            .find(|l| l.key == "pane:w1:p5")
            .unwrap()
            .text()
            .contains("alpha-one ✓"));
        // a loud row carries its unread kind too
        let alpha = lines.iter().find(|l| l.key == "attention:w1:p3").unwrap();
        assert_eq!(span_with(alpha, "?").style.fg, Some(Color::Magenta));
        let config = unicode();
        let read = LoomMark {
            meaning: Meaning::UnreadDone,
            unread: false,
        };
        let span = loom_mark_span(Some(read), &config).unwrap();
        assert_eq!(
            (span.text.as_str(), span.style.fg),
            ("✓", Some(config.muted))
        );
    }

    #[test]
    fn narrowing_an_attention_row_drops_age_then_workspace_then_cuts_the_name() {
        let mut extras = Extras::default();
        extras.waited.insert("w1:p5".into(), 5);
        let row = |width: u16| -> String {
            render_attention(width, &extras)
                .iter()
                .find(|l| l.key == "attention:w1:p5")
                .unwrap()
                .text()
        };
        assert!(row(30).ends_with("5s"));
        assert_eq!(row(22), " ○alpha-one ✓ dogfood");
        assert_eq!(row(16), " ○alpha-one ✓");
        assert_eq!(row(9), "○alpha-on");
        assert!(render_attention(9, &extras)
            .iter()
            .all(|l| l.text().width() <= 9));
        assert_eq!(render_attention(9, &extras)[0].text(), "! 4");
    }
}
