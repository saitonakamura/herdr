//! The interactive sidebar: selection, search, folding and scope. A row's actions
//! live in its menu, a herdr popup (`menu`). It never does IO itself; it returns
//! `Effect`s for the runner and takes results back.

use crate::api::SessionSnapshot;
use crate::config::Config;
use crate::keys::{Command, KeyTable};
use crate::layout::{self, cut, Line, Target};
use crate::menu;
use crate::model::{self, Extras, Item, Scope, UiState};
use crate::reorder;
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::buffer::Buffer;
use ratatui::style::{Modifier, Style};
use serde_json::json;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// A request whose only outcome worth showing is an error.
    Call(&'static str, serde_json::Value),
    /// Open the row's menu in a popup; `refocus` when the sidebar holds keyboard focus.
    Menu {
        target: menu::Target,
        refocus: bool,
    },
    /// Hand keyboard focus back to the focused pane.
    Unfocus,
    /// A `[sidebar.actions]` override.
    Run {
        argv: Vec<String>,
        popup: bool,
    },
    Persist,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Search,
}

/// What a screen row shows: a line of the list, or a line of the bottom overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    List(usize),
    Overlay,
}

/// A left press on a workspace header (062): a click on release, unless it
/// became a drag by leaving the header's row.
#[derive(Debug, Clone, PartialEq)]
struct Press {
    workspace_id: String,
    key: String,
    /// What the press does as a click.
    target: Option<Target>,
    row: u16,
    /// The row a drag points at, once it is one.
    dragging: Option<u16>,
}

pub struct App {
    pub snapshot: SessionSnapshot,
    pub extras: Extras,
    pub ui: UiState,
    pub search: Option<String>,
    pub config: Config,
    pub mode: Mode,
    /// Whether the program holds keyboard focus, from focus reports.
    pub focused: bool,
    pub selected: Option<String>,
    pub scroll: usize,
    follow: bool,
    pub status: Option<String>,
    keys: KeyTable,
    lines: Vec<Line>,
    rows: Vec<Row>,
    height: u16,
    press: Option<Press>,
}

impl App {
    pub fn new(config: Config, ui: UiState) -> Self {
        let status = (!config.diagnostics.is_empty())
            .then(|| format!("config: {}", config.diagnostics.join("; ")));
        App {
            snapshot: SessionSnapshot::default(),
            extras: Extras::default(),
            ui,
            search: None,
            config,
            mode: Mode::Normal,
            focused: false,
            selected: None,
            scroll: 0,
            follow: true,
            status,
            keys: KeyTable::default(),
            lines: Vec::new(),
            rows: Vec::new(),
            height: 0,
            press: None,
        }
    }

    pub fn items(&self) -> Vec<Item> {
        model::build(&model::Inputs {
            snapshot: &self.snapshot,
            extras: &self.extras,
            ui: &self.ui,
            search: self.search.as_deref(),
            attention: self.config.attention,
        })
    }

    /// Takes a fresh snapshot; a scope on something gone is cleared.
    pub fn set_snapshot(&mut self, snapshot: SessionSnapshot) -> Vec<Effect> {
        self.snapshot = snapshot;
        let mut effects = Vec::new();
        if let Some(scope) = &self.ui.scope {
            if !model::scope_is_live(&self.snapshot, scope) {
                self.ui.scope = None;
                effects.push(Effect::Persist);
            }
        }
        effects
    }

    fn item(&self, key: &str) -> Option<Item> {
        self.items().into_iter().find(|item| item.key() == key)
    }

    fn selected_item(&self) -> Option<Item> {
        self.selected.as_deref().and_then(|key| self.item(key))
    }

    /// Selects the focused pane's row, or the first row, when nothing valid is selected.
    pub fn ensure_selection(&mut self) {
        let items = self.items();
        if self
            .selected
            .as_deref()
            .is_some_and(|key| items.iter().any(|item| item.key() == key))
        {
            return;
        }
        let focused = self
            .snapshot
            .focused_pane_id
            .as_ref()
            .map(|pane| format!("pane:{pane}"));
        self.selected = focused
            .filter(|key| items.iter().any(|item| &item.key() == key))
            .or_else(|| items.iter().find(|i| i.selectable()).map(Item::key));
        self.follow = true;
    }

    fn move_selection(&mut self, delta: isize) {
        let keys: Vec<String> = self
            .items()
            .iter()
            .filter(|item| item.selectable())
            .map(Item::key)
            .collect();
        if keys.is_empty() {
            return;
        }
        let at = self
            .selected
            .as_ref()
            .and_then(|key| keys.iter().position(|k| k == key));
        let next = match at {
            None => 0,
            Some(at) => (at as isize + delta).clamp(0, keys.len() as isize - 1) as usize,
        };
        self.selected = Some(keys[next].clone());
        self.follow = true;
    }

    fn select_edge(&mut self, last: bool) {
        let items = self.items();
        let mut keys = items.iter().filter(|i| i.selectable()).map(Item::key);
        self.selected = if last { keys.next_back() } else { keys.next() };
        self.follow = true;
    }

    fn action(
        &self,
        name: &str,
        vars: &[(&str, &str)],
        builtin: impl FnOnce() -> Vec<Effect>,
    ) -> Vec<Effect> {
        match self.config.action(name) {
            Some(action) => vec![Effect::Run {
                argv: substitute(&action.command, vars),
                popup: action.popup,
            }],
            None => builtin(),
        }
    }

    fn toggle_scope(&mut self, scope: Scope) -> Vec<Effect> {
        self.ui.scope = if self.ui.scope.as_ref() == Some(&scope) {
            None
        } else {
            Some(scope)
        };
        self.follow = true;
        vec![Effect::Persist]
    }

    fn toggle_fold(&mut self, key: &str) -> Vec<Effect> {
        if !self.ui.folded.remove(key) {
            self.ui.folded.insert(key.to_string());
        }
        vec![Effect::Persist]
    }

    fn toggle_section(&mut self, workspace_id: &str) -> Vec<Effect> {
        if !self.ui.collapsed.remove(workspace_id) {
            self.ui.collapsed.insert(workspace_id.to_string());
        }
        vec![Effect::Persist]
    }

    fn agent_has_children(&self, agent_id: &str) -> bool {
        model::subtree_ids(&self.snapshot, agent_id).len() > 1
    }

    fn parent_key(&self, pane_id: &str) -> Option<String> {
        let pane = self.snapshot.panes.iter().find(|p| p.pane_id == pane_id)?;
        let parent = pane.parent_agent_id.as_deref().and_then(|parent| {
            self.snapshot.panes.iter().find(|p| {
                p.agent_id.as_deref() == Some(parent) && p.workspace_id == pane.workspace_id
            })
        });
        Some(match parent {
            Some(parent) => format!("pane:{}", parent.pane_id),
            None => format!("ws:{}", pane.workspace_id),
        })
    }

    pub fn command(&mut self, command: Command) -> Vec<Effect> {
        let page = (self.height as isize / 2).max(1);
        match command {
            Command::Down => self.move_selection(1),
            Command::Up => self.move_selection(-1),
            Command::PageDown => self.move_selection(page),
            Command::PageUp => self.move_selection(-page),
            Command::Top => self.select_edge(false),
            Command::Bottom => self.select_edge(true),
            Command::Search => {
                self.search.get_or_insert_with(String::new);
                self.mode = Mode::Search;
            }
            Command::Escape => {
                if self.search.take().is_some() {
                    self.follow = true;
                } else if self.ui.scope.take().is_some() {
                    self.follow = true;
                    return vec![Effect::Persist];
                } else {
                    return vec![Effect::Unfocus];
                }
            }
            _ => {
                let Some(item) = self.selected_item() else {
                    return Vec::new();
                };
                return self.item_command(command, item);
            }
        }
        Vec::new()
    }

    fn item_command(&mut self, command: Command, item: Item) -> Vec<Effect> {
        if let Item::Attention(row) = &item {
            return self.attention_command(command, row);
        }
        match (command, item) {
            (Command::Activate, item) => self.activate(&item),
            (Command::Fold, Item::Header(h)) => {
                if self.ui.collapsed.insert(h.workspace_id) {
                    return vec![Effect::Persist];
                }
                Vec::new()
            }
            (Command::Unfold, Item::Header(h)) => {
                if self.ui.collapsed.remove(&h.workspace_id) {
                    return vec![Effect::Persist];
                }
                Vec::new()
            }
            (Command::ToggleFold, Item::Header(h)) => self.toggle_section(&h.workspace_id),
            (Command::Fold, Item::Agent(a)) => {
                if let Some(id) = a
                    .agent_id
                    .clone()
                    .filter(|id| self.agent_has_children(id) && !a.folded)
                {
                    self.ui.folded.insert(id);
                    return vec![Effect::Persist];
                }
                self.selected = self.parent_key(&a.pane_id);
                self.follow = true;
                Vec::new()
            }
            (Command::Fold, Item::Pane(p)) => {
                self.selected = Some(format!("ws:{}", p.workspace_id));
                self.follow = true;
                Vec::new()
            }
            (Command::Unfold, Item::Agent(a)) => match a.agent_id {
                Some(id) if self.ui.folded.remove(&id) => vec![Effect::Persist],
                _ => Vec::new(),
            },
            (Command::ToggleFold, Item::Agent(a)) => match a.agent_id {
                Some(id) if self.agent_has_children(&id) => self.toggle_fold(&id),
                _ => Vec::new(),
            },
            (Command::ToggleScope, _) if self.ui.scope.is_some() => {
                self.ui.scope = None;
                vec![Effect::Persist]
            }
            (Command::ToggleScope, Item::Header(h)) => {
                self.toggle_scope(Scope::Workspace(h.workspace_id))
            }
            (Command::ToggleScope, Item::Agent(a)) => match a.agent_id {
                Some(id) => self.toggle_scope(Scope::Subtree(id)),
                None => self.toggle_scope(Scope::Workspace(a.workspace_id)),
            },
            (Command::ToggleScope, Item::Pane(p)) => {
                self.toggle_scope(Scope::Workspace(p.workspace_id))
            }
            (Command::Menu, item) => self.open_menu(&item),
            _ => Vec::new(),
        }
    }

    /// An attention row focuses its agent and opens its menu as the tree row does;
    /// folding and scope belong to the trees.
    fn attention_command(&mut self, command: Command, row: &model::AttentionItem) -> Vec<Effect> {
        match command {
            Command::Activate => vec![focus_pane(&row.pane_id)],
            Command::Menu => self.menu_effect(menu::Target::Pane(row.pane_id.clone())),
            _ => Vec::new(),
        }
    }

    fn activate(&mut self, item: &Item) -> Vec<Effect> {
        match item {
            Item::Header(h) => vec![Effect::Call(
                "workspace.focus",
                json!({ "workspace_id": h.workspace_id }),
            )],
            Item::Agent(a) => vec![focus_pane(&a.pane_id)],
            Item::Pane(p) => vec![focus_pane(&p.pane_id)],
            Item::New => self.new_workspace(),
            Item::Scope(_) => {
                self.ui.scope = None;
                vec![Effect::Persist]
            }
            Item::Search(_) | Item::AttentionHeader(_) | Item::Attention(_) => Vec::new(),
        }
    }

    fn new_workspace(&self) -> Vec<Effect> {
        self.action("new_workspace", &[], || {
            vec![Effect::Call("workspace.create", json!({ "focus": true }))]
        })
    }

    fn menu_effect(&self, target: menu::Target) -> Vec<Effect> {
        vec![Effect::Menu {
            target,
            refocus: self.focused,
        }]
    }

    /// Agent rows and workspace headers have a menu; it opens in a herdr popup.
    fn open_menu(&mut self, item: &Item) -> Vec<Effect> {
        let target = match item {
            Item::Header(h) => menu::Target::Workspace(h.workspace_id.clone()),
            Item::Agent(a) => menu::Target::Pane(a.pane_id.clone()),
            Item::Attention(a) => menu::Target::Pane(a.pane_id.clone()),
            _ => return Vec::new(),
        };
        self.selected = Some(item.key());
        self.menu_effect(target)
    }

    pub fn key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if key.kind == KeyEventKind::Release {
            return Vec::new();
        }
        self.status = None;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Normal => {
                return match self.keys.feed(key) {
                    Some(command) => self.command(command),
                    None => Vec::new(),
                }
            }
            Mode::Search => {
                let query = self.search.get_or_insert_with(String::new);
                match key.code {
                    KeyCode::Esc => {
                        self.search = None;
                    }
                    KeyCode::Enter => {
                        if query.is_empty() {
                            self.search = None;
                        }
                        self.ensure_selection();
                        return Vec::new();
                    }
                    KeyCode::Backspace => {
                        query.pop();
                        self.mode = Mode::Search;
                    }
                    KeyCode::Char('u') if ctrl => {
                        query.clear();
                        self.mode = Mode::Search;
                    }
                    KeyCode::Char(c) if !ctrl => {
                        query.push(c);
                        self.mode = Mode::Search;
                    }
                    _ => self.mode = Mode::Search,
                }
                self.selected = None;
                self.ensure_selection();
            }
        }
        Vec::new()
    }

    pub fn mouse(&mut self, event: MouseEvent) -> Vec<Effect> {
        match event.kind {
            MouseEventKind::ScrollDown => {
                self.scroll = self.scroll.saturating_add(3);
                self.follow = false;
                return Vec::new();
            }
            MouseEventKind::ScrollUp => {
                self.scroll = self.scroll.saturating_sub(3);
                self.follow = false;
                return Vec::new();
            }
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Down(MouseButton::Right) => {}
            MouseEventKind::Drag(MouseButton::Left) => return self.drag(event.row),
            MouseEventKind::Up(MouseButton::Left) => return self.release(event.row),
            _ => return Vec::new(),
        }
        self.status = None;
        self.press = None;
        let right = matches!(event.kind, MouseEventKind::Down(MouseButton::Right));
        let Some(Row::List(index)) = self.rows.get(event.row as usize).copied() else {
            return Vec::new();
        };
        let line = self.lines[index].clone();
        // a spacer row belongs to no item and does nothing
        if line.target.is_none() && line.spans.iter().all(|s| s.target.is_none()) {
            return Vec::new();
        }
        if right {
            return match self.item(&line.key) {
                Some(item) => self.open_menu(&item),
                None => Vec::new(),
            };
        }
        self.selected = Some(line.key.clone());
        if let Some(id) = line.key.strip_prefix("ws:") {
            // a header acts on release, so pressing one can start a drag instead
            self.press = Some(Press {
                workspace_id: id.to_string(),
                key: line.key.clone(),
                target: line.target_at(event.column),
                row: event.row,
                dragging: None,
            });
            return Vec::new();
        }
        self.click(line.target_at(event.column))
    }

    /// Dragging moves workspaces only over the whole list: no scope or search.
    fn drag(&mut self, row: u16) -> Vec<Effect> {
        let reorderable = self.ui.scope.is_none() && self.search.is_none();
        let Some(press) = self.press.as_mut() else {
            return Vec::new();
        };
        if press.dragging.is_some()
            || (row != press.row
                && reorderable
                && reorder::movable(&self.snapshot.workspaces, &press.workspace_id))
        {
            press.dragging = Some(row);
        }
        Vec::new()
    }

    fn release(&mut self, row: u16) -> Vec<Effect> {
        let Some(press) = self.press.take() else {
            return Vec::new();
        };
        if press.dragging.is_none() {
            let same = match self.rows.get(row as usize) {
                Some(Row::List(index)) => {
                    self.lines.get(*index).map(|l| &l.key) == Some(&press.key)
                }
                _ => false,
            };
            return if same {
                self.click(press.target)
            } else {
                Vec::new()
            };
        }
        let Some(slot) = self.drop_slot(&self.lines, row) else {
            return Vec::new();
        };
        match reorder::request(
            &self.snapshot.workspaces,
            &press.workspace_id,
            slot.before.as_deref(),
        ) {
            Some((method, params)) => vec![Effect::Call(method, params)],
            None => Vec::new(),
        }
    }

    /// Where a header dropped at screen row `row` lands, among `lines` as drawn.
    fn drop_slot(&self, lines: &[Line], row: u16) -> Option<reorder::Slot> {
        let line = match self.rows.get(row as usize) {
            Some(Row::List(index)) => *index,
            // under the list: just past its last row on screen
            _ => self
                .rows
                .iter()
                .rev()
                .find_map(|r| match r {
                    Row::List(index) => Some(index + 1),
                    Row::Overlay => None,
                })
                .unwrap_or(lines.len()),
        };
        reorder::slot_at(&reorder::slots(lines, &self.snapshot.workspaces), line).cloned()
    }

    fn click(&mut self, target: Option<Target>) -> Vec<Effect> {
        match target {
            Some(Target::FocusPane(pane)) => vec![focus_pane(&pane)],
            Some(Target::FocusAgent(agent)) => {
                vec![Effect::Call("agent.focus", json!({ "target": agent }))]
            }
            Some(Target::FocusWorkspace(id)) => vec![Effect::Call(
                "workspace.focus",
                json!({ "workspace_id": id }),
            )],
            Some(Target::ToggleSection(id)) => self.toggle_section(&id),
            Some(Target::ToggleFold(id)) => self.toggle_fold(&id),
            Some(Target::NewWorkspace) => self.new_workspace(),
            Some(Target::ClearScope) => {
                self.ui.scope = None;
                vec![Effect::Persist]
            }
            None => Vec::new(),
        }
    }

    /// `herdr-sidebar ctl fold|scope|collapse-others [<pane>]`. collapse-others
    /// also takes a workspace id, and keeps that workspace (or the pane's) open.
    pub fn ctl(&mut self, verb: &str, pane: Option<&str>) -> Result<Vec<Effect>, String> {
        if verb == "collapse-others" {
            if let Some(ws) = pane.filter(|id| {
                self.snapshot
                    .workspaces
                    .iter()
                    .any(|w| w.workspace_id == *id)
            }) {
                return Ok(self.collapse_others(ws));
            }
        }
        let pane_id = pane
            .map(str::to_string)
            .or_else(|| self.snapshot.focused_pane_id.clone())
            .ok_or("no pane given and none is focused")?;
        let pane = self
            .snapshot
            .panes
            .iter()
            .find(|p| p.pane_id == pane_id || p.agent_id.as_deref() == Some(pane_id.as_str()))
            .ok_or_else(|| format!("no pane {pane_id}"))?
            .clone();
        match verb {
            "fold" => {
                let Some(agent) = pane.agent_id.clone() else {
                    return Ok(self.toggle_section(&pane.workspace_id));
                };
                // a leaf folds its parent, so the key always does something visible
                let key = if self.agent_has_children(&agent) {
                    agent
                } else {
                    pane.parent_agent_id.clone().unwrap_or(agent)
                };
                Ok(self.toggle_fold(&key))
            }
            "scope" => Ok(match pane.agent_id.clone() {
                Some(agent) => self.toggle_scope(Scope::Subtree(agent)),
                None => self.toggle_scope(Scope::Workspace(pane.workspace_id.clone())),
            }),
            "collapse-others" => Ok(self.collapse_others(&pane.workspace_id)),
            other => Err(format!(
                "unknown ctl verb {other}; expected fold, scope or collapse-others"
            )),
        }
    }

    fn collapse_others(&mut self, workspace_id: &str) -> Vec<Effect> {
        self.ui.collapsed = self
            .snapshot
            .workspaces
            .iter()
            .map(|w| w.workspace_id.clone())
            .filter(|id| id != workspace_id)
            .collect();
        vec![Effect::Persist]
    }

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = Some(text.into());
    }

    fn overlay(&self, width: usize) -> Vec<(String, Style)> {
        match &self.mode {
            Mode::Search => vec![(
                cut(
                    &format!("/{}▏", self.search.as_deref().unwrap_or("")),
                    width,
                ),
                Style::default().add_modifier(Modifier::BOLD),
            )],
            Mode::Normal => self
                .status
                .iter()
                .map(|s| (cut(s, width), Style::default().add_modifier(Modifier::DIM)))
                .collect(),
        }
    }

    /// Draws the whole pane into `buf` and remembers what each row is, for clicks.
    pub fn draw(&mut self, buf: &mut Buffer) {
        let area = buf.area;
        self.height = area.height;
        let width = area.width as usize;
        let items = self.items();
        let lines = layout::lines(&items, area.width, &self.config);
        let overlay = self.overlay(width);
        let list_height = (area.height as usize).saturating_sub(overlay.len());

        let show_selection = self.focused || !matches!(self.mode, Mode::Normal);
        let selected_lines: Vec<usize> = match &self.selected {
            Some(key) => lines
                .iter()
                .enumerate()
                .filter(|(_, l)| &l.key == key)
                .map(|(i, _)| i)
                .collect(),
            None => Vec::new(),
        };
        let max_scroll = lines.len().saturating_sub(list_height);
        if self.follow {
            if let (Some(first), Some(last)) = (selected_lines.first(), selected_lines.last()) {
                if *first < self.scroll {
                    self.scroll = *first;
                } else if *last >= self.scroll + list_height {
                    self.scroll = (last + 1).saturating_sub(list_height).min(*first);
                }
            }
        }
        self.scroll = self.scroll.min(max_scroll);

        buf.reset();
        self.rows = vec![Row::Overlay; area.height as usize];
        for row in 0..list_height {
            if lines.get(self.scroll + row).is_some() {
                self.rows[row] = Row::List(self.scroll + row);
            }
        }
        let dragged = self
            .press
            .as_ref()
            .and_then(|p| Some((p, p.dragging?)))
            .and_then(|(press, row)| Some((press.key.clone(), self.drop_slot(&lines, row)?)));
        let marker = dragged
            .as_ref()
            .map(|(_, slot)| reorder::marker(&lines, slot));
        for row in 0..list_height {
            let index = self.scroll + row;
            let Some(line) = lines.get(index) else { break };
            let y = area.y + row as u16;
            let mut x = area.x;
            if marker == Some(reorder::Marker::Rule(index)) {
                let label = dragged
                    .as_ref()
                    .and_then(|(key, _)| self.item(key))
                    .and_then(|item| match item {
                        Item::Header(h) => Some(h.label),
                        _ => None,
                    })
                    .unwrap_or_default();
                let text = cut(&format!("── {label} "), width);
                let fill = "─".repeat(width.saturating_sub(text.width()));
                buf.set_stringn(
                    x,
                    y,
                    format!("{text}{fill}"),
                    width,
                    Style::default().add_modifier(Modifier::BOLD),
                );
                continue;
            }
            let highlight = show_selection && selected_lines.contains(&index);
            let target = marker == Some(reorder::Marker::Header(index));
            let source = dragged.as_ref().is_some_and(|(key, _)| key == &line.key);
            for span in &line.spans {
                let style = if target {
                    span.style.add_modifier(Modifier::REVERSED)
                } else if source {
                    span.style.add_modifier(Modifier::DIM)
                } else if highlight {
                    span.style.bg(self.config.selection)
                } else {
                    span.style
                };
                let (next, _) =
                    buf.set_stringn(x, y, &span.text, (area.right() - x) as usize, style);
                x = next;
                if x >= area.right() {
                    break;
                }
            }
            if highlight && x < area.right() {
                let fill = " ".repeat((area.right() - x) as usize);
                buf.set_string(x, y, fill, Style::default().bg(self.config.selection));
            }
        }
        for (offset, (text, style)) in overlay.iter().enumerate() {
            let y = area.y + (list_height + offset) as u16;
            if y < area.bottom() {
                buf.set_stringn(area.x, y, text, width, *style);
            }
        }
        self.lines = lines;
    }
}

fn focus_pane(pane_id: &str) -> Effect {
    Effect::Call("pane.focus", json!({ "pane_id": pane_id }))
}

/// `{agent_id}`, `{pane_id}`, `{workspace_id}` and `{name}` in an override's argv.
pub fn substitute(argv: &[String], vars: &[(&str, &str)]) -> Vec<String> {
    argv.iter()
        .map(|arg| {
            let mut arg = arg.clone();
            for (name, value) in vars {
                arg = arg.replace(&format!("{{{name}}}"), value);
            }
            arg
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::from_table;
    use crate::model::fixtures::snapshot;
    use crossterm::event::KeyModifiers;
    use unicode_width::UnicodeWidthStr;

    fn app(config: &str) -> App {
        let mut app = App::new(from_table(&config.parse().unwrap()), UiState::default());
        app.set_snapshot(snapshot());
        app.focused = true;
        app
    }

    fn press(app: &mut App, code: KeyCode) -> Vec<Effect> {
        app.key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn typed(app: &mut App, text: &str) {
        for ch in text.chars() {
            press(app, KeyCode::Char(ch));
        }
    }

    fn draw(app: &mut App, width: u16, height: u16) -> Vec<String> {
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, width, height));
        app.draw(&mut buf);
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    fn mouse(app: &mut App, kind: MouseEventKind, col: u16, row: u16) -> Vec<Effect> {
        app.mouse(MouseEvent {
            kind,
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    /// A press and a release on the same cell.
    fn click(app: &mut App, col: u16, row: u16, button: MouseButton) -> Vec<Effect> {
        let mut effects = mouse(app, MouseEventKind::Down(button), col, row);
        effects.extend(mouse(app, MouseEventKind::Up(button), col, row));
        effects
    }

    /// Presses at `from`, drags through `path`, draws, and releases at the last row.
    fn drag(app: &mut App, from: u16, path: &[u16]) -> (Vec<String>, Vec<Effect>) {
        let mut effects = mouse(app, MouseEventKind::Down(MouseButton::Left), 4, from);
        for row in path {
            effects.extend(mouse(app, MouseEventKind::Drag(MouseButton::Left), 4, *row));
        }
        let drawn = draw(app, 40, 30);
        let last = *path.last().unwrap_or(&from);
        effects.extend(mouse(app, MouseEventKind::Up(MouseButton::Left), 4, last));
        (drawn, effects)
    }

    /// Three efforts: dogfood (w1, a tree), elsewhere (w2, one agent), quiet (w3, empty).
    fn three() -> App {
        let mut app = app("glyphs = \"unicode\"\nattention = false");
        let mut snap = snapshot();
        snap.workspaces
            .push(crate::model::fixtures::workspace("w3", "quiet"));
        app.set_snapshot(snap);
        app
    }

    fn row_of(lines: &[String], start: &str) -> u16 {
        lines.iter().position(|l| l.starts_with(start)).unwrap() as u16
    }

    #[test]
    fn dragging_a_header_above_another_moves_it_there() {
        let mut app = three();
        let lines = draw(&mut app, 40, 30);
        let (dogfood, quiet) = (row_of(&lines, "▾ dogfood"), row_of(&lines, "▾ quiet"));
        let (drawn, effects) = drag(&mut app, quiet, &[quiet - 1, dogfood]);
        assert_eq!(
            effects,
            [Effect::Call(
                "workspace.move",
                json!({ "workspace_id": "w3", "insert_index": 0 })
            )]
        );
        assert_eq!(drawn[dogfood as usize], lines[dogfood as usize]);
        // dogfood sits at the top with no gap above it, so its header lights up instead
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 4, quiet);
        mouse(
            &mut app,
            MouseEventKind::Drag(MouseButton::Left),
            4,
            dogfood,
        );
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 40, 30));
        app.draw(&mut buf);
        assert!(buf[(2, dogfood)].modifier.contains(Modifier::REVERSED));
        assert!(buf[(2, quiet)].modifier.contains(Modifier::DIM));
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 4, dogfood);
        // a drop onto elsewhere rules off the gap above it, naming what moves
        let elsewhere = row_of(&lines, "▾ elsewhere");
        let (drawn, effects) = drag(&mut app, quiet, &[elsewhere]);
        assert!(
            drawn[elsewhere as usize - 1].starts_with("── quiet ──"),
            "{drawn:#?}"
        );
        assert_eq!(
            effects,
            [Effect::Call(
                "workspace.move",
                json!({ "workspace_id": "w3", "insert_index": 1 })
            )]
        );
    }

    #[test]
    fn dropping_below_the_last_tree_moves_it_to_the_end() {
        let mut app = three();
        let lines = draw(&mut app, 40, 30);
        let new = lines.iter().position(|l| l == " + new").unwrap() as u16;
        let (drawn, effects) = drag(&mut app, 0, &[5, new]);
        assert!(
            drawn[new as usize - 1].starts_with("── dogfood ──"),
            "{drawn:#?}"
        );
        assert_eq!(
            effects,
            [Effect::Call(
                "workspace.move",
                json!({ "workspace_id": "w1", "insert_index": 3 })
            )]
        );
        // past the list too
        let (_, effects) = drag(&mut app, 0, &[29]);
        assert_eq!(effects.len(), 1);
        // the last one dropped after itself stays put
        let quiet = row_of(&lines, "▾ quiet");
        assert!(drag(&mut app, quiet, &[new]).1.is_empty());
    }

    #[test]
    fn a_press_and_release_on_one_header_stays_a_click() {
        let mut app = three();
        draw(&mut app, 40, 30);
        // the press alone does nothing yet
        assert!(mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 0, 0).is_empty());
        // moving along the header's own row is no drag
        assert!(mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 3, 0).is_empty());
        assert_eq!(
            mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 3, 0),
            [Effect::Persist]
        );
        assert!(app.ui.collapsed.contains("w1"));
        // a drag that comes back to its own header is a drop in place: nothing
        let lines = draw(&mut app, 40, 30);
        let elsewhere = row_of(&lines, "▾ elsewhere");
        assert!(drag(&mut app, elsewhere, &[elsewhere + 1, elsewhere])
            .1
            .is_empty());
        assert!(!app.ui.collapsed.contains("w2"));
    }

    #[test]
    fn a_scoped_or_searched_list_does_not_drag() {
        let mut app = three();
        app.ui.scope = Some(Scope::Workspace("w2".into()));
        let lines = draw(&mut app, 40, 30);
        let elsewhere = row_of(&lines, "▾ elsewhere");
        let (drawn, effects) = drag(&mut app, elsewhere, &[elsewhere + 3]);
        assert!(effects.is_empty());
        assert_eq!(drawn, lines);
    }

    #[test]
    fn movement_starts_at_the_focused_pane_and_enter_focuses_it() {
        let mut app = app("glyphs = \"unicode\"");
        app.ensure_selection();
        assert_eq!(app.selected.as_deref(), Some("pane:w1:p1"));
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.selected.as_deref(), Some("pane:w1:p3"));
        assert_eq!(
            press(&mut app, KeyCode::Enter),
            [Effect::Call("pane.focus", json!({ "pane_id": "w1:p3" }))]
        );
        press(&mut app, KeyCode::Char('G'));
        assert_eq!(app.selected.as_deref(), Some("new"));
        typed(&mut app, "gg");
        assert_eq!(app.selected.as_deref(), Some("ws:w1"));
    }

    #[test]
    fn escape_clears_search_then_scope_then_hands_focus_back() {
        let mut app = app("");
        app.ensure_selection();
        press(&mut app, KeyCode::Char('f'));
        assert!(app.ui.scope.is_some());
        press(&mut app, KeyCode::Char('/'));
        typed(&mut app, "x");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.search.as_deref(), Some("x"));
        assert!(press(&mut app, KeyCode::Esc).is_empty());
        assert_eq!(app.search, None);
        assert_eq!(press(&mut app, KeyCode::Esc), [Effect::Persist]);
        assert_eq!(app.ui.scope, None);
        assert_eq!(press(&mut app, KeyCode::Esc), [Effect::Unfocus]);
    }

    #[test]
    fn folding_keys_fold_nodes_and_sections() {
        let mut app = app("");
        app.selected = Some("pane:w1:p3".into());
        press(&mut app, KeyCode::Char('h'));
        assert!(app.ui.folded.contains("a"));
        press(&mut app, KeyCode::Char('l'));
        assert!(!app.ui.folded.contains("a"));
        typed(&mut app, "za");
        assert!(app.ui.folded.contains("a"));
        // h on a leaf moves to its parent
        app.selected = Some("pane:w1:p4".into());
        press(&mut app, KeyCode::Char('h'));
        assert_eq!(app.selected.as_deref(), Some("pane:w1:p1"));
        app.selected = Some("ws:w2".into());
        press(&mut app, KeyCode::Char('h'));
        assert!(app.ui.collapsed.contains("w2"));
    }

    #[test]
    fn action_overrides_replace_the_builtin() {
        let mut app =
            app(r#"actions = { new_workspace = { command = ["mk", "ws"], popup = true } }"#);
        app.selected = Some("new".into());
        assert_eq!(
            press(&mut app, KeyCode::Enter),
            [Effect::Run {
                argv: vec!["mk".into(), "ws".into()],
                popup: true
            }]
        );
    }

    #[test]
    fn the_row_above_new_is_a_dead_gap() {
        let mut app = app("glyphs = \"unicode\"");
        let lines = draw(&mut app, 40, 30);
        let new = lines.iter().position(|l| l.trim() == "+ new").unwrap();
        assert_eq!(lines[new - 1].trim(), "");
        let selected = app.selected.clone();
        for button in [MouseButton::Left, MouseButton::Right] {
            assert!(click(&mut app, 3, (new - 1) as u16, button).is_empty());
        }
        assert_eq!(app.selected, selected);
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(
            click(&mut app, 3, new as u16, MouseButton::Left),
            [Effect::Call("workspace.create", json!({ "focus": true }))]
        );
    }

    #[test]
    fn clicks_focus_rows_toggle_headers_and_follow_source_tokens() {
        let mut app = app("glyphs = \"unicode\"");
        let lines = draw(&mut app, 40, 20);
        assert!(lines[0].starts_with("▾ dogfood"));
        assert_eq!(click(&mut app, 0, 0, MouseButton::Left), [Effect::Persist]);
        assert!(app.ui.collapsed.contains("w1"));
        draw(&mut app, 40, 20);
        assert_eq!(
            click(&mut app, 4, 0, MouseButton::Left),
            [Effect::Call(
                "workspace.focus",
                json!({ "workspace_id": "w1" })
            )]
        );
        app.ui.collapsed.clear();
        let lines = draw(&mut app, 40, 20);
        let remote = lines.iter().position(|l| l.contains("remote")).unwrap();
        let col = lines[remote][..lines[remote].find('←').unwrap()].width() as u16;
        assert_eq!(
            click(&mut app, col, remote as u16, MouseButton::Left),
            [Effect::Call("agent.focus", json!({ "target": "a" }))]
        );
        assert_eq!(
            click(&mut app, 2, remote as u16, MouseButton::Left),
            [Effect::Call("pane.focus", json!({ "pane_id": "w2:p1" }))]
        );
        let new = lines.iter().position(|l| l == " + new").unwrap();
        assert_eq!(
            click(&mut app, 2, new as u16, MouseButton::Left),
            [Effect::Call("workspace.create", json!({ "focus": true }))]
        );
    }

    #[test]
    fn right_click_opens_the_rows_menu_in_a_popup() {
        let mut app = app("glyphs = \"unicode\"");
        app.focused = false;
        let lines = draw(&mut app, 40, 20);
        let alpha = lines
            .iter()
            .position(|l| l.contains("alpha fork-030"))
            .unwrap();
        assert_eq!(
            click(&mut app, 3, alpha as u16, MouseButton::Right),
            [Effect::Menu {
                target: menu::Target::Pane("w1:p3".into()),
                refocus: false
            }]
        );
        assert_eq!(app.selected.as_deref(), Some("pane:w1:p3"));
        assert_eq!(
            click(&mut app, 3, 0, MouseButton::Right),
            [Effect::Menu {
                target: menu::Target::Workspace("w1".into()),
                refocus: false
            }]
        );
        // the list draws no menu of its own
        assert_eq!(draw(&mut app, 40, 20), lines);
    }

    #[test]
    fn m_opens_the_menu_and_hands_focus_back_after_it() {
        let mut app = app("");
        app.selected = Some("ws:w2".into());
        assert_eq!(
            press(&mut app, KeyCode::Char('m')),
            [Effect::Menu {
                target: menu::Target::Workspace("w2".into()),
                refocus: true
            }]
        );
        // a shell pane's row has no menu
        app.selected = Some("pane:w1:p2".into());
        assert!(press(&mut app, KeyCode::Char('m')).is_empty());
    }

    #[test]
    fn the_wheel_scrolls_and_selection_stays_in_view() {
        let mut app = app("glyphs = \"unicode\"");
        draw(&mut app, 30, 4);
        app.mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        draw(&mut app, 30, 4);
        assert_eq!(app.scroll, 3);
        press(&mut app, KeyCode::Char('G'));
        let lines = draw(&mut app, 30, 4);
        assert_eq!(lines[3], " + new");
    }

    #[test]
    fn ctl_folds_and_scopes_by_pane() {
        let mut app = app("");
        app.ctl("fold", Some("w1:p3")).unwrap();
        assert!(app.ui.folded.contains("a"));
        // a leaf folds its parent
        app.ctl("fold", Some("w1:p4")).unwrap();
        assert!(app.ui.folded.contains("root"));
        app.ctl("scope", None).unwrap();
        assert_eq!(app.ui.scope, Some(Scope::Subtree("root".into())));
        app.ctl("scope", None).unwrap();
        assert_eq!(app.ui.scope, None);
        app.ctl("scope", Some("w1:p2")).unwrap();
        assert_eq!(app.ui.scope, Some(Scope::Workspace("w1".into())));
        app.ctl("collapse-others", Some("w2")).unwrap();
        assert_eq!(app.ui.collapsed, ["w1".to_string()].into());
        app.ctl("collapse-others", Some("w1:p3")).unwrap();
        assert_eq!(app.ui.collapsed, ["w2".to_string()].into());
        assert!(app.ctl("zoom", None).is_err());
        assert!(app.ctl("fold", Some("w9:p1")).is_err());
    }

    #[test]
    fn a_scope_whose_target_closes_is_cleared() {
        let mut app = app("");
        app.ui.scope = Some(Scope::Subtree("a".into()));
        let mut snap = snapshot();
        snap.panes.retain(|p| p.agent_id.as_deref() != Some("a"));
        assert_eq!(app.set_snapshot(snap), [Effect::Persist]);
        assert_eq!(app.ui.scope, None);
    }

    fn attention_app(config: &str) -> App {
        let mut app = App::new(from_table(&config.parse().unwrap()), UiState::default());
        app.set_snapshot(crate::model::fixtures::attention_snapshot());
        app.focused = true;
        app
    }

    const ACK: &str = r#"actions = { ack = { command = ["loom", "ack", "{agent_id}"] } }"#;

    #[test]
    fn the_menus_letters_do_nothing_at_the_top_level() {
        let mut app = app(ACK);
        app.selected = Some("pane:w1:p3".into());
        for key in ['r', 't', 'c', 'a', 'p'] {
            assert!(press(&mut app, KeyCode::Char(key)).is_empty(), "{key}");
            assert_eq!(app.mode, Mode::Normal);
        }
        // z a is still the fold chord
        typed(&mut app, "za");
        assert!(app.ui.folded.contains("a"));
    }

    #[test]
    fn attention_rows_focus_and_open_the_menu_like_tree_rows() {
        let mut app = attention_app(ACK);
        app.ensure_selection();
        // the header is skipped; the first row is the oldest loud one
        typed(&mut app, "gg");
        assert_eq!(app.selected.as_deref(), Some("attention:w1:p3"));
        assert_eq!(
            press(&mut app, KeyCode::Enter),
            [Effect::Call("pane.focus", json!({ "pane_id": "w1:p3" }))]
        );
        assert_eq!(
            press(&mut app, KeyCode::Char('m')),
            [Effect::Menu {
                target: menu::Target::Pane("w1:p3".into()),
                refocus: true
            }]
        );
        // folding and scope belong to the trees
        assert!(press(&mut app, KeyCode::Char('h')).is_empty());
        assert!(press(&mut app, KeyCode::Char('f')).is_empty());
        assert_eq!(app.ui, UiState::default());
    }

    #[test]
    fn the_attention_section_draws_on_top_and_its_rows_click_to_focus() {
        let mut app = attention_app("glyphs = \"unicode\"");
        let lines = draw(&mut app, 40, 30);
        assert_eq!(lines[0], "attention 4");
        assert!(lines[1].starts_with("▌●alpha ? dogfood ?2"), "{lines:?}");
        assert!(click(&mut app, 3, 0, MouseButton::Left).is_empty());
        for row in [3, 4] {
            assert_eq!(
                click(&mut app, 3, row, MouseButton::Left),
                [Effect::Call("pane.focus", json!({ "pane_id": "w1:p4" }))]
            );
        }
        assert_eq!(app.selected.as_deref(), Some("attention:w1:p4"));
        let mut off = attention_app("attention = false");
        let lines = draw(&mut off, 40, 30);
        assert!(lines[0].starts_with("▾ dogfood"), "{lines:?}");
    }

    #[test]
    fn substitute_fills_placeholders() {
        let argv = vec!["x".to_string(), "{agent_id}:{name}".to_string()];
        assert_eq!(
            substitute(&argv, &[("agent_id", "a"), ("name", "n")]),
            ["x", "a:n"]
        );
    }
}
