//! Live agent references (fork 061, 065): an agent id on screen is drawn in its
//! agent's status colour, and the line's trailing blank cells say who it is and
//! what it is doing, right-aligned, the way GitLab renders `!57 (merged)`.
//!
//! This is render-only. It recolours cells and fills cells that were blank, so
//! the terminal grid, copy, selection and `pane read` never see it. It runs in
//! the multiplicative render paths: the scan is one byte test per cell and
//! allocates nothing, and the agent index, names included, is built only once a
//! frame finds an id-shaped token.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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
const PART_SEPARATOR: &str = " · ";
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

/// Text drawn in cells, with its width measured once.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Label {
    text: String,
    width: usize,
}

impl Label {
    fn new(text: String) -> Self {
        let width = text.graphemes(true).map(grapheme_width).sum();
        Self { text, width }
    }
}

/// Cells a grapheme takes; control and zero-width graphemes are not drawn.
fn grapheme_width(grapheme: &str) -> usize {
    if grapheme.chars().any(char::is_control) {
        return 0;
    }
    grapheme.width().min(2)
}

/// What one agent id stands for at render time.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RefState {
    word: &'static str,
    tone: Tone,
    /// The sidebar's row name; a closed agent has none.
    name: Option<Label>,
    /// The agent's workspace index; a closed agent has none.
    workspace: Option<usize>,
}

/// Word and tone: loom's task status where it says more than herdr's agent
/// status (wanted, finished, failed), else herdr's own status word.
fn ref_word(
    task: Option<TaskStatus>,
    state: AgentState,
    seen: bool,
    reason: Option<BlockedReason>,
) -> (&'static str, Tone) {
    if task == Some(TaskStatus::NeedsUser) {
        return ("asking", Tone::BlockedQuestion);
    }
    if state == AgentState::Blocked {
        return ("blocked", Tone::for_blocked(reason));
    }
    match task {
        Some(TaskStatus::NeedsParent) => return ("blocked", Tone::UnreadBlocked),
        Some(TaskStatus::Failed) => return ("failed", Tone::UnreadFailed),
        Some(TaskStatus::Complete) => return ("done", Tone::UnreadDone),
        Some(TaskStatus::Closed) => return ("closed", Tone::Muted),
        _ => {}
    }
    match (state, seen) {
        (AgentState::Working, _) => ("working", Tone::Working),
        (AgentState::Awaiting, _) => ("awaiting", Tone::Awaiting),
        (AgentState::Idle, true) => ("idle", Tone::Idle),
        (AgentState::Idle, false) => ("done", Tone::Done),
        (AgentState::Unknown, _) | (AgentState::Blocked, _) => match task {
            Some(TaskStatus::Starting) => ("starting", Tone::Unknown),
            Some(TaskStatus::Running) => ("running", Tone::Unknown),
            _ => ("idle", Tone::Unknown),
        },
    }
}

fn is_agent(terminal: &crate::terminal::TerminalState) -> bool {
    terminal.agent_id.is_some() || terminal.effective_agent_label().is_some()
}

/// What herdr-sidebar calls the agent's row (`tools/sidebar/src/model.rs`
/// `agent_name`): its tab's name, or for a tab's second agent its own label or
/// title, since the tab's name belongs to the tab's first agent.
fn agent_name(
    ws: &crate::workspace::Workspace,
    tab_idx: usize,
    pane_id: crate::layout::PaneId,
    terminal: &crate::terminal::TerminalState,
    host: Option<crate::layout::PaneId>,
) -> String {
    let own = || {
        terminal
            .manual_label
            .clone()
            .or_else(|| terminal.terminal_title_stripped())
            .or_else(|| terminal.effective_title())
            .filter(|title| !title.trim().is_empty())
    };
    let tab = ws
        .tab_display_name(tab_idx)
        .filter(|label| !label.is_empty());
    let name = if host.is_some_and(|host| host != pane_id) {
        own().or(tab)
    } else {
        tab.or_else(|| terminal.manual_label.clone())
    };
    name.or_else(|| terminal.effective_agent_label().map(str::to_string))
        .or_else(|| {
            ws.public_pane_number(pane_id)
                .map(|number| crate::workspace::public_pane_id_for_number(&ws.id, number))
        })
        .unwrap_or_default()
}

/// Every agent id herdr can describe now, sorted for lookup.
#[derive(Debug, Default)]
struct AgentRefIndex {
    entries: Vec<([u8; ID_LEN], RefState)>,
    /// Workspace labels by index, for the workspaces that hold an agent.
    workspaces: Vec<Option<Label>>,
}

impl AgentRefIndex {
    fn build(app: &AppState) -> Self {
        let mut entries = Vec::new();
        let mut workspaces = vec![None; app.workspaces.len()];
        for (ws_idx, ws) in app.workspaces.iter().enumerate() {
            for (tab_idx, tab) in ws.tabs.iter().enumerate() {
                let terminal_of =
                    |pane: &crate::pane::PaneState| app.terminals.get(&pane.attached_terminal_id);
                // The tab's first agent, by creation, as the sidebar picks it.
                let host = tab
                    .panes
                    .iter()
                    .filter(|(_, pane)| terminal_of(pane).is_some_and(is_agent))
                    .min_by_key(|(pane_id, _)| ws.public_pane_number(**pane_id))
                    .map(|(pane_id, _)| *pane_id);
                for (pane_id, pane) in &tab.panes {
                    let Some(terminal) = terminal_of(pane) else {
                        continue;
                    };
                    let Some(id) = terminal
                        .agent_id
                        .as_ref()
                        .and_then(|id| id_bytes(id.as_str()))
                    else {
                        continue;
                    };
                    let (word, tone) = ref_word(
                        terminal.task_status,
                        terminal.state,
                        pane.seen,
                        terminal.blocked_reason(),
                    );
                    let name = agent_name(ws, tab_idx, *pane_id, terminal, host);
                    if workspaces[ws_idx].is_none() {
                        workspaces[ws_idx] =
                            Some(Label::new(ws.display_name_from_terminals(&app.terminals)));
                    }
                    entries.push((
                        id,
                        RefState {
                            word,
                            tone,
                            name: Some(Label::new(name)),
                            workspace: Some(ws_idx),
                        },
                    ));
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
                        name: None,
                        workspace: None,
                    },
                ));
            }
        }
        // A live agent wins over a closed record of the same id.
        entries.sort_by_key(|entry| entry.0);
        entries.dedup_by(|later, earlier| later.0 == earlier.0);
        Self {
            entries,
            workspaces,
        }
    }

    fn find(&self, id: &[u8; ID_LEN]) -> Option<usize> {
        self.entries.binary_search_by(|entry| entry.0.cmp(id)).ok()
    }
}

/// How much of a segment is drawn, longest first: `name · workspace · word`,
/// `name · word`, then the word alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    Full,
    Named,
    Word,
}

impl Form {
    const LADDER: [Form; 3] = [Form::Full, Form::Named, Form::Word];
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

    /// The label of the agent's workspace.
    fn workspace_label(&self, state: &RefState) -> Option<&Label> {
        let index = self.index.as_ref()?;
        state
            .workspace
            .and_then(|workspace| index.workspaces.get(workspace)?.as_ref())
    }

    /// Cells a segment takes in `form`, or `None` where the form has nothing to add.
    fn segment_width(&self, slot: usize, form: Form, workspace: usize) -> Option<usize> {
        let state = self.entry(slot);
        let word = state.word.len();
        let part = PART_SEPARATOR.chars().count();
        match form {
            Form::Word => Some(word),
            Form::Named => Some(state.name.as_ref()?.width + part + word),
            Form::Full => {
                if state.workspace? == workspace {
                    return None;
                }
                let label = self.workspace_label(state)?;
                Some(state.name.as_ref()?.width + part + label.width + part + word)
            }
        }
    }
}

/// One rendered row, as the full renderer's buffer or a retained patch row.
pub(crate) trait RefRow {
    fn width(&self) -> usize;
    fn symbol(&self, x: usize) -> &str;
    fn set_fg(&mut self, x: usize, color: Color);
    /// Writes one grapheme into a cell, dimmed, keeping its background.
    fn put_dim(&mut self, x: usize, symbol: &str, color: Color);
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
/// blank cells, describes them there. `workspace` is the drawn pane's workspace
/// index: an agent elsewhere is described with its workspace's label.
pub(crate) fn decorate_row<R: RefRow + ?Sized>(
    row: &mut R,
    refs: &mut AgentRefs<'_>,
    workspace: usize,
) {
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
        write_eol(row, refs, &found[..found_len], workspace);
    }
}

/// The cell after the row's last text, counting a trailing wide grapheme whole.
fn text_end<R: RefRow + ?Sized>(row: &R) -> usize {
    (0..row.width())
        .rev()
        .find(|&x| !matches!(row.symbol(x), " " | ""))
        .map_or(0, |x| x + grapheme_width(row.symbol(x)).max(1))
}

/// One segment per distinct id in line order, each in the longest form that
/// fits (`name · workspace · word`, `name · word`, `word`), right-aligned as a
/// group to the row's last column with at least `EOL_GAP` blank cells before it.
/// The first id with no form that fits ends the text.
fn write_eol<R: RefRow + ?Sized>(
    row: &mut R,
    refs: &AgentRefs<'_>,
    slots: &[usize],
    workspace: usize,
) {
    let width = row.width();
    let room = width.saturating_sub(text_end(row) + EOL_GAP);
    let mut chosen = [(0usize, Form::Word); MAX_EOL_REFS];
    let mut chosen_len = 0;
    let mut used = 0;
    for &slot in slots {
        let separator = if chosen_len == 0 {
            0
        } else {
            EOL_SEPARATOR.len()
        };
        let fit = Form::LADDER.into_iter().find(|&form| {
            refs.segment_width(slot, form, workspace)
                .is_some_and(|segment| used + separator + segment <= room)
        });
        let Some(form) = fit else {
            break;
        };
        used += separator + refs.segment_width(slot, form, workspace).unwrap_or(0);
        chosen[chosen_len] = (slot, form);
        chosen_len += 1;
    }
    let mut x = width - used;
    for (index, &(slot, form)) in chosen[..chosen_len].iter().enumerate() {
        let state = refs.entry(slot);
        let color = refs.color(state.tone);
        if index > 0 {
            put_text(row, &mut x, EOL_SEPARATOR, color);
        }
        if form != Form::Word {
            if let Some(name) = &state.name {
                put_text(row, &mut x, &name.text, color);
                put_text(row, &mut x, PART_SEPARATOR, color);
            }
        }
        if form == Form::Full {
            if let Some(label) = refs.workspace_label(state) {
                put_text(row, &mut x, &label.text, color);
                put_text(row, &mut x, PART_SEPARATOR, color);
            }
        }
        put_text(row, &mut x, state.word, color);
    }
}

/// Writes `text` from cell `x` on, dimmed; a wide grapheme's second cell is blank.
fn put_text<R: RefRow + ?Sized>(row: &mut R, x: &mut usize, text: &str, color: Color) {
    for grapheme in text.graphemes(true) {
        let cells = grapheme_width(grapheme);
        if cells == 0 {
            continue;
        }
        row.put_dim(*x, grapheme, color);
        for covered in 1..cells {
            row.put_dim(*x + covered, " ", color);
        }
        *x += cells;
    }
}

/// The full renderer's rows inside one pane's terminal area.
pub(crate) fn decorate_area(
    buf: &mut Buffer,
    area: Rect,
    refs: &mut AgentRefs<'_>,
    workspace: usize,
) {
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        decorate_row(&mut BufferRow { buf, area, y }, refs, workspace);
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

    fn put_dim(&mut self, x: usize, symbol: &str, color: Color) {
        let position = self.position(x);
        let cell = &mut self.buf[position];
        cell.set_symbol(symbol);
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

    fn put_dim(&mut self, x: usize, symbol: &str, color: Color) {
        let cell = &mut self[x];
        cell.symbol.clear();
        cell.symbol.push_str(symbol);
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
    use crate::layout::PaneId;
    use crate::terminal::AgentId;
    use crate::workspace::Workspace;
    use ratatui::layout::Direction;

    fn row_text(row: &[CellData]) -> String {
        row.iter().map(|cell| cell.symbol.as_str()).collect()
    }

    fn blank() -> CellData {
        CellData {
            symbol: " ".into(),
            fg: 0,
            bg: 0,
            modifier: 0,
            skip: false,
            hyperlink: None,
        }
    }

    fn cells(text: &str, width: usize) -> Vec<CellData> {
        let mut row: Vec<CellData> = text
            .chars()
            .map(|ch| CellData {
                symbol: ch.to_string(),
                ..blank()
            })
            .collect();
        row.resize(width, blank());
        row
    }

    /// `text` padded with blanks so it ends at the row's last column.
    fn right(line: &str, segment: &str, width: usize) -> String {
        let used = line.chars().count() + segment.chars().count();
        format!("{line}{}{segment}", " ".repeat(width - used))
    }

    fn workspace(name: &str, tab: &str) -> Workspace {
        let mut workspace = Workspace::test_new(name);
        workspace.tabs[0].custom_name = Some(tab.to_string());
        workspace
    }

    fn set_agent<'a>(
        app: &'a mut AppState,
        ws: usize,
        tab: usize,
        pane: PaneId,
        id: &str,
        task: Option<TaskStatus>,
        state: AgentState,
    ) -> &'a mut crate::terminal::TerminalState {
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[ws].tabs[tab].panes[&pane]
            .attached_terminal_id
            .clone();
        let terminal = app.terminals.get_mut(&terminal_id).expect("test terminal");
        terminal.agent_id = AgentId::parse(id);
        terminal.task_status = task;
        terminal.state = state;
        terminal
    }

    /// Workspace `refs`, whose tab `main-tracker` holds agent `id`.
    fn app_with_agent(id: &str, task: Option<TaskStatus>, state: AgentState) -> AppState {
        let mut app = AppState::test_new();
        app.workspaces.push(workspace("refs", "main-tracker"));
        app.active = Some(0);
        let pane = app.workspaces[0].tabs[0].root_pane;
        set_agent(&mut app, 0, 0, pane, id, task, state);
        app
    }

    /// `app_with_agent`, plus workspace `pi-migration` whose tab `pimig-002`
    /// holds the asking agent `5a1b2c3d`.
    fn app_across_workspaces() -> AppState {
        let mut app = app_with_agent("8c1ee73d", None, AgentState::Working);
        app.workspaces.push(workspace("pi-migration", "pimig-002"));
        let pane = app.workspaces[1].tabs[0].root_pane;
        set_agent(
            &mut app,
            1,
            0,
            pane,
            "5a1b2c3d",
            Some(TaskStatus::NeedsUser),
            AgentState::Idle,
        );
        app
    }

    fn decorate_in(app: &AppState, row: &mut [CellData], workspace: usize) {
        if let Some(mut refs) = AgentRefs::new(app) {
            decorate_row(row, &mut refs, workspace);
        }
    }

    fn decorate(app: &AppState, row: &mut [CellData]) {
        decorate_in(app, row, 0);
    }

    #[test]
    fn a_known_id_is_recoloured_and_named_at_the_end_of_the_line() {
        let app = app_with_agent("8c1ee73d", Some(TaskStatus::NeedsUser), AgentState::Idle);
        let line = "child (8c1ee73d pimig) needs you";
        let mut row = cells(line, 60);
        decorate(&app, &mut row);
        let magenta = crate::protocol::color_to_u32(Color::Magenta);
        assert!(row[7..15].iter().all(|cell| cell.fg == magenta));
        assert_eq!(row[6].fg, 0);
        assert_eq!(row[15].fg, 0);
        assert_eq!(row_text(&row), right(line, "main-tracker · asking", 60));
        let dim = crate::protocol::modifier_to_u16(Modifier::DIM);
        assert!(row[39..].iter().all(|cell| cell.modifier & dim == dim));
        assert!(row[39..].iter().all(|cell| cell.fg == magenta));
        assert!(row[..39].iter().all(|cell| cell.modifier & dim == 0));
    }

    #[test]
    fn an_agent_in_another_workspace_names_that_workspace() {
        let app = app_across_workspaces();
        let line = "pimig 5a1b2c3d needs you";
        let mut row = cells(line, 70);
        decorate_in(&app, &mut row, 0);
        assert_eq!(
            row_text(&row),
            right(line, "pimig-002 · pi-migration · asking", 70)
        );
        let mut row = cells(line, 70);
        decorate_in(&app, &mut row, 1);
        assert_eq!(row_text(&row), right(line, "pimig-002 · asking", 70));
    }

    #[test]
    fn a_tight_line_drops_the_workspace_then_the_name_then_everything() {
        let app = app_across_workspaces();
        let line = "5a1b2c3d";
        let decorated = |width: usize| {
            let mut row = cells(line, width);
            decorate_in(&app, &mut row, 0);
            (row_text(&row), row[0].fg)
        };
        // Each form at exactly the width that keeps two blank cells before it.
        let full = "pimig-002 · pi-migration · asking";
        assert_eq!(decorated(43).0, format!("{line}  {full}"));
        assert_eq!(decorated(42).0, right(line, "pimig-002 · asking", 42));
        assert_eq!(decorated(28).0, format!("{line}  pimig-002 · asking"));
        assert_eq!(decorated(27).0, right(line, "asking", 27));
        assert_eq!(decorated(16).0, format!("{line}  asking"));
        let (text, fg) = decorated(15);
        assert_eq!(text, right(line, "", 15));
        assert_eq!(fg, crate::protocol::color_to_u32(Color::Magenta));
    }

    #[test]
    fn several_ids_are_described_in_line_order_right_aligned_together() {
        let mut app = app_across_workspaces();
        app.closed_agent_ids
            .push_back(AgentId::parse("1234abcd").unwrap());
        let line = "8c1ee73d and 1234abcd and 8c1ee73d and 5a1b2c3d";
        // 47 cells of text and a gap of 2: the group gets `width - 49`.
        let group = "main-tracker · working  closed  pimig-002 · pi-migration · asking";
        let mut row = cells(line, 114);
        decorate(&app, &mut row);
        assert_eq!(row_text(&row), right(line, group, 114));

        // The last segment steps down the ladder before it is dropped.
        let mut row = cells(line, 113);
        decorate(&app, &mut row);
        let group = "main-tracker · working  closed  pimig-002 · asking";
        assert_eq!(row_text(&row), right(line, group, 113));
        let mut row = cells(line, 98);
        decorate(&app, &mut row);
        let group = "main-tracker · working  closed  asking";
        assert_eq!(row_text(&row), right(line, group, 98));

        // The first that does not fit ends the text.
        let mut row = cells(line, 78);
        decorate(&app, &mut row);
        assert_eq!(row_text(&row), right(line, "main-tracker · working", 78));
    }

    #[test]
    fn a_tabs_second_agent_goes_by_its_own_label_or_title() {
        let mut app = app_with_agent("8c1ee73d", None, AgentState::Working);
        let second = app.workspaces[0].test_split(Direction::Vertical);
        let terminal = set_agent(&mut app, 0, 0, second, "2b2b2b2b", None, AgentState::Idle);
        terminal.terminal_title = Some("pi task".into());
        let line = "8c1ee73d 2b2b2b2b";
        let mut row = cells(line, 70);
        decorate(&app, &mut row);
        let group = "main-tracker · working  pi task · idle";
        assert_eq!(row_text(&row), right(line, group, 70));

        let terminal_id = app.workspaces[0].tabs[0].panes[&second]
            .attached_terminal_id
            .clone();
        let terminal = app.terminals.get_mut(&terminal_id).unwrap();
        terminal.manual_label = Some("reviewer".into());
        let mut row = cells(line, 70);
        decorate(&app, &mut row);
        let group = "main-tracker · working  reviewer · idle";
        assert_eq!(row_text(&row), right(line, group, 70));

        // With nothing of its own it takes the tab's name, like the sidebar.
        let terminal = app.terminals.get_mut(&terminal_id).unwrap();
        terminal.manual_label = None;
        terminal.terminal_title = None;
        let mut row = cells(line, 70);
        decorate(&app, &mut row);
        let group = "main-tracker · working  main-tracker · idle";
        assert_eq!(row_text(&row), right(line, group, 70));
    }

    #[test]
    fn an_unnamed_tab_is_called_by_its_number() {
        let mut app = app_with_agent("8c1ee73d", None, AgentState::Working);
        app.workspaces[0].tabs[0].custom_name = None;
        let mut row = cells("8c1ee73d", 30);
        decorate(&app, &mut row);
        assert_eq!(row_text(&row), right("8c1ee73d", "1 · working", 30));
    }

    #[test]
    fn wide_graphemes_take_two_cells_in_the_text_and_the_line() {
        let mut app = app_with_agent("8c1ee73d", None, AgentState::Working);
        app.workspaces[0].tabs[0].custom_name = Some("日本".into());
        let mut row = cells("8c1ee73d", 30);
        decorate(&app, &mut row);
        // "日本 · working" is 14 cells wide; each wide grapheme covers a blank.
        let at = 30 - 14;
        assert_eq!(row[at].symbol, "日");
        assert_eq!(row[at + 1].symbol, " ");
        assert_eq!(row[at + 2].symbol, "本");
        assert_eq!(row_text(&row[at + 4..]), " · working");

        // A wide grapheme ending the text counts both cells toward the gap:
        // `💡` sits in cell 9 and covers cell 10, so the gap is cells 11 and 12.
        let app = app_with_agent("8c1ee73d", None, AgentState::Working);
        let mut row = cells("8c1ee73d 💡", 35);
        decorate(&app, &mut row);
        assert_eq!(row_text(&row[11..]), "  main-tracker · working");
        let mut row = cells("8c1ee73d 💡", 34);
        decorate(&app, &mut row);
        assert_eq!(row_text(&row[11..]), right("", "working", 23));
    }

    #[test]
    fn ids_match_bare_prefixed_and_bracketed_but_not_inside_longer_words() {
        let app = app_with_agent("8c1ee73d", Some(TaskStatus::Complete), AgentState::Idle);
        let cyan = crate::protocol::color_to_u32(Color::Cyan);
        for (text, start) in [("8c1ee73d", 0), ("loom:8c1ee73d", 5), ("[8c1ee73d]", 1)] {
            let mut row = cells(text, 40);
            decorate(&app, &mut row);
            assert_eq!(row[start].fg, cyan, "{text}");
            assert!(row_text(&row).ends_with("main-tracker · done"), "{text}");
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
    fn a_closed_agent_says_closed_alone() {
        let mut app = app_with_agent("8c1ee73d", None, AgentState::Working);
        app.closed_agent_ids
            .push_back(AgentId::parse("1234abcd").unwrap());
        let mut row = cells("1234abcd", 30);
        decorate(&app, &mut row);
        assert_eq!(row_text(&row), right("1234abcd", "closed", 30));
    }

    #[test]
    fn a_live_agent_wins_over_a_closed_record_of_its_id() {
        let mut app = app_with_agent("8c1ee73d", None, AgentState::Working);
        app.closed_agent_ids
            .push_back(AgentId::parse("8c1ee73d").unwrap());
        let mut row = cells("8c1ee73d", 40);
        decorate(&app, &mut row);
        assert_eq!(
            row_text(&row),
            right("8c1ee73d", "main-tracker · working", 40)
        );
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
        let mut buf = Buffer::with_lines(["|8c1ee73d                          |"]);
        let mut refs = AgentRefs::new(&app).unwrap();
        decorate_area(&mut buf, Rect::new(1, 0, 34, 1), &mut refs, 0);
        assert_eq!(buf[(1, 0)].fg, Color::Yellow);
        assert_eq!(buf[(0, 0)].fg, Color::Reset);
        let line: String = (0..36).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert_eq!(line, "|8c1ee73d    main-tracker · working|");
        assert!(buf[(13, 0)].modifier.contains(Modifier::DIM));
        assert!(!buf[(12, 0)].modifier.contains(Modifier::DIM));
    }
}
