//! The glyph table (018's resolution): what each meaning draws, and in which colour.

use ratatui::style::Color;

use crate::api::{AgentStatus, BlockedReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Meaning {
    Idle,
    Working,
    Awaiting,
    Done,
    BlockedQuestion,
    BlockedPermission,
    BlockedForm,
    BlockedInterrupted,
    Blocked,
    Unknown,
    OtherPane,
    Messages,
    Branch,
    Worktree,
    /// The mark after a quiet row's name, one glyph and colour per report kind.
    UnreadDone,
    UnreadFailed,
    UnreadBlocked,
    UnreadAsking,
    UnreadHandoff,
    Unread,
}

impl Meaning {
    pub const ALL: [Meaning; 20] = [
        Meaning::Idle,
        Meaning::Working,
        Meaning::Awaiting,
        Meaning::Done,
        Meaning::BlockedQuestion,
        Meaning::BlockedPermission,
        Meaning::BlockedForm,
        Meaning::BlockedInterrupted,
        Meaning::Blocked,
        Meaning::Unknown,
        Meaning::OtherPane,
        Meaning::Messages,
        Meaning::Branch,
        Meaning::Worktree,
        Meaning::UnreadDone,
        Meaning::UnreadFailed,
        Meaning::UnreadBlocked,
        Meaning::UnreadAsking,
        Meaning::UnreadHandoff,
        Meaning::Unread,
    ];

    /// The key in `[sidebar.glyph]` and `[sidebar.colors]`.
    pub fn key(self) -> &'static str {
        match self {
            Meaning::Idle => "idle",
            Meaning::Working => "working",
            Meaning::Awaiting => "awaiting",
            Meaning::Done => "done",
            Meaning::BlockedQuestion => "blocked_question",
            Meaning::BlockedPermission => "blocked_permission",
            Meaning::BlockedForm => "blocked_form",
            Meaning::BlockedInterrupted => "blocked_interrupted",
            Meaning::Blocked => "blocked",
            Meaning::Unknown => "unknown",
            Meaning::OtherPane => "pane",
            Meaning::Messages => "messages",
            Meaning::Branch => "branch",
            Meaning::Worktree => "worktree",
            Meaning::UnreadDone => "unread_done",
            Meaning::UnreadFailed => "unread_failed",
            Meaning::UnreadBlocked => "unread_blocked",
            Meaning::UnreadAsking => "unread_asking",
            Meaning::UnreadHandoff => "unread_handoff",
            Meaning::Unread => "unread",
        }
    }

    /// The dot for an unread report of this kind (loom's report kinds).
    pub fn for_unread(kind: &str) -> Meaning {
        match kind {
            "done" => Meaning::UnreadDone,
            "failed" => Meaning::UnreadFailed,
            "blocked" => Meaning::UnreadBlocked,
            "asking" => Meaning::UnreadAsking,
            "handoff" => Meaning::UnreadHandoff,
            _ => Meaning::Unread,
        }
    }

    fn is_unread(self) -> bool {
        matches!(
            self,
            Meaning::UnreadDone
                | Meaning::UnreadFailed
                | Meaning::UnreadBlocked
                | Meaning::UnreadAsking
                | Meaning::UnreadHandoff
                | Meaning::Unread
        )
    }

    pub fn from_key(key: &str) -> Option<Meaning> {
        Meaning::ALL
            .into_iter()
            .find(|meaning| meaning.key() == key)
    }

    pub fn for_state(status: AgentStatus, reason: Option<BlockedReason>) -> Meaning {
        match status {
            AgentStatus::Idle => Meaning::Idle,
            AgentStatus::Working => Meaning::Working,
            AgentStatus::Awaiting => Meaning::Awaiting,
            AgentStatus::Done => Meaning::Done,
            AgentStatus::Blocked => match reason {
                Some(BlockedReason::Question) => Meaning::BlockedQuestion,
                Some(BlockedReason::Permission) => Meaning::BlockedPermission,
                Some(BlockedReason::Form) => Meaning::BlockedForm,
                Some(BlockedReason::Interrupted) => Meaning::BlockedInterrupted,
                Some(BlockedReason::Unknown) | None => Meaning::Blocked,
            },
            AgentStatus::Unknown => Meaning::Unknown,
        }
    }

    fn nerd(self) -> &'static str {
        match self {
            Meaning::Idle => "\u{f10c}",
            Meaning::Working => "\u{f111}",
            Meaning::Awaiting => "\u{f252}",
            Meaning::Done => "\u{f058}",
            Meaning::BlockedQuestion => "\u{f059}",
            Meaning::BlockedPermission => "\u{f071}",
            Meaning::BlockedForm => "\u{f044}",
            Meaning::BlockedInterrupted => "\u{f28c}",
            Meaning::Blocked => "\u{f111}",
            Meaning::Unknown => "\u{f10c}",
            Meaning::OtherPane => "\u{f120}",
            Meaning::Messages => "\u{f0e0}",
            Meaning::Branch => "\u{e725}",
            Meaning::Worktree => "\u{f126}",
            Meaning::UnreadDone => "\u{f00c}",
            Meaning::UnreadFailed => "\u{f00d}",
            Meaning::UnreadBlocked => "\u{f062}",
            Meaning::UnreadAsking => "\u{f128}",
            Meaning::UnreadHandoff => "\u{f0a4}",
            _ => "•",
        }
    }

    fn unicode(self) -> &'static str {
        match self {
            Meaning::Idle => "○",
            Meaning::Working => "●",
            Meaning::Awaiting => "◔",
            Meaning::Done => "✓",
            Meaning::BlockedQuestion => "?",
            Meaning::BlockedPermission => "!",
            Meaning::BlockedForm => "✎",
            Meaning::BlockedInterrupted => "‖",
            Meaning::Blocked => "●",
            Meaning::Unknown => "·",
            Meaning::OtherPane => "›",
            Meaning::Messages => "✉",
            Meaning::Branch => "⎇",
            Meaning::Worktree => "⑂",
            Meaning::UnreadDone => "✓",
            Meaning::UnreadFailed => "✗",
            Meaning::UnreadBlocked => "↑",
            Meaning::UnreadAsking => "?",
            Meaning::UnreadHandoff => "→",
            _ => "•",
        }
    }

    fn color(self) -> Option<Color> {
        match self {
            Meaning::Idle => Some(Color::Green),
            Meaning::Working => Some(Color::Yellow),
            Meaning::Awaiting => Some(Color::Blue),
            Meaning::Done => Some(Color::Cyan),
            // Both wait on the user to decide how to go on.
            Meaning::BlockedQuestion | Meaning::BlockedInterrupted => Some(Color::Magenta),
            Meaning::BlockedPermission | Meaning::BlockedForm | Meaning::Blocked => {
                Some(Color::Red)
            }
            Meaning::Unknown => Some(Color::DarkGray),
            Meaning::UnreadDone => Some(Color::Cyan),
            Meaning::UnreadFailed => Some(Color::Red),
            Meaning::UnreadBlocked | Meaning::UnreadAsking => Some(Color::Magenta),
            Meaning::UnreadHandoff => Some(Color::Yellow),
            Meaning::OtherPane
            | Meaning::Messages
            | Meaning::Branch
            | Meaning::Worktree
            | Meaning::Unread => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlyphSet {
    #[default]
    Nerd,
    Unicode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    /// What is drawn, padding included.
    pub text: String,
    pub color: Option<Color>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Glyphs {
    pub set: GlyphSet,
    table: Vec<(Meaning, Glyph)>,
    brands: Vec<(String, Glyph)>,
}

impl Glyphs {
    pub fn new(
        set: GlyphSet,
        glyph_overrides: &[(Meaning, String)],
        color_overrides: &[(Meaning, Color)],
    ) -> Self {
        let table = Meaning::ALL
            .into_iter()
            .map(|meaning| {
                let base = match glyph_overrides.iter().find(|(m, _)| *m == meaning) {
                    Some((_, text)) => text.clone(),
                    None => match set {
                        GlyphSet::Nerd => meaning.nerd().to_string(),
                        GlyphSet::Unicode => meaning.unicode().to_string(),
                    },
                };
                let color = color_overrides
                    .iter()
                    .find(|(m, _)| *m == meaning)
                    .map(|(_, color)| *color)
                    .or_else(|| meaning.color());
                // the dot is a plain character in either set, so kitty draws it one cell wide
                let text = if meaning.is_unread() {
                    base
                } else {
                    pad(set, &base)
                };
                (meaning, Glyph { text, color })
            })
            .collect();
        let brands = [
            ("claude", "\u{ec82}", "✻", Color::Rgb(217, 119, 87)),
            ("codex", "\u{ec81}", "◎", Color::Rgb(16, 163, 127)),
            ("pi", "\u{f03ff}", "π", Color::Reset),
        ]
        .into_iter()
        .map(|(agent, nerd, unicode, color)| {
            let text = match set {
                GlyphSet::Nerd => pad(set, nerd),
                GlyphSet::Unicode => pad(set, unicode),
            };
            let color = (color != Color::Reset).then_some(color);
            (agent.to_string(), Glyph { text, color })
        })
        .collect();
        Glyphs { set, table, brands }
    }

    pub fn get(&self, meaning: Meaning) -> &Glyph {
        // `table` holds every meaning, built from `Meaning::ALL`.
        &self
            .table
            .iter()
            .find(|(m, _)| *m == meaning)
            .unwrap_or(&self.table[0])
            .1
    }

    /// The harness's brand glyph; an unknown harness shows its name's first letter.
    pub fn brand(&self, agent: &str) -> Glyph {
        self.brands
            .iter()
            .find(|(name, _)| name == agent)
            .map(|(_, glyph)| glyph.clone())
            .unwrap_or_else(|| Glyph {
                text: agent.chars().next().map(String::from).unwrap_or_default(),
                color: None,
            })
    }

    /// `herdr-sidebar glyphs`: the table in use, one meaning per line.
    pub fn table_text(&self) -> String {
        let mut out = format!(
            "glyphs = \"{}\"\n",
            match self.set {
                GlyphSet::Nerd => "nerd",
                GlyphSet::Unicode => "unicode",
            }
        );
        for (meaning, glyph) in &self.table {
            out.push_str(&format!(
                "{:<20} {}  {}\n",
                meaning.key(),
                glyph.text.trim_end(),
                glyph.color.map(color_name).unwrap_or_else(|| "-".into())
            ));
        }
        for (agent, glyph) in &self.brands {
            out.push_str(&format!(
                "{:<20} {}  {}\n",
                format!("brand.{agent}"),
                glyph.text.trim_end(),
                glyph.color.map(color_name).unwrap_or_else(|| "-".into())
            ));
        }
        out
    }
}

/// kitty draws a Nerd Font glyph two cells wide over the space that follows it.
fn pad(set: GlyphSet, text: &str) -> String {
    match set {
        GlyphSet::Nerd if unicode_width::UnicodeWidthStr::width(text) < 2 => format!("{text} "),
        _ => text.to_string(),
    }
}

pub fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() == 6 {
            let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
            return Some(Color::Rgb(channel(0)?, channel(2)?, channel(4)?));
        }
        return None;
    }
    if let Ok(index) = value.parse::<u8>() {
        return Some(Color::Indexed(index));
    }
    Some(
        match value.to_ascii_lowercase().replace(['-', '_'], "").as_str() {
            "black" => Color::Black,
            "red" => Color::Red,
            "green" => Color::Green,
            "yellow" => Color::Yellow,
            "blue" => Color::Blue,
            "magenta" => Color::Magenta,
            "cyan" | "teal" => Color::Cyan,
            "gray" | "grey" => Color::Gray,
            "darkgray" | "darkgrey" => Color::DarkGray,
            "lightred" => Color::LightRed,
            "lightgreen" => Color::LightGreen,
            "lightyellow" => Color::LightYellow,
            "lightblue" => Color::LightBlue,
            "lightmagenta" => Color::LightMagenta,
            "lightcyan" => Color::LightCyan,
            "white" => Color::White,
            "default" | "reset" => Color::Reset,
            _ => return None,
        },
    )
}

pub fn color_name(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Indexed(index) => index.to_string(),
        Color::Cyan => "teal".into(),
        other => format!("{other:?}").to_ascii_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nerd_glyphs_take_a_trailing_space_and_unicode_ones_do_not() {
        let nerd = Glyphs::new(GlyphSet::Nerd, &[], &[]);
        assert_eq!(nerd.get(Meaning::Working).text, "\u{f111} ");
        let unicode = Glyphs::new(GlyphSet::Unicode, &[], &[]);
        assert_eq!(unicode.get(Meaning::Working).text, "●");
    }

    #[test]
    fn blocked_reasons_pick_their_own_glyph_and_colour() {
        let glyphs = Glyphs::new(GlyphSet::Unicode, &[], &[]);
        let question = Meaning::for_state(AgentStatus::Blocked, Some(BlockedReason::Question));
        assert_eq!(glyphs.get(question).color, Some(Color::Magenta));
        let interrupted =
            Meaning::for_state(AgentStatus::Blocked, Some(BlockedReason::Interrupted));
        assert_eq!(interrupted, Meaning::BlockedInterrupted);
        assert_eq!(glyphs.get(interrupted).text, "‖");
        assert_eq!(glyphs.get(interrupted).color, Some(Color::Magenta));
        let bare = Meaning::for_state(AgentStatus::Blocked, None);
        assert_eq!(bare, Meaning::Blocked);
        assert_eq!(glyphs.get(bare).color, Some(Color::Red));
    }

    #[test]
    fn overrides_replace_glyph_and_colour() {
        let glyphs = Glyphs::new(
            GlyphSet::Unicode,
            &[(Meaning::Idle, "o".into())],
            &[(Meaning::Idle, Color::Rgb(1, 2, 3))],
        );
        assert_eq!(glyphs.get(Meaning::Idle).text, "o");
        assert_eq!(glyphs.get(Meaning::Idle).color, Some(Color::Rgb(1, 2, 3)));
        assert!(glyphs.table_text().contains("idle"));
    }

    #[test]
    fn unread_marks_are_one_cell_distinct_and_coloured_by_report_kind() {
        use unicode_width::UnicodeWidthStr;
        let kinds = ["done", "failed", "blocked", "asking", "handoff", "progress"];
        for set in [GlyphSet::Nerd, GlyphSet::Unicode] {
            let glyphs = Glyphs::new(set, &[], &[]);
            let texts: std::collections::HashSet<String> = kinds
                .iter()
                .map(|kind| glyphs.get(Meaning::for_unread(kind)).text.clone())
                .collect();
            assert_eq!(texts.len(), kinds.len(), "{set:?}");
            assert!(texts.iter().all(|t| t.width() == 1), "{set:?}");
        }
        let nerd = Glyphs::new(GlyphSet::Nerd, &[], &[]);
        assert_eq!(nerd.get(Meaning::for_unread("progress")).text, "•");
        assert_eq!(
            nerd.get(Meaning::for_unread("done")).color,
            Some(Color::Cyan)
        );
        assert_eq!(
            nerd.get(Meaning::for_unread("failed")).color,
            Some(Color::Red)
        );
        assert_eq!(Meaning::for_unread("progress"), Meaning::Unread);
        assert_eq!(nerd.get(Meaning::Unread).color, None);
    }

    #[test]
    fn colours_parse_names_hex_and_indexes() {
        assert_eq!(parse_color("teal"), Some(Color::Cyan));
        assert_eq!(parse_color("#757ead"), Some(Color::Rgb(0x75, 0x7e, 0xad)));
        assert_eq!(parse_color("8"), Some(Color::Indexed(8)));
        assert_eq!(parse_color("nonsense"), None);
    }
}
