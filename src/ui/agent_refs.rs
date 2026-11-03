//! Live agent references (fork 061): an agent id on screen is drawn in its
//! agent's status colour, and the line's trailing blank cells say what it is
//! doing, the way GitLab renders `!57 (merged)`.
//!
//! This is render-only. It recolours cells and fills cells that were blank, so
//! the terminal grid, copy, selection and `pane read` never see it. It runs in
//! the multiplicative render paths: the scan is one byte test per cell and
//! allocates nothing, and the agent index is built only once a frame finds an
//! id-shaped token.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

use crate::api::schema::TaskStatus;
use crate::app::AppState;
use crate::detect::{AgentState, BlockedReason};
use crate::protocol::CellData;

const ID_LEN: usize = 8;
/// Distinct agents one line's end-of-line text can describe.
const MAX_EOL_REFS: usize = 4;
/// Blank cells kept between a line's last text and the end-of-line text.
const EOL_GAP: usize = 2;
const EOL_SEPARATOR: &str = "  ";
/// Closed agent ids remembered for `closed`; herdr forgets older ones.
pub(crate) const CLOSED_AGENT_MEMORY: usize = 256;

/// The sidebar's meanings (`tools/sidebar/src/glyphs.rs`), by `[sidebar.colors]` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
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
    UnreadDone,
    UnreadFailed,
    UnreadBlocked,
    Muted,
}

impl Tone {
    const ALL: [Tone; 14] = [
        Tone::Idle,
        Tone::Working,
        Tone::Awaiting,
        Tone::Done,
        Tone::BlockedQuestion,
        Tone::BlockedPermission,
        Tone::BlockedForm,
        Tone::BlockedInterrupted,
        Tone::Blocked,
        Tone::Unknown,
        Tone::UnreadDone,
        Tone::UnreadFailed,
        Tone::UnreadBlocked,
        Tone::Muted,
    ];

    fn key(self) -> &'static str {
        match self {
            Tone::Idle => "idle",
            Tone::Working => "working",
            Tone::Awaiting => "awaiting",
            Tone::Done => "done",
            Tone::BlockedQuestion => "blocked_question",
            Tone::BlockedPermission => "blocked_permission",
            Tone::BlockedForm => "blocked_form",
            Tone::BlockedInterrupted => "blocked_interrupted",
            Tone::Blocked => "blocked",
            Tone::Unknown => "unknown",
            Tone::UnreadDone => "unread_done",
            Tone::UnreadFailed => "unread_failed",
            Tone::UnreadBlocked => "unread_blocked",
            Tone::Muted => "muted",
        }
    }

    fn default_color(self) -> Color {
        match self {
            Tone::Idle => Color::Green,
            Tone::Working => Color::Yellow,
            Tone::Awaiting => Color::Blue,
            Tone::Done | Tone::UnreadDone => Color::Cyan,
            Tone::BlockedQuestion | Tone::BlockedInterrupted | Tone::UnreadBlocked => {
                Color::Magenta
            }
            Tone::BlockedPermission | Tone::BlockedForm | Tone::Blocked | Tone::UnreadFailed => {
                Color::Red
            }
            Tone::Unknown => Color::DarkGray,
            Tone::Muted => Color::Rgb(0x75, 0x7e, 0xad),
        }
    }

    fn for_blocked(reason: Option<BlockedReason>) -> Tone {
        match reason {
            Some(BlockedReason::Question) => Tone::BlockedQuestion,
            Some(BlockedReason::Permission) => Tone::BlockedPermission,
            Some(BlockedReason::Form) => Tone::BlockedForm,
            Some(BlockedReason::Interrupted) => Tone::BlockedInterrupted,
            None => Tone::Blocked,
        }
    }
}

/// `[ui] agent_references` and the colours `[sidebar.colors]` gives the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentRefConfig {
    pub(crate) enabled: bool,
    colors: [Color; Tone::ALL.len()],
}

impl Default for AgentRefConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            colors: Tone::ALL.map(Tone::default_color),
        }
    }
}

impl AgentRefConfig {
    pub(crate) fn from_config(config: &crate::config::Config) -> Self {
        let mut resolved = Self {
            enabled: config.ui.agent_references,
            ..Self::default()
        };
        let colors = config
            .sidebar
            .as_ref()
            .and_then(|sidebar| sidebar.get("colors"))
            .and_then(toml::Value::as_table);
        for (slot, tone) in resolved.colors.iter_mut().zip(Tone::ALL) {
            // The sidebar reports a bad value itself; here it keeps the default.
            if let Some(color) = colors
                .and_then(|colors| colors.get(tone.key()))
                .and_then(toml::Value::as_str)
                .and_then(sidebar_color)
            {
                *slot = color;
            }
        }
        resolved
    }

    fn color(&self, tone: Tone) -> Color {
        self.colors[tone as usize]
    }
}

/// The sidebar's colour syntax: `#rrggbb`, a palette index, or a colour name.
fn sidebar_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        let channel = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
        return (hex.len() == 6).then_some(Color::Rgb(channel(0)?, channel(2)?, channel(4)?));
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
            _ => return None,
        },
    )
}

/// What one agent id stands for at render time.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RefState {
    word: &'static str,
    tone: Tone,
    /// Shown after the word while the agent wants someone.
    pane: Option<String>,
}

/// Word and tone: loom's task status where it says more than herdr's agent
/// status (wanted, finished, failed), else herdr's own status word.
fn ref_word(
    task: Option<TaskStatus>,
    state: AgentState,
    seen: bool,
    reason: Option<BlockedReason>,
) -> (&'static str, Tone, bool) {
    if task == Some(TaskStatus::NeedsUser) {
        return ("asking", Tone::BlockedQuestion, true);
    }
    if state == AgentState::Blocked {
        return ("blocked", Tone::for_blocked(reason), true);
    }
    match task {
        Some(TaskStatus::NeedsParent) => return ("blocked", Tone::UnreadBlocked, true),
        Some(TaskStatus::Failed) => return ("failed", Tone::UnreadFailed, false),
        Some(TaskStatus::Complete) => return ("done", Tone::UnreadDone, false),
        Some(TaskStatus::Closed) => return ("closed", Tone::Muted, false),
        _ => {}
    }
    match (state, seen) {
        (AgentState::Working, _) => ("working", Tone::Working, false),
        (AgentState::Awaiting, _) => ("awaiting", Tone::Awaiting, false),
        (AgentState::Idle, true) => ("idle", Tone::Idle, false),
        (AgentState::Idle, false) => ("done", Tone::Done, false),
        (AgentState::Unknown, _) | (AgentState::Blocked, _) => match task {
            Some(TaskStatus::Starting) => ("starting", Tone::Unknown, false),
            Some(TaskStatus::Running) => ("running", Tone::Unknown, false),
            _ => ("idle", Tone::Unknown, false),
        },
    }
}

/// Every agent id herdr can describe now, sorted for lookup.
#[derive(Debug, Default)]
struct AgentRefIndex {
    entries: Vec<([u8; ID_LEN], RefState)>,
}

impl AgentRefIndex {
    fn build(app: &AppState) -> Self {
        let mut entries = Vec::new();
        for ws in &app.workspaces {
            for tab in &ws.tabs {
                for (pane_id, pane) in &tab.panes {
                    let Some(terminal) = app.terminals.get(&pane.attached_terminal_id) else {
                        continue;
                    };
                    let Some(id) = terminal
                        .agent_id
                        .as_ref()
                        .and_then(|id| id_bytes(id.as_str()))
                    else {
                        continue;
                    };
                    let (word, tone, wants) = ref_word(
                        terminal.task_status,
                        terminal.state,
                        pane.seen,
                        terminal.blocked_reason(),
                    );
                    let pane = wants
                        .then(|| ws.public_pane_number(*pane_id))
                        .flatten()
                        .map(|number| crate::workspace::public_pane_id_for_number(&ws.id, number));
                    entries.push((id, RefState { word, tone, pane }));
                }
            }
        }
        for id in &app.closed_agent_ids {
            if let Some(id) = id_bytes(id.as_str()) {
                entries.push((
                    id,
                    RefState {
                        word: "closed",
                        tone: Tone::Muted,
                        pane: None,
                    },
                ));
            }
        }
        // A live agent wins over a closed record of the same id.
        entries.sort_by_key(|entry| entry.0);
        entries.dedup_by(|later, earlier| later.0 == earlier.0);
        Self { entries }
    }

    fn find(&self, id: &[u8; ID_LEN]) -> Option<usize> {
        self.entries.binary_search_by(|entry| entry.0.cmp(id)).ok()
    }
}

fn id_bytes(id: &str) -> Option<[u8; ID_LEN]> {
    id.as_bytes().try_into().ok()
}

/// The decoration context for one render pass: the agent index is built the
/// first time a row holds an id-shaped token, and at most once per pass.
pub(crate) struct AgentRefs<'a> {
    app: &'a AppState,
    index: Option<AgentRefIndex>,
}

impl<'a> AgentRefs<'a> {
    /// `None` when the feature is off or no agent could match, so callers skip the scan.
    pub(crate) fn new(app: &'a AppState) -> Option<Self> {
        let any = !app.closed_agent_ids.is_empty()
            || app
                .terminals
                .values()
                .any(|terminal| terminal.agent_id.is_some());
        (app.agent_references.enabled && any).then_some(Self { app, index: None })
    }

    fn lookup(&mut self, id: &[u8; ID_LEN]) -> Option<usize> {
        let app = self.app;
        self.index
            .get_or_insert_with(|| AgentRefIndex::build(app))
            .find(id)
    }

    fn entry(&self, slot: usize) -> &RefState {
        &self.index.as_ref().expect("looked up before").entries[slot].1
    }

    fn color(&self, tone: Tone) -> Color {
        self.app.agent_references.color(tone)
    }
}

/// One rendered row, as the full renderer's buffer or a retained patch row.
pub(crate) trait RefRow {
    fn width(&self) -> usize;
    fn symbol(&self, x: usize) -> &str;
    fn set_fg(&mut self, x: usize, color: Color);
    /// Writes one narrow character into a cell, dimmed, keeping its background.
    fn put_dim(&mut self, x: usize, ch: char, color: Color);
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The byte of a single-byte cell, or `None` for anything wider or empty.
fn cell_byte(symbol: &str) -> Option<u8> {
    match symbol.as_bytes() {
        [byte] => Some(*byte),
        _ => None,
    }
}

/// The shape `AgentId::alloc` produces: a digit, then seven lowercase hex digits.
fn is_agent_id_shape(bytes: &[u8]) -> bool {
    bytes.len() == ID_LEN
        && bytes[0].is_ascii_digit()
        && bytes.iter().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Recolours each known agent id in the row and, where the row ends in enough
/// blank cells, describes them there.
pub(crate) fn decorate_row<R: RefRow + ?Sized>(row: &mut R, refs: &mut AgentRefs<'_>) {
    let width = row.width();
    let mut found = [0usize; MAX_EOL_REFS];
    let mut found_len = 0;
    let mut x = 0;
    while x < width {
        if !cell_byte(row.symbol(x)).is_some_and(is_word_byte) {
            x += 1;
            continue;
        }
        let start = x;
        while x < width && cell_byte(row.symbol(x)).is_some_and(is_word_byte) {
            x += 1;
        }
        if x - start != ID_LEN {
            continue;
        }
        let mut id = [0u8; ID_LEN];
        for (offset, byte) in id.iter_mut().enumerate() {
            *byte = cell_byte(row.symbol(start + offset)).unwrap_or(0);
        }
        if !is_agent_id_shape(&id) {
            continue;
        }
        let Some(slot) = refs.lookup(&id) else {
            continue;
        };
        let color = refs.color(refs.entry(slot).tone);
        for cell in start..x {
            row.set_fg(cell, color);
        }
        if found_len < MAX_EOL_REFS && !found[..found_len].contains(&slot) {
            found[found_len] = slot;
            found_len += 1;
        }
    }
    if found_len > 0 {
        write_eol(row, refs, &found[..found_len]);
    }
}

/// One segment per distinct id in line order, each as long as it fits whole:
/// `asking · w1:p2`, else `asking`. The first id that does not fit ends it.
fn write_eol<R: RefRow + ?Sized>(row: &mut R, refs: &AgentRefs<'_>, slots: &[usize]) {
    let width = row.width();
    let text_end = (0..width)
        .rev()
        .find(|&x| row.symbol(x) != " ")
        .map_or(0, |x| x + 1);
    let mut x = text_end + EOL_GAP;
    for (index, &slot) in slots.iter().enumerate() {
        let state = refs.entry(slot);
        let separator = if index == 0 { 0 } else { EOL_SEPARATOR.len() };
        let word = state.word.len();
        let full = state
            .pane
            .as_deref()
            .map_or(word, |pane| word + " · ".chars().count() + pane.len());
        let room = width.saturating_sub(x + separator);
        let with_pane = full <= room;
        if !with_pane && word > room {
            break;
        }
        let color = refs.color(state.tone);
        let mut put = |text: &str, x: &mut usize| {
            for ch in text.chars() {
                row.put_dim(*x, ch, color);
                *x += 1;
            }
        };
        if index > 0 {
            put(EOL_SEPARATOR, &mut x);
        }
        put(state.word, &mut x);
        if let (true, Some(pane)) = (with_pane, state.pane.as_deref()) {
            put(" · ", &mut x);
            put(pane, &mut x);
        }
    }
}

/// The full renderer's rows inside one pane's terminal area.
pub(crate) fn decorate_area(buf: &mut Buffer, area: Rect, refs: &mut AgentRefs<'_>) {
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        decorate_row(&mut BufferRow { buf, area, y }, refs);
    }
}

struct BufferRow<'b> {
    buf: &'b mut Buffer,
    area: Rect,
    y: u16,
}

impl BufferRow<'_> {
    fn position(&self, x: usize) -> (u16, u16) {
        (self.area.x + x as u16, self.y)
    }
}

impl RefRow for BufferRow<'_> {
    fn width(&self) -> usize {
        usize::from(self.area.width)
    }

    fn symbol(&self, x: usize) -> &str {
        self.buf[self.position(x)].symbol()
    }

    fn set_fg(&mut self, x: usize, color: Color) {
        let position = self.position(x);
        self.buf[position].set_fg(color);
    }

    fn put_dim(&mut self, x: usize, ch: char, color: Color) {
        let position = self.position(x);
        let cell = &mut self.buf[position];
        cell.set_char(ch);
        cell.set_fg(color);
        cell.modifier.insert(Modifier::DIM);
    }
}

impl RefRow for [CellData] {
    fn width(&self) -> usize {
        self.len()
    }

    fn symbol(&self, x: usize) -> &str {
        &self[x].symbol
    }

    fn set_fg(&mut self, x: usize, color: Color) {
        self[x].fg = crate::protocol::color_to_u32(color);
    }

    fn put_dim(&mut self, x: usize, ch: char, color: Color) {
        let cell = &mut self[x];
        cell.symbol.clear();
        cell.symbol.push(ch);
        cell.fg = crate::protocol::color_to_u32(color);
        cell.modifier |= crate::protocol::modifier_to_u16(Modifier::DIM);
    }
}

/// The agent id under a click in a link-sized run of text, for
/// `viewport_link_regions`: the id-shaped word holding `clicked_byte`.
pub(crate) fn agent_id_byte_range(
    text: &str,
    clicked_byte: usize,
) -> Option<std::ops::Range<usize>> {
    let bytes = text.as_bytes();
    if !bytes.get(clicked_byte).copied().is_some_and(is_word_byte) {
        return None;
    }
    let start = bytes[..clicked_byte]
        .iter()
        .rposition(|byte| !is_word_byte(*byte))
        .map_or(0, |at| at + 1);
    let end = bytes[clicked_byte..]
        .iter()
        .position(|byte| !is_word_byte(*byte))
        .map_or(bytes.len(), |at| clicked_byte + at);
    is_agent_id_shape(&bytes[start..end]).then_some(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;
    use crate::terminal::AgentId;

    fn row_text(row: &[CellData]) -> String {
        row.iter().map(|cell| cell.symbol.as_str()).collect()
    }

    fn cells(text: &str, width: usize) -> Vec<CellData> {
        let mut row: Vec<CellData> = text
            .chars()
            .map(|ch| CellData {
                symbol: ch.to_string(),
                fg: 0,
                bg: 0,
                modifier: 0,
                skip: false,
                hyperlink: None,
            })
            .collect();
        row.resize(
            width,
            CellData {
                symbol: " ".into(),
                fg: 0,
                bg: 0,
                modifier: 0,
                skip: false,
                hyperlink: None,
            },
        );
        row
    }

    /// One workspace whose root pane's terminal is agent `id`.
    fn app_with_agent(id: &str, task: Option<TaskStatus>, state: AgentState) -> AppState {
        let mut app = AppState::test_new();
        let workspace = crate::workspace::Workspace::test_new("refs");
        let pane_id = workspace.tabs[0].root_pane;
        let terminal_id = workspace.tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        app.workspaces.push(workspace);
        app.active = Some(0);
        app.ensure_test_terminals();
        let terminal = app.terminals.get_mut(&terminal_id).expect("test terminal");
        terminal.agent_id = AgentId::parse(id);
        terminal.task_status = task;
        terminal.state = state;
        app
    }

    fn decorate(app: &AppState, row: &mut [CellData]) {
        if let Some(mut refs) = AgentRefs::new(app) {
            decorate_row(row, &mut refs);
        }
    }

    #[test]
    fn a_known_id_is_recoloured_and_described_at_the_end_of_the_line() {
        let app = app_with_agent("8c1ee73d", Some(TaskStatus::NeedsUser), AgentState::Idle);
        let mut row = cells("child (8c1ee73d pimig) needs you", 60);
        decorate(&app, &mut row);
        let magenta = crate::protocol::color_to_u32(Color::Magenta);
        assert!(row[7..15].iter().all(|cell| cell.fg == magenta));
        assert_eq!(row[6].fg, 0);
        assert_eq!(row[15].fg, 0);
        assert_eq!(
            row_text(&row).trim_end(),
            "child (8c1ee73d pimig) needs you  asking · w1:p1"
        );
        let dim = crate::protocol::modifier_to_u16(Modifier::DIM);
        assert_eq!(row[34].modifier & dim, dim);
        assert_eq!(row[31].modifier & dim, 0);
    }

    #[test]
    fn ids_match_bare_prefixed_and_bracketed_but_not_inside_longer_words() {
        let app = app_with_agent("8c1ee73d", Some(TaskStatus::Complete), AgentState::Idle);
        let cyan = crate::protocol::color_to_u32(Color::Cyan);
        for (text, start) in [("8c1ee73d", 0), ("loom:8c1ee73d", 5), ("[8c1ee73d]", 1)] {
            let mut row = cells(text, 40);
            decorate(&app, &mut row);
            assert_eq!(row[start].fg, cyan, "{text}");
            assert!(row_text(&row).ends_with("done") || row_text(&row).contains("  done"));
        }
        for text in ["x8c1ee73d", "8c1ee73d0", "8c1ee73d_a", "8C1EE73D"] {
            let mut row = cells(text, 40);
            decorate(&app, &mut row);
            assert!(row.iter().all(|cell| cell.fg == 0), "{text}");
            assert_eq!(row_text(&row).trim_end(), text);
        }
    }

    #[test]
    fn unknown_ids_and_a_disabled_switch_leave_the_row_alone() {
        let mut app = app_with_agent("8c1ee73d", None, AgentState::Working);
        let mut row = cells("9b636ee5 fix: carry", 40);
        let before = row.clone();
        decorate(&app, &mut row);
        assert_eq!(row, before);

        app.agent_references.enabled = false;
        let mut row = cells("8c1ee73d", 40);
        let before = row.clone();
        decorate(&app, &mut row);
        assert_eq!(row, before);
    }

    #[test]
    fn a_full_line_is_recoloured_only() {
        let app = app_with_agent("8c1ee73d", None, AgentState::Working);
        let mut row = cells("see 8c1ee73d now", 18);
        decorate(&app, &mut row);
        assert_eq!(row_text(&row), "see 8c1ee73d now  ");
        assert_eq!(row[4].fg, crate::protocol::color_to_u32(Color::Yellow));
    }

    #[test]
    fn a_short_line_drops_the_pane_before_the_word() {
        let app = app_with_agent("8c1ee73d", Some(TaskStatus::NeedsUser), AgentState::Idle);
        let mut row = cells("8c1ee73d", 18);
        decorate(&app, &mut row);
        assert_eq!(row_text(&row), "8c1ee73d  asking  ");
    }

    #[test]
    fn several_ids_are_described_in_line_order_while_they_fit() {
        let mut app = app_with_agent("8c1ee73d", None, AgentState::Working);
        app.closed_agent_ids
            .push_back(AgentId::parse("1234abcd").unwrap());
        let mut row = cells("8c1ee73d and 1234abcd and 8c1ee73d", 60);
        decorate(&app, &mut row);
        assert_eq!(
            row_text(&row).trim_end(),
            "8c1ee73d and 1234abcd and 8c1ee73d  working  closed"
        );
        let mut row = cells("8c1ee73d and 1234abcd and 8c1ee73d", 46);
        decorate(&app, &mut row);
        assert_eq!(
            row_text(&row).trim_end(),
            "8c1ee73d and 1234abcd and 8c1ee73d  working"
        );
    }

    #[test]
    fn a_live_agent_wins_over_a_closed_record_of_its_id() {
        let mut app = app_with_agent("8c1ee73d", None, AgentState::Working);
        app.closed_agent_ids
            .push_back(AgentId::parse("8c1ee73d").unwrap());
        let mut row = cells("8c1ee73d", 30);
        decorate(&app, &mut row);
        assert_eq!(row_text(&row).trim_end(), "8c1ee73d  working");
    }

    #[test]
    fn the_word_prefers_what_wants_someone_then_loom_then_herdr() {
        use AgentState::*;
        let word = |task, state, seen| ref_word(task, state, seen, None).0;
        assert_eq!(word(Some(TaskStatus::NeedsUser), Working, true), "asking");
        assert_eq!(word(Some(TaskStatus::Complete), Blocked, true), "blocked");
        assert_eq!(word(Some(TaskStatus::NeedsParent), Idle, true), "blocked");
        assert_eq!(word(Some(TaskStatus::Complete), Idle, true), "done");
        assert_eq!(word(Some(TaskStatus::Failed), Idle, true), "failed");
        assert_eq!(word(Some(TaskStatus::Running), Working, true), "working");
        assert_eq!(word(Some(TaskStatus::Running), Unknown, true), "running");
        assert_eq!(word(None, Idle, false), "done");
        assert_eq!(word(None, Awaiting, true), "awaiting");
    }

    #[test]
    fn sidebar_colour_overrides_apply() {
        let config: crate::config::Config = toml::from_str(
            "[sidebar]\ncolors = { working = \"#010203\", idle = \"8\", done = \"nope\" }",
        )
        .unwrap();
        let refs = AgentRefConfig::from_config(&config);
        assert_eq!(refs.color(Tone::Working), Color::Rgb(1, 2, 3));
        assert_eq!(refs.color(Tone::Idle), Color::Indexed(8));
        assert_eq!(refs.color(Tone::Done), Color::Cyan);
        assert!(refs.enabled);
        let config: crate::config::Config =
            toml::from_str("[ui]\nagent_references = false").unwrap();
        assert!(!AgentRefConfig::from_config(&config).enabled);
    }

    #[test]
    fn the_click_range_is_the_id_shaped_word_under_the_pointer() {
        let text = "loom:8c1ee73d,";
        assert_eq!(agent_id_byte_range(text, 5), Some(5..13));
        assert_eq!(agent_id_byte_range(text, 12), Some(5..13));
        assert_eq!(agent_id_byte_range(text, 2), None);
        assert_eq!(agent_id_byte_range(text, 13), None);
        assert_eq!(agent_id_byte_range("8c1ee73dz", 0), None);
    }

    #[test]
    fn the_buffer_renderer_decorates_inside_the_pane_area_only() {
        let app = app_with_agent("8c1ee73d", None, AgentState::Working);
        let mut buf = Buffer::with_lines(["|8c1ee73d            |"]);
        let mut refs = AgentRefs::new(&app).unwrap();
        decorate_area(&mut buf, Rect::new(1, 0, 20, 1), &mut refs);
        assert_eq!(buf[(1, 0)].fg, Color::Yellow);
        assert_eq!(buf[(0, 0)].fg, Color::Reset);
        let line: String = (0..22).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert_eq!(line, "|8c1ee73d  working   |");
        assert!(buf[(11, 0)].modifier.contains(Modifier::DIM));
    }
}
