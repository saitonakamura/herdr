//! The interactive sidebar: selection, modes, menus and actions. It never does
//! IO itself; it returns `Effect`s for the runner and takes results back.

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::buffer::Buffer;
use ratatui::style::{Modifier, Style};
use serde_json::json;
use unicode_width::UnicodeWidthStr;

use crate::api::{ApiError, SessionSnapshot};
use crate::config::Config;
use crate::keys::{Command, KeyTable};
use crate::layout::{self, cut, Line, Span, Target};
use crate::model::{self, Extras, Item, Scope, UiState};

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// A request whose only outcome worth showing is an error.
    Call(&'static str, serde_json::Value),
    Reply {
        agent_id: String,
        summary: String,
    },
    ClosePreview {
        agent_id: String,
        name: String,
    },
    Close {
        agent_id: String,
        name: String,
        force: bool,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
    Reply { agent_id: String, name: String },
    Tell { agent_id: String, name: String },
    Rename { workspace_id: String },
    Worktree { workspace_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    Rename(String),
    CloseWorkspace(String, String),
    NewWorktree(String),
    CollapseOthers(String),
    Reply(String, String),
    Tell(String, String),
    CloseSubtree(String, String),
    Scope(String),
    OpenUi(String, String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Search,
    Prompt {
        kind: PromptKind,
        input: String,
    },
    Confirm {
        lines: Vec<String>,
        on_yes: Box<Effect>,
    },
    Menu {
        key: String,
        entries: Vec<(String, MenuAction)>,
        selected: usize,
    },
}

/// What a screen row shows: a line of the list, or a line of the bottom overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    List(usize),
    Overlay,
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
        }
    }

    pub fn items(&self) -> Vec<Item> {
        model::build(&model::Inputs {
            snapshot: &self.snapshot,
            extras: &self.extras,
            ui: &self.ui,
            search: self.search.as_deref(),
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

    fn reply(&mut self, agent_id: String, name: String) -> Vec<Effect> {
        let vars = [("agent_id", agent_id.as_str()), ("name", name.as_str())];
        let effects = self.action("reply", &vars, Vec::new);
        if effects.is_empty() {
            self.mode = Mode::Prompt {
                kind: PromptKind::Reply { agent_id, name },
                input: String::new(),
            };
        }
        effects
    }

    fn tell(&mut self, agent_id: String, name: String) -> Vec<Effect> {
        let vars = [("agent_id", agent_id.as_str()), ("name", name.as_str())];
        let effects = self.action("tell", &vars, Vec::new);
        if effects.is_empty() {
            self.mode = Mode::Prompt {
                kind: PromptKind::Tell { agent_id, name },
                input: String::new(),
            };
        }
        effects
    }

    fn close(&mut self, agent_id: String, name: String) -> Vec<Effect> {
        let vars = [("agent_id", agent_id.as_str()), ("name", name.as_str())];
        self.action("close", &vars, || {
            vec![Effect::ClosePreview {
                agent_id: agent_id.clone(),
                name: name.clone(),
            }]
        })
    }

    fn open_ui(&self, agent_id: &str, name: &str) -> Vec<Effect> {
        self.action("ui", &[("agent_id", agent_id), ("name", name)], || {
            vec![Effect::Call(
                "plugin.pane.open",
                json!({ "plugin_id": "loom", "entrypoint": "ui", "placement": "popup", "focus": true }),
            )]
        })
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
            (Command::Reply, Item::Agent(a)) => match a.agent_id {
                Some(id) => self.reply(id, a.name),
                None => Vec::new(),
            },
            (Command::Tell, Item::Agent(a)) => match a.agent_id {
                Some(id) => self.tell(id, a.name),
                None => Vec::new(),
            },
            (Command::Close, Item::Agent(a)) => self.close(a.agent_id.unwrap_or(a.pane_id), a.name),
            (Command::OpenUi, Item::Agent(a)) => {
                self.open_ui(a.agent_id.as_deref().unwrap_or(&a.pane_id), &a.name)
            }
            (Command::Menu, item) => {
                self.open_menu(&item);
                Vec::new()
            }
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
            Item::Search(_) => Vec::new(),
        }
    }

    fn new_workspace(&self) -> Vec<Effect> {
        self.action("new_workspace", &[], || {
            vec![Effect::Call("workspace.create", json!({ "focus": true }))]
        })
    }

    fn open_menu(&mut self, item: &Item) {
        let entries: Vec<(String, MenuAction)> = match item {
            Item::Header(h) => vec![
                ("rename".into(), MenuAction::Rename(h.workspace_id.clone())),
                (
                    "close".into(),
                    MenuAction::CloseWorkspace(h.workspace_id.clone(), h.label.clone()),
                ),
                (
                    "new worktree".into(),
                    MenuAction::NewWorktree(h.workspace_id.clone()),
                ),
                (
                    "collapse others".into(),
                    MenuAction::CollapseOthers(h.workspace_id.clone()),
                ),
            ],
            Item::Agent(a) => {
                let id = a.agent_id.clone().unwrap_or_else(|| a.pane_id.clone());
                let mut entries = Vec::new();
                if a.agent_id.is_some() {
                    entries.push((
                        "reply".into(),
                        MenuAction::Reply(id.clone(), a.name.clone()),
                    ));
                    entries.push(("tell".into(), MenuAction::Tell(id.clone(), a.name.clone())));
                }
                entries.push((
                    "close subtree".into(),
                    MenuAction::CloseSubtree(id.clone(), a.name.clone()),
                ));
                if a.agent_id.is_some() {
                    entries.push(("scope to subtree".into(), MenuAction::Scope(id.clone())));
                }
                entries.push((
                    "open in loom ui".into(),
                    MenuAction::OpenUi(id, a.name.clone()),
                ));
                entries
            }
            _ => return,
        };
        self.selected = Some(item.key());
        self.mode = Mode::Menu {
            key: item.key(),
            entries,
            selected: 0,
        };
    }

    fn menu_choose(&mut self, action: MenuAction) -> Vec<Effect> {
        self.mode = Mode::Normal;
        match action {
            MenuAction::Rename(workspace_id) => {
                let label = self
                    .snapshot
                    .workspaces
                    .iter()
                    .find(|w| w.workspace_id == workspace_id)
                    .map(|w| w.label.clone())
                    .unwrap_or_default();
                let effects = self.action(
                    "rename",
                    &[("workspace_id", &workspace_id), ("name", &label)],
                    Vec::new,
                );
                if effects.is_empty() {
                    self.mode = Mode::Prompt {
                        kind: PromptKind::Rename { workspace_id },
                        input: label,
                    };
                }
                effects
            }
            MenuAction::CloseWorkspace(workspace_id, label) => {
                let vars = [
                    ("workspace_id", workspace_id.as_str()),
                    ("name", label.as_str()),
                ];
                let effects = self.action("close_workspace", &vars, Vec::new);
                if effects.is_empty() {
                    self.mode = Mode::Confirm {
                        lines: vec![format!("close workspace {label}?")],
                        on_yes: Box::new(Effect::Call(
                            "workspace.close",
                            json!({ "workspace_id": workspace_id }),
                        )),
                    };
                }
                effects
            }
            MenuAction::NewWorktree(workspace_id) => {
                let effects =
                    self.action("new_worktree", &[("workspace_id", &workspace_id)], Vec::new);
                if effects.is_empty() {
                    self.mode = Mode::Prompt {
                        kind: PromptKind::Worktree { workspace_id },
                        input: String::new(),
                    };
                }
                effects
            }
            MenuAction::CollapseOthers(workspace_id) => {
                self.ui.collapsed = self
                    .snapshot
                    .workspaces
                    .iter()
                    .map(|w| w.workspace_id.clone())
                    .filter(|id| *id != workspace_id)
                    .collect();
                self.ui.collapsed.remove(&workspace_id);
                vec![Effect::Persist]
            }
            MenuAction::Reply(id, name) => self.reply(id, name),
            MenuAction::Tell(id, name) => self.tell(id, name),
            MenuAction::CloseSubtree(id, name) => self.close(id, name),
            MenuAction::Scope(id) => self.toggle_scope(Scope::Subtree(id)),
            MenuAction::OpenUi(id, name) => self.open_ui(&id, &name),
        }
    }

    fn submit_prompt(&mut self, kind: PromptKind, input: String) -> Vec<Effect> {
        let text = input.trim().to_string();
        if text.is_empty() {
            return Vec::new();
        }
        match kind {
            PromptKind::Reply { agent_id, .. } => vec![Effect::Reply {
                agent_id,
                summary: text,
            }],
            PromptKind::Tell { agent_id, .. } => vec![Effect::Call(
                "message.send",
                json!({ "to": agent_id, "summary": text, "kind": "tell" }),
            )],
            PromptKind::Rename { workspace_id } => vec![Effect::Call(
                "workspace.rename",
                json!({ "workspace_id": workspace_id, "label": text }),
            )],
            PromptKind::Worktree { workspace_id } => vec![Effect::Call(
                "worktree.create",
                json!({ "workspace_id": workspace_id, "branch": text, "focus": true }),
            )],
        }
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
            Mode::Prompt { kind, mut input } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter => return self.submit_prompt(kind, input),
                KeyCode::Backspace => {
                    input.pop();
                    self.mode = Mode::Prompt { kind, input };
                }
                KeyCode::Char('u') if ctrl => {
                    input.clear();
                    self.mode = Mode::Prompt { kind, input };
                }
                KeyCode::Char(c) if !ctrl => {
                    input.push(c);
                    self.mode = Mode::Prompt { kind, input };
                }
                _ => self.mode = Mode::Prompt { kind, input },
            },
            Mode::Confirm { on_yes, .. } => {
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                    return vec![*on_yes];
                }
            }
            Mode::Menu {
                key: row,
                entries,
                selected,
            } => {
                let last = entries.len().saturating_sub(1);
                let selected = match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('m') => return Vec::new(),
                    KeyCode::Enter => {
                        let action = entries[selected.min(last)].1.clone();
                        return self.menu_choose(action);
                    }
                    KeyCode::Char('j') | KeyCode::Down => (selected + 1).min(last),
                    KeyCode::Char('k') | KeyCode::Up => selected.saturating_sub(1),
                    _ => selected,
                };
                self.mode = Mode::Menu {
                    key: row,
                    entries,
                    selected,
                };
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
            _ => return Vec::new(),
        }
        self.status = None;
        let right = matches!(event.kind, MouseEventKind::Down(MouseButton::Right));
        let Some(Row::List(index)) = self.rows.get(event.row as usize).copied() else {
            return Vec::new();
        };
        let line = self.lines[index].clone();
        let in_menu = matches!(self.mode, Mode::Menu { .. });
        if let Some(Target::Menu(choice)) = line.target_at(event.column) {
            if let Mode::Menu { entries, .. } = std::mem::replace(&mut self.mode, Mode::Normal) {
                if let Some((_, action)) = entries.get(choice) {
                    return self.menu_choose(action.clone());
                }
            }
            return Vec::new();
        }
        if in_menu || matches!(self.mode, Mode::Confirm { .. }) {
            self.mode = Mode::Normal;
            return Vec::new();
        }
        if right {
            if let Some(item) = self.item(&line.key) {
                self.open_menu(&item);
            }
            return Vec::new();
        }
        self.selected = Some(line.key.clone());
        match line.target_at(event.column) {
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
            Some(Target::Menu(_)) | None => Vec::new(),
        }
    }

    /// `herdr-sidebar ctl fold|scope [<pane>]`.
    pub fn ctl(&mut self, verb: &str, pane: Option<&str>) -> Result<Vec<Effect>, String> {
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
            other => Err(format!("unknown ctl verb {other}; expected fold or scope")),
        }
    }

    pub fn close_preview(
        &mut self,
        agent_id: String,
        name: String,
        result: Result<Vec<String>, ApiError>,
    ) {
        match result {
            Ok(names) => {
                let mut lines = vec![format!("close {name}: {} pane(s)", names.len())];
                lines.extend(names.into_iter().map(|n| format!("  {n}")));
                lines.push("[y/N]".into());
                self.mode = Mode::Confirm {
                    lines,
                    on_yes: Box::new(Effect::Close {
                        agent_id,
                        name,
                        force: false,
                    }),
                };
            }
            Err(err) => self.close_failed(agent_id, name, false, err),
        }
    }

    pub fn close_failed(&mut self, agent_id: String, name: String, forced: bool, err: ApiError) {
        if err.code == "agent_subtree_busy" && !forced {
            self.mode = Mode::Confirm {
                lines: vec![err.message, "still busy; close with --force? [y/N]".into()],
                on_yes: Box::new(Effect::Close {
                    agent_id,
                    name,
                    force: true,
                }),
            };
        } else {
            self.status = Some(format!("close {name}: {err}"));
        }
    }

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = Some(text.into());
    }

    fn overlay(&self, width: usize) -> Vec<(String, Style)> {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        match &self.mode {
            Mode::Prompt { kind, input } => {
                let label = match kind {
                    PromptKind::Reply { name, .. } => format!("reply {name}"),
                    PromptKind::Tell { name, .. } => format!("tell {name}"),
                    PromptKind::Rename { .. } => "rename".into(),
                    PromptKind::Worktree { .. } => "branch".into(),
                };
                // show the end of a long input, where the cursor is
                let prompt = format!("{label}> ");
                let room = width.saturating_sub(prompt.width() + 1);
                let shown: String = {
                    let chars: Vec<char> = input.chars().collect();
                    let mut start = chars.len();
                    let mut used = 0;
                    while start > 0 {
                        let w =
                            unicode_width::UnicodeWidthChar::width(chars[start - 1]).unwrap_or(0);
                        if used + w > room {
                            break;
                        }
                        used += w;
                        start -= 1;
                    }
                    chars[start..].iter().collect()
                };
                vec![(format!("{prompt}{shown}▏"), bold)]
            }
            Mode::Confirm { lines, .. } => lines.iter().map(|l| (cut(l, width), bold)).collect(),
            Mode::Search => vec![(
                cut(
                    &format!("/{}▏", self.search.as_deref().unwrap_or("")),
                    width,
                ),
                bold,
            )],
            _ => self
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
        let mut lines = layout::lines(&items, area.width, &self.config);
        if let Mode::Menu {
            key,
            entries,
            selected,
        } = &self.mode
        {
            if let Some(at) = lines.iter().rposition(|l| &l.key == key) {
                let menu: Vec<Line> = entries
                    .iter()
                    .enumerate()
                    .map(|(index, (label, _))| {
                        let style = if index == *selected {
                            Style::default().add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default().add_modifier(Modifier::BOLD)
                        };
                        Line {
                            spans: vec![Span {
                                text: cut(&format!("  › {label}"), width),
                                style,
                                target: Some(Target::Menu(index)),
                            }],
                            key: key.clone(),
                            target: Some(Target::Menu(index)),
                        }
                    })
                    .collect();
                lines.splice(at + 1..at + 1, menu);
            }
        }
        let overlay = self.overlay(width);
        let list_height = (area.height as usize).saturating_sub(overlay.len());

        let show_selection = self.focused || !matches!(self.mode, Mode::Normal);
        let selected_lines: Vec<usize> = match &self.selected {
            Some(key) => lines
                .iter()
                .enumerate()
                .filter(|(_, l)| &l.key == key && !matches!(l.target, Some(Target::Menu(_))))
                .map(|(i, _)| i)
                .collect(),
            None => Vec::new(),
        };
        let max_scroll = lines.len().saturating_sub(list_height);
        if self.follow {
            if let (Some(first), Some(last)) = (selected_lines.first(), selected_lines.last()) {
                let menu_end = if let Mode::Menu { entries, .. } = &self.mode {
                    last + entries.len()
                } else {
                    *last
                };
                if *first < self.scroll {
                    self.scroll = *first;
                } else if menu_end >= self.scroll + list_height {
                    self.scroll = (menu_end + 1).saturating_sub(list_height).min(*first);
                }
            }
        }
        self.scroll = self.scroll.min(max_scroll);

        buf.reset();
        self.rows = vec![Row::Overlay; area.height as usize];
        for row in 0..list_height {
            let index = self.scroll + row;
            let Some(line) = lines.get(index) else { break };
            self.rows[row] = Row::List(index);
            let y = area.y + row as u16;
            let mut x = area.x;
            let highlight = show_selection && selected_lines.contains(&index);
            for span in &line.spans {
                let style = if highlight {
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

    fn click(app: &mut App, col: u16, row: u16, button: MouseButton) -> Vec<Effect> {
        app.mouse(MouseEvent {
            kind: MouseEventKind::Down(button),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        })
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
    fn reply_prompts_then_sends_through_the_runner() {
        let mut app = app("");
        app.selected = Some("pane:w1:p3".into());
        assert!(press(&mut app, KeyCode::Char('r')).is_empty());
        typed(&mut app, "go on");
        let lines = draw(&mut app, 30, 12);
        assert_eq!(lines[11], "reply alpha> go on▏");
        assert_eq!(
            press(&mut app, KeyCode::Enter),
            [Effect::Reply {
                agent_id: "a".into(),
                summary: "go on".into()
            }]
        );
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn tell_sends_a_tell_as_the_user() {
        let mut app = app("");
        app.selected = Some("pane:w1:p4".into());
        press(&mut app, KeyCode::Char('t'));
        typed(&mut app, "hi");
        assert_eq!(
            press(&mut app, KeyCode::Enter),
            [Effect::Call(
                "message.send",
                json!({ "to": "b", "summary": "hi", "kind": "tell" })
            )]
        );
    }

    #[test]
    fn close_previews_confirms_and_offers_force_when_busy() {
        let mut app = app("");
        app.selected = Some("pane:w1:p3".into());
        assert_eq!(
            press(&mut app, KeyCode::Char('c')),
            [Effect::ClosePreview {
                agent_id: "a".into(),
                name: "alpha".into()
            }]
        );
        app.close_preview(
            "a".into(),
            "alpha".into(),
            Ok(vec!["alpha-one".into(), "alpha".into()]),
        );
        let lines = draw(&mut app, 30, 12);
        assert_eq!(lines[11], "[y/N]");
        assert_eq!(
            press(&mut app, KeyCode::Char('y')),
            [Effect::Close {
                agent_id: "a".into(),
                name: "alpha".into(),
                force: false
            }]
        );
        app.close_failed(
            "a".into(),
            "alpha".into(),
            false,
            ApiError {
                code: "agent_subtree_busy".into(),
                message: "alpha-one working".into(),
            },
        );
        assert_eq!(
            press(&mut app, KeyCode::Char('y')),
            [Effect::Close {
                agent_id: "a".into(),
                name: "alpha".into(),
                force: true
            }]
        );
        // anything but y declines
        app.close_preview("a".into(), "alpha".into(), Ok(vec![]));
        assert!(press(&mut app, KeyCode::Char('n')).is_empty());
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn action_overrides_replace_the_builtin() {
        let mut app = app(
            r#"actions = { close = { command = ["loom", "close", "{agent_id}"], popup = true } }"#,
        );
        app.selected = Some("pane:w1:p3".into());
        assert_eq!(
            press(&mut app, KeyCode::Char('c')),
            [Effect::Run {
                argv: vec!["loom".into(), "close".into(), "a".into()],
                popup: true
            }]
        );
    }

    #[test]
    fn the_default_ui_action_opens_looms_popup() {
        let mut app = app("");
        app.selected = Some("pane:w1:p3".into());
        let effects = press(&mut app, KeyCode::Char('p'));
        assert!(
            matches!(&effects[..], [Effect::Call("plugin.pane.open", params)] if params["plugin_id"] == "loom")
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
    fn right_click_opens_an_inline_menu_whose_entries_click() {
        let mut app = app("glyphs = \"unicode\"");
        let lines = draw(&mut app, 40, 20);
        let alpha = lines
            .iter()
            .position(|l| l.contains("alpha fork-030"))
            .unwrap();
        click(&mut app, 3, alpha as u16, MouseButton::Right);
        let lines = draw(&mut app, 40, 20);
        assert_eq!(lines[alpha + 1].trim(), "› reply");
        let scope = lines
            .iter()
            .position(|l| l.trim() == "› scope to subtree")
            .unwrap();
        assert_eq!(
            click(&mut app, 3, scope as u16, MouseButton::Left),
            [Effect::Persist]
        );
        assert_eq!(app.ui.scope, Some(Scope::Subtree("a".into())));
        let lines = draw(&mut app, 40, 20);
        assert!(lines[0].starts_with("◎ scope: alpha"));
    }

    #[test]
    fn header_menu_renames_through_a_prefilled_prompt() {
        let mut app = app("");
        app.selected = Some("ws:w2".into());
        press(&mut app, KeyCode::Char('m'));
        assert!(matches!(app.mode, Mode::Menu { .. }));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(&app.mode, Mode::Prompt { input, .. } if input == "elsewhere"));
        press(&mut app, KeyCode::Backspace);
        assert_eq!(
            press(&mut app, KeyCode::Enter),
            [Effect::Call(
                "workspace.rename",
                json!({ "workspace_id": "w2", "label": "elsewher" })
            )]
        );
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

    #[test]
    fn substitute_fills_placeholders() {
        let argv = vec!["x".to_string(), "{agent_id}:{name}".to_string()];
        assert_eq!(
            substitute(&argv, &[("agent_id", "a"), ("name", "n")]),
            ["x", "a:n"]
        );
    }
}
