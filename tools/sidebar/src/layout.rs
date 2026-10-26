//! Items to screen lines at a given width: narrowing, the narrow mode, and the
//! click zone under every cell. Pure, so it is tested on fixed-size buffers.

use ratatui::style::{Color, Modifier, Style};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::config::Config;
use crate::glyphs::Meaning;
use crate::model::{AgentItem, HeaderItem, Item};

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
    /// An entry of the open inline menu.
    Menu(usize),
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

fn age_text(seconds: u64) -> String {
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
    for item in items {
        let key = item.key();
        match item {
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

// only the focused row is bold: underline alone is too faint to find
fn name_style(agent: &AgentItem) -> Style {
    let style = if agent.failed {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };
    if agent.focused {
        style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        style
    }
}

fn agent_lines(agent: &AgentItem, width: u16, config: &Config, key: String) -> Vec<Line> {
    let width = width as usize;
    let muted = fg(Some(config.muted));
    let state = config.glyphs.get(agent.meaning);
    let brand = agent.harness.as_deref().map(|h| config.glyphs.brand(h));
    let focus = Target::FocusPane(agent.pane_id.clone());

    let guides = guides_text(agent);
    let mut lead = vec![
        span(" ", Style::default()),
        span(guides.clone(), muted),
        span(state.text.clone(), fg(state.color)),
    ];
    if let Some(brand) = &brand {
        lead.push(span(brand.text.clone(), fg(brand.color)));
        lead.push(span(" ", Style::default()));
    }
    let lead_width: usize = lead.iter().map(|s| s.text.width()).sum();

    let name_style = name_style(agent);

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
            let pad = " ".repeat(state_width);
            let room = width.saturating_sub(indent.width() + pad.width());
            out.push(Line {
                spans: vec![
                    span(indent, muted),
                    span(pad, Style::default()),
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

fn narrow_agent_line(agent: &AgentItem, width: u16, config: &Config, key: String) -> Line {
    let state = config.glyphs.get(agent.meaning);
    let indent = " ".repeat(agent.depth);
    let room = (width as usize).saturating_sub(indent.width() + state.text.width());
    let style = name_style(agent);
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
}
