//! `herdr-sidebar menu`: a row's actions in a herdr popup (059). The popup holds
//! none of the sidebar's state. It reads its subject through herdr's API and
//! reaches the sidebar's own state (scope, collapsed sections) through `ctl`.
//! Keys and clicks only pick a `Job`; `Menu::work` runs it through `Io`, so the
//! model is tested with a stub and the runner draws "working" first.

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::buffer::Buffer;
use ratatui::style::{Modifier, Style};
use serde_json::{json, Value};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::api::{self, ApiError, MessageInfo, MessageKind, SessionSnapshot};
use crate::app::substitute;
use crate::config::Config;
use crate::glyphs::Meaning;
use crate::layout::{age_text, cut};
use crate::model;
use crate::report::{self, Spans};

/// What the menu is for: `herdr-sidebar menu pane <id>` or `menu workspace <id>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Pane(String),
    Workspace(String),
}

impl Target {
    pub fn parse(kind: &str, id: &str) -> Option<Target> {
        match kind {
            "pane" => Some(Target::Pane(id.to_string())),
            "workspace" => Some(Target::Workspace(id.to_string())),
            _ => None,
        }
    }

    pub fn args(&self) -> [String; 2] {
        match self {
            Target::Pane(id) => ["pane".into(), id.clone()],
            Target::Workspace(id) => ["workspace".into(), id.clone()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub message_id: u64,
    pub summary: String,
    pub body: Option<String>,
    pub sent_at_ms: Option<u64>,
}

impl Report {
    /// The summary's first line, with its kind but without loom's rows for the parent.
    fn first_line(&self) -> &str {
        let line = self.summary.lines().next().unwrap_or("");
        line.split(" ‖ ").next().unwrap_or(line).trim()
    }
}

/// An agent's latest report, from `message.list { from }`.
pub fn latest_report(messages: Vec<MessageInfo>) -> Option<Report> {
    messages
        .into_iter()
        .filter(|m| m.kind == MessageKind::Report)
        .max_by_key(|m| m.message_id)
        .map(|m| Report {
            message_id: m.message_id,
            summary: m.summary,
            body: m.body.filter(|b| !b.trim().is_empty()),
            sent_at_ms: m.sent_at_ms,
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    Agent {
        pane_id: String,
        agent_id: Option<String>,
        name: String,
        /// The unread report's kind (loom's `unseen` annotation).
        unread: Option<String>,
        report: Option<Report>,
    },
    Workspace {
        workspace_id: String,
        label: String,
    },
}

/// Finds the target in a snapshot; `report` reads an agent's latest report.
pub fn subject(
    snapshot: &SessionSnapshot,
    target: &Target,
    report: impl FnOnce(&str) -> Option<Report>,
) -> Result<Subject, String> {
    match target {
        Target::Workspace(id) => snapshot
            .workspaces
            .iter()
            .find(|w| &w.workspace_id == id)
            .map(|w| Subject::Workspace {
                workspace_id: w.workspace_id.clone(),
                label: w.label.clone(),
            })
            .ok_or_else(|| format!("no workspace {id}")),
        Target::Pane(id) => {
            let pane = snapshot
                .panes
                .iter()
                .find(|p| &p.pane_id == id)
                .ok_or_else(|| format!("no pane {id}"))?;
            let tab_labels = snapshot
                .tabs
                .iter()
                .map(|t| (t.tab_id.as_str(), t.label.as_str()))
                .collect();
            Ok(Subject::Agent {
                pane_id: pane.pane_id.clone(),
                agent_id: pane.agent_id.clone(),
                name: model::agent_name(pane, &tab_labels),
                unread: model::unread_kind(pane).map(str::to_string),
                report: pane.agent_id.as_deref().and_then(report),
            })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    OpenReport,
    Ack,
    Reply,
    Tell,
    Close,
    Scope,
    OpenUi,
    Rename,
    CloseWorkspace,
    NewWorktree,
    CollapseOthers,
}

impl Entry {
    pub fn key(self) -> char {
        match self {
            Entry::OpenReport => 'o',
            Entry::Ack => 'a',
            Entry::Reply | Entry::Rename => 'r',
            Entry::Tell => 't',
            Entry::Close | Entry::CloseWorkspace => 'c',
            Entry::Scope => 'f',
            Entry::OpenUi => 'p',
            Entry::NewWorktree => 'w',
            Entry::CollapseOthers => 'o',
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Entry::OpenReport => "open report",
            Entry::Ack => "ack",
            Entry::Reply => "reply",
            Entry::Tell => "tell",
            Entry::Close => "close",
            Entry::Scope => "scope to subtree",
            Entry::OpenUi => "open in loom ui",
            Entry::Rename => "rename",
            Entry::CloseWorkspace => "close",
            Entry::NewWorktree => "new worktree",
            Entry::CollapseOthers => "collapse others",
        }
    }
}

/// The entries a subject offers, in menu order.
pub fn entries(subject: &Subject) -> Vec<Entry> {
    match subject {
        Subject::Workspace { .. } => vec![
            Entry::Rename,
            Entry::CloseWorkspace,
            Entry::NewWorktree,
            Entry::CollapseOthers,
        ],
        Subject::Agent {
            agent_id,
            unread,
            report,
            ..
        } => {
            let mut entries = Vec::new();
            if report.is_some() {
                entries.push(Entry::OpenReport);
            }
            if unread.is_some() {
                entries.push(Entry::Ack);
            }
            if agent_id.is_some() {
                entries.extend([Entry::Reply, Entry::Tell]);
            }
            entries.push(Entry::Close);
            if agent_id.is_some() {
                entries.push(Entry::Scope);
            }
            entries.push(Entry::OpenUi);
            entries
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Struggled,
}

impl Outcome {
    fn word(self) -> &'static str {
        match self {
            Outcome::Ok => "ok",
            Outcome::Struggled => "struggled",
        }
    }
}

/// One `[sidebar.actions] close` run: loom's flags go after the override's argv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseRequest {
    pub outcome: Outcome,
    pub why: Option<String>,
    pub force: bool,
}

impl CloseRequest {
    fn args(&self) -> Vec<String> {
        let mut args = vec!["--outcome".into(), self.outcome.word().into()];
        if let Some(why) = &self.why {
            args.extend(["--why".into(), why.clone()]);
        }
        if self.force {
            args.push("--force".into());
        }
        args
    }
}

/// Why `loom close` refused. It prints no structured refusal, so this reads its
/// held lines, `  <name> [<id>] <reason>: …`, whose reason is loom's own token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The user still has part of the subtree; `(id, name, reason)` per node.
    Held(Vec<(String, String, String)>),
    /// Part of the subtree is working or blocked.
    Busy,
    /// Anything else: an error, not a refusal, so force is not offered.
    Error,
}

impl Refusal {
    /// Only unread reports hold it, so acking them is all closing needs.
    pub fn unread_only(&self) -> bool {
        matches!(self, Refusal::Held(nodes) if !nodes.is_empty() && nodes.iter().all(|(_, _, reason)| reason == "unseen"))
    }
}

pub fn refusal(stderr: &str) -> Refusal {
    if stderr.contains("loom: close: still working or blocked") {
        return Refusal::Busy;
    }
    if !stderr.contains("loom: close: the user still has it") {
        return Refusal::Error;
    }
    let nodes = stderr
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let open = line.find(" [")?;
            let close = open + line[open..].find("] ")?;
            let name = &line[..open];
            let id = &line[open + 2..close];
            let reason = line[close + 2..].split(':').next()?;
            (!id.is_empty() && !reason.contains(' '))
                .then(|| (id.to_string(), name.to_string(), reason.to_string()))
        })
        .collect();
    Refusal::Held(nodes)
}

/// A node whose unread report holds a close, shown before the user acks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unread {
    pub agent_id: String,
    pub pane_id: String,
    pub name: String,
    /// Its latest report's first line, which leads with loom's kind.
    pub line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    Reply,
    Tell,
    Rename,
    Worktree,
    /// `--why` for a struggled outcome.
    Why,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Menu,
    Report {
        scroll: usize,
    },
    Prompt {
        kind: Prompt,
        input: String,
    },
    Outcome,
    Confirm {
        lines: Vec<String>,
        yes: Job,
    },
    Refused {
        lines: Vec<String>,
        request: CloseRequest,
        /// Set when unread reports are all that hold the close.
        unread: Vec<Unread>,
    },
    Working(String),
}

/// IO the menu needs; `work` runs a job through it.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    /// Ack the subject's report, then quit or go back to the menu.
    Ack {
        then_quit: bool,
    },
    LoomClose(CloseRequest),
    AckAndClose(CloseRequest, Vec<Unread>),
    ClosePreview,
    Close {
        force: bool,
    },
    /// A request whose only outcome worth showing is an error; quits on success.
    Call(&'static str, Value),
    Reply(String),
    Ctl(&'static str, String),
    /// A detached `[sidebar.actions]` override; quits once started.
    Spawn(Vec<String>),
}

/// How the popup ends.
#[derive(Debug, Clone, PartialEq)]
pub enum Exit {
    Quit,
    /// Run this `popup = true` override in the popup's own terminal.
    Exec(Vec<String>),
    /// Make this request once the popup has closed: herdr opens one popup at a time.
    After(&'static str, Value),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

pub trait Io {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, ApiError>;
    /// Runs `argv` to completion. A program that can't start is `ok: false`.
    fn run(&mut self, argv: &[String]) -> Output;
    fn spawn(&mut self, argv: &[String]) -> Result<(), String>;
    /// `herdr-sidebar ctl <verb> <arg>` against the running sidebar.
    fn ctl(&mut self, verb: &str, arg: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq)]
enum Choice {
    Entry(Entry),
    Outcome(Outcome),
    Yes,
    No,
    AckAndClose,
    Force,
}

pub struct Menu {
    pub subject: Subject,
    pub config: Config,
    pub screen: Screen,
    pub selected: usize,
    pub status: Option<String>,
    pub job: Option<Job>,
    pub exit: Option<Exit>,
    /// When the menu opened, for the report's age.
    pub now_ms: u64,
    /// The screen row of each choice, from the last draw, for clicks.
    rows: Vec<Option<usize>>,
}

/// A body line and the choice it picks when clicked.
type Row = (Spans, Option<usize>);

const NO_ACK: &str = "ack: no [sidebar.actions] ack configured";

impl Menu {
    pub fn new(subject: Subject, config: Config) -> Self {
        Menu {
            subject,
            config,
            screen: Screen::Menu,
            selected: 0,
            status: None,
            job: None,
            exit: None,
            now_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
            rows: Vec::new(),
        }
    }

    fn name(&self) -> &str {
        match &self.subject {
            Subject::Agent { name, .. } => name,
            Subject::Workspace { label, .. } => label,
        }
    }

    /// The id loom and herdr know the agent by; a pane without one goes by its pane id.
    fn id(&self) -> &str {
        match &self.subject {
            Subject::Agent {
                agent_id, pane_id, ..
            } => agent_id.as_deref().unwrap_or(pane_id),
            Subject::Workspace { workspace_id, .. } => workspace_id,
        }
    }

    fn vars(&self) -> Vec<(&'static str, String)> {
        match &self.subject {
            Subject::Agent { pane_id, name, .. } => vec![
                ("agent_id", self.id().to_string()),
                ("pane_id", pane_id.clone()),
                ("name", name.clone()),
            ],
            Subject::Workspace {
                workspace_id,
                label,
            } => vec![
                ("workspace_id", workspace_id.clone()),
                ("name", label.clone()),
            ],
        }
    }

    fn override_argv(&self, action: &str, vars: &[(&str, String)]) -> Option<(Vec<String>, bool)> {
        let action = self.config.action(action)?;
        let vars: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (*k, v.as_str())).collect();
        Some((substitute(&action.command, &vars), action.popup))
    }

    /// Runs a `[sidebar.actions]` override the way its `popup` asks, if there is one.
    fn run_override(&mut self, action: &str) -> bool {
        let Some((argv, popup)) = self.override_argv(action, &self.vars()) else {
            return false;
        };
        if popup {
            self.exit = Some(Exit::Exec(argv));
        } else {
            self.job = Some(Job::Spawn(argv));
        }
        true
    }

    fn choices(&self) -> Vec<(char, String, Choice)> {
        match &self.screen {
            Screen::Menu => entries(&self.subject)
                .into_iter()
                .map(|e| (e.key(), e.label().to_string(), Choice::Entry(e)))
                .collect(),
            Screen::Outcome => vec![
                ('o', "ok".into(), Choice::Outcome(Outcome::Ok)),
                ('s', "struggled".into(), Choice::Outcome(Outcome::Struggled)),
            ],
            Screen::Confirm { .. } => vec![
                ('y', "yes".into(), Choice::Yes),
                ('n', "no".into(), Choice::No),
            ],
            Screen::Refused { unread, .. } => {
                let mut choices = Vec::new();
                if !unread.is_empty() {
                    choices.push(('a', "ack and close".into(), Choice::AckAndClose));
                }
                choices.push(('f', "force close".into(), Choice::Force));
                choices
            }
            Screen::Report { .. } | Screen::Prompt { .. } | Screen::Working(_) => Vec::new(),
        }
    }

    fn back(&mut self) {
        self.screen = Screen::Menu;
        self.selected = 0;
    }

    fn goto(&mut self, screen: Screen) {
        self.screen = screen;
        self.selected = 0;
    }

    fn choose(&mut self, choice: Choice) {
        match choice {
            Choice::Entry(entry) => self.entry(entry),
            Choice::Outcome(Outcome::Ok) => {
                self.start(Job::LoomClose(CloseRequest {
                    outcome: Outcome::Ok,
                    why: None,
                    force: false,
                }));
            }
            Choice::Outcome(Outcome::Struggled) => self.goto(Screen::Prompt {
                kind: Prompt::Why,
                input: String::new(),
            }),
            Choice::Yes => {
                if let Screen::Confirm { yes, .. } =
                    std::mem::replace(&mut self.screen, Screen::Menu)
                {
                    self.start(yes);
                }
            }
            Choice::No => self.back(),
            Choice::AckAndClose => {
                if self.config.action("ack").is_none() {
                    self.status = Some(NO_ACK.into());
                    return;
                }
                if let Screen::Refused {
                    request, unread, ..
                } = self.screen.clone()
                {
                    self.start(Job::AckAndClose(request, unread));
                }
            }
            Choice::Force => {
                if let Screen::Refused { request, .. } = self.screen.clone() {
                    self.start(Job::LoomClose(CloseRequest {
                        force: true,
                        ..request
                    }));
                }
            }
        }
    }

    fn start(&mut self, job: Job) {
        let what = match &job {
            Job::Ack { .. } => "acking".into(),
            Job::LoomClose(_) | Job::AckAndClose(..) | Job::Close { .. } => {
                format!("closing {}", self.name())
            }
            Job::ClosePreview => "reading what close takes".into(),
            _ => "working".into(),
        };
        self.screen = Screen::Working(what);
        self.selected = 0;
        self.job = Some(job);
    }

    fn entry(&mut self, entry: Entry) {
        let prompt = |kind| Screen::Prompt {
            kind,
            input: String::new(),
        };
        match entry {
            Entry::OpenReport => self.goto(Screen::Report { scroll: 0 }),
            Entry::Ack => {
                if self.config.action("ack").is_none() {
                    self.status = Some(NO_ACK.into());
                } else {
                    self.start(Job::Ack { then_quit: true });
                }
            }
            Entry::Reply => {
                if !self.run_override("reply") {
                    self.goto(prompt(Prompt::Reply));
                }
            }
            Entry::Tell => {
                if !self.run_override("tell") {
                    self.goto(prompt(Prompt::Tell));
                }
            }
            Entry::Close => {
                if self.config.action("close").is_some() {
                    self.goto(Screen::Outcome);
                } else {
                    self.start(Job::ClosePreview);
                }
            }
            Entry::Scope => {
                if let Subject::Agent { pane_id, .. } = &self.subject {
                    self.job = Some(Job::Ctl("scope", pane_id.clone()));
                }
            }
            Entry::OpenUi => {
                if !self.run_override("ui") {
                    self.exit = Some(Exit::After(
                        "plugin.pane.open",
                        json!({ "plugin_id": "loom", "entrypoint": "ui", "placement": "popup", "focus": true }),
                    ));
                }
            }
            Entry::Rename => {
                if !self.run_override("rename") {
                    self.goto(Screen::Prompt {
                        kind: Prompt::Rename,
                        input: self.name().to_string(),
                    });
                }
            }
            Entry::CloseWorkspace => {
                if !self.run_override("close_workspace") {
                    let lines = vec![format!("close workspace {}?", self.name())];
                    let yes = Job::Call("workspace.close", json!({ "workspace_id": self.id() }));
                    self.goto(Screen::Confirm { lines, yes });
                }
            }
            Entry::NewWorktree => {
                if !self.run_override("new_worktree") {
                    self.goto(prompt(Prompt::Worktree));
                }
            }
            Entry::CollapseOthers => {
                self.job = Some(Job::Ctl("collapse-others", self.id().to_string()));
            }
        }
    }

    fn submit(&mut self, kind: Prompt, input: String) {
        let text = input.trim().to_string();
        if text.is_empty() {
            self.screen = Screen::Prompt { kind, input };
            return;
        }
        let id = self.id().to_string();
        let job = match kind {
            Prompt::Reply => Job::Reply(text),
            Prompt::Tell => Job::Call(
                "message.send",
                json!({ "to": id, "summary": text, "kind": "tell" }),
            ),
            Prompt::Rename => Job::Call(
                "workspace.rename",
                json!({ "workspace_id": id, "label": text }),
            ),
            Prompt::Worktree => Job::Call(
                "worktree.create",
                json!({ "workspace_id": id, "branch": text, "focus": true }),
            ),
            Prompt::Why => Job::LoomClose(CloseRequest {
                outcome: Outcome::Struggled,
                why: Some(text),
                force: false,
            }),
        };
        self.start(job);
    }

    /// Leaving the report acks it: opening it is how the user reads it.
    fn close_report(&mut self) {
        let unread = matches!(
            &self.subject,
            Subject::Agent {
                unread: Some(_),
                ..
            }
        );
        if !unread {
            self.back();
        } else if self.config.action("ack").is_none() {
            self.status = Some(NO_ACK.into());
            self.back();
        } else {
            self.start(Job::Ack { then_quit: false });
        }
    }

    pub fn key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release || self.job.is_some() {
            return;
        }
        self.status = None;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match std::mem::replace(&mut self.screen, Screen::Menu) {
            Screen::Prompt { kind, mut input } => {
                match key.code {
                    KeyCode::Esc => return self.back(),
                    KeyCode::Enter => return self.submit(kind, input),
                    KeyCode::Backspace => {
                        input.pop();
                    }
                    KeyCode::Char('u') if ctrl => input.clear(),
                    KeyCode::Char(c) if !ctrl => input.push(c),
                    _ => {}
                }
                self.screen = Screen::Prompt { kind, input };
            }
            Screen::Report { scroll } => {
                let scroll = match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                        return self.close_report()
                    }
                    KeyCode::Char('j') | KeyCode::Down => scroll + 1,
                    KeyCode::Char('k') | KeyCode::Up => scroll.saturating_sub(1),
                    KeyCode::Char('d') if ctrl => scroll + 10,
                    KeyCode::Char(' ') | KeyCode::PageDown => scroll + 10,
                    KeyCode::Char('u') if ctrl => scroll.saturating_sub(10),
                    KeyCode::PageUp => scroll.saturating_sub(10),
                    KeyCode::Char('g') | KeyCode::Home => 0,
                    _ => scroll,
                };
                self.screen = Screen::Report { scroll };
            }
            Screen::Working(text) => self.screen = Screen::Working(text),
            screen => {
                let at_menu = screen == Screen::Menu;
                self.screen = screen;
                let choices = self.choices();
                let last = choices.len().saturating_sub(1);
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') if at_menu => self.exit = Some(Exit::Quit),
                    KeyCode::Esc | KeyCode::Char('q') => self.back(),
                    KeyCode::Char('j') | KeyCode::Down => {
                        self.selected = (self.selected + 1).min(last)
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.selected = self.selected.saturating_sub(1)
                    }
                    KeyCode::Enter => {
                        if let Some((_, _, choice)) = choices.into_iter().nth(self.selected) {
                            self.choose(choice);
                        }
                    }
                    KeyCode::Char(c) if !ctrl => {
                        if let Some((_, _, choice)) = choices.into_iter().find(|(k, _, _)| *k == c)
                        {
                            self.choose(choice);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn mouse(&mut self, event: MouseEvent) {
        if self.job.is_some() {
            return;
        }
        match event.kind {
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                if let Screen::Report { scroll } = &mut self.screen {
                    *scroll = if event.kind == MouseEventKind::ScrollDown {
                        *scroll + 3
                    } else {
                        scroll.saturating_sub(3)
                    };
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(Some(index)) = self.rows.get(event.row as usize).copied() else {
                    return;
                };
                self.status = None;
                if let Some((_, _, choice)) = self.choices().into_iter().nth(index) {
                    self.selected = index;
                    self.choose(choice);
                }
            }
            _ => {}
        }
    }

    /// Runs the pending job, if any.
    pub fn work(&mut self, io: &mut dyn Io) {
        let Some(job) = self.job.take() else {
            return;
        };
        match job {
            Job::Ack { then_quit } => match self.ack(io, &self.vars()) {
                Ok(()) => {
                    if let Subject::Agent { unread, .. } = &mut self.subject {
                        *unread = None;
                    }
                    if then_quit {
                        self.exit = Some(Exit::Quit);
                    } else {
                        self.back();
                        self.status = Some("acked".into());
                    }
                }
                Err(err) => {
                    self.back();
                    self.status = Some(err);
                }
            },
            Job::LoomClose(request) => self.loom_close(io, request),
            Job::AckAndClose(request, unread) => {
                for node in &unread {
                    let vars = [
                        ("agent_id", node.agent_id.clone()),
                        ("pane_id", node.pane_id.clone()),
                        ("name", node.name.clone()),
                    ];
                    if let Err(err) = self.ack(io, &vars) {
                        self.back();
                        self.status = Some(err);
                        return;
                    }
                    if node.agent_id == self.id() {
                        if let Subject::Agent { unread, .. } = &mut self.subject {
                            *unread = None;
                        }
                    }
                }
                self.loom_close(io, request);
            }
            Job::ClosePreview => {
                let params = json!({ "targets": [self.id()], "dry_run": true });
                match io.call("agent.close", params) {
                    Ok(result) => {
                        let names = closed_names(&result);
                        let mut lines =
                            vec![format!("close {}: {} pane(s)", self.name(), names.len())];
                        lines.extend(names.into_iter().map(|n| format!("  {n}")));
                        self.goto(Screen::Confirm {
                            lines,
                            yes: Job::Close { force: false },
                        });
                    }
                    Err(err) => self.close_failed(false, err),
                }
            }
            Job::Close { force } => {
                let params = json!({ "targets": [self.id()], "force": force });
                match io.call("agent.close", params) {
                    Ok(_) => self.exit = Some(Exit::Quit),
                    Err(err) => self.close_failed(force, err),
                }
            }
            Job::Call(method, params) => match io.call(method, params) {
                Ok(_) => self.exit = Some(Exit::Quit),
                Err(err) => {
                    self.back();
                    self.status = Some(format!("{method}: {err}"));
                }
            },
            Job::Reply(text) => {
                let id = self.id().to_string();
                let mut params = json!({ "to": id, "summary": text, "kind": "reply" });
                // a reply answers the latest report, read now: it may be newer than the menu's
                let latest = io
                    .call("message.list", json!({ "from": id }))
                    .ok()
                    .and_then(|r| latest_report(api::messages_from_result(&r)));
                if let Some(report) = latest {
                    params["reply_to"] = json!(report.message_id);
                }
                match io.call("message.send", params) {
                    Ok(_) => self.exit = Some(Exit::Quit),
                    Err(err) => {
                        self.back();
                        self.status = Some(format!("reply: {err}"));
                    }
                }
            }
            Job::Ctl(verb, arg) => match io.ctl(verb, &arg) {
                Ok(()) => self.exit = Some(Exit::Quit),
                Err(err) => {
                    self.back();
                    self.status = Some(format!("{verb}: {err}"));
                }
            },
            Job::Spawn(argv) => match io.spawn(&argv) {
                Ok(()) => self.exit = Some(Exit::Quit),
                Err(err) => {
                    self.back();
                    self.status = Some(format!("{}: {err}", argv[0]));
                }
            },
        }
    }

    /// Runs `[sidebar.actions] ack` for one agent and waits for it.
    fn ack(&self, io: &mut dyn Io, vars: &[(&'static str, String)]) -> Result<(), String> {
        let (argv, _) = self.override_argv("ack", vars).ok_or(NO_ACK)?;
        let out = io.run(&argv);
        if out.ok {
            Ok(())
        } else {
            Err(format!("ack: {}", first_line(&out.stderr, &out.stdout)))
        }
    }

    fn loom_close(&mut self, io: &mut dyn Io, request: CloseRequest) {
        let Some((mut argv, _)) = self.override_argv("close", &self.vars()) else {
            return self.back();
        };
        argv.extend(request.args());
        let out = io.run(&argv);
        if out.ok {
            self.exit = Some(Exit::Quit);
            return;
        }
        let refusal = refusal(&out.stderr);
        let lines: Vec<String> = out
            .stderr
            .lines()
            .chain(out.stdout.lines())
            .map(str::to_string)
            .filter(|l| !l.trim().is_empty())
            .collect();
        match refusal {
            Refusal::Error => {
                self.back();
                self.status = Some(format!("close: {}", first_line(&out.stderr, &out.stdout)));
            }
            Refusal::Busy if request.force => {
                self.back();
                self.status = Some(format!("close: {}", first_line(&out.stderr, &out.stdout)));
            }
            refusal => {
                let unread = if refusal.unread_only() {
                    self.unread_nodes(io, &refusal)
                } else {
                    Vec::new()
                };
                self.goto(Screen::Refused {
                    lines,
                    request: CloseRequest {
                        force: false,
                        ..request
                    },
                    unread,
                });
            }
        }
    }

    /// The held nodes with their latest reports, so the user never acks blind.
    fn unread_nodes(&self, io: &mut dyn Io, refusal: &Refusal) -> Vec<Unread> {
        let Refusal::Held(nodes) = refusal else {
            return Vec::new();
        };
        let snapshot = io
            .call("session.snapshot", json!({}))
            .ok()
            .and_then(|r| api::snapshot_from_result(&r).ok())
            .unwrap_or_default();
        nodes
            .iter()
            .map(|(id, name, _)| {
                let pane_id = snapshot
                    .panes
                    .iter()
                    .find(|p| p.agent_id.as_deref() == Some(id.as_str()))
                    .map(|p| p.pane_id.clone())
                    .unwrap_or_default();
                let line = io
                    .call("message.list", json!({ "from": id }))
                    .ok()
                    .and_then(|r| latest_report(api::messages_from_result(&r)))
                    .map(|r| r.first_line().to_string())
                    .unwrap_or_else(|| "(no report found)".into());
                Unread {
                    agent_id: id.clone(),
                    pane_id,
                    name: name.clone(),
                    line,
                }
            })
            .collect()
    }

    fn close_failed(&mut self, forced: bool, err: ApiError) {
        if err.code == "agent_subtree_busy" && !forced {
            self.goto(Screen::Confirm {
                lines: vec![err.message, "still busy; close with --force?".into()],
                yes: Job::Close { force: true },
            });
        } else {
            self.back();
            self.status = Some(format!("close: {err}"));
        }
    }

    /// The report screen's header: name, the kind's glyph in its colour, kind, age.
    fn report_header(&self) -> Spans {
        let Subject::Agent {
            name,
            unread,
            report,
            ..
        } = &self.subject
        else {
            return Vec::new();
        };
        let kind = report
            .as_ref()
            .and_then(|r| report::summary_kind(&r.summary))
            .or(unread.as_deref())
            .unwrap_or("report");
        let glyph = self.config.glyphs.get(Meaning::for_unread(kind));
        let mut glyph_style = Style::default();
        if let Some(color) = glyph.color {
            glyph_style = glyph_style.fg(color);
        }
        let mut spans = vec![
            (name.clone(), Style::default().add_modifier(Modifier::BOLD)),
            (" ".into(), Style::default()),
            (glyph.text.trim().to_string(), glyph_style),
            (format!(" {kind}"), Style::default()),
        ];
        if let Some(sent) = report.as_ref().and_then(|r| r.sent_at_ms) {
            let age = age_text(self.now_ms.saturating_sub(sent) / 1000);
            spans.push((
                format!(" · {age}"),
                Style::default().add_modifier(Modifier::DIM),
            ));
        }
        spans
    }

    fn header(&self) -> (String, Option<(String, bool)>) {
        match &self.subject {
            Subject::Agent {
                name,
                unread,
                report,
                ..
            } => {
                let line = report
                    .as_ref()
                    .map(|r| r.first_line().to_string())
                    .unwrap_or_else(|| "no report yet".into());
                let unread = unread.is_some();
                let line = if unread {
                    format!("unread · {line}")
                } else {
                    line
                };
                (name.clone(), Some((line, unread)))
            }
            Subject::Workspace { label, .. } => (format!("workspace {label}"), None),
        }
    }

    fn body(&self, width: usize) -> Vec<Row> {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let plain = Style::default();
        let dim = Style::default().add_modifier(Modifier::DIM);
        let mut out: Vec<Row> = Vec::new();
        let text = |out: &mut Vec<Row>, s: &str, style: Style| {
            for line in wrap(s, width) {
                out.push((vec![(line, style)], None));
            }
        };
        match &self.screen {
            Screen::Menu | Screen::Outcome | Screen::Confirm { .. } | Screen::Refused { .. } => {
                if let Screen::Outcome = self.screen {
                    text(
                        &mut out,
                        &format!("close {}: how did it go?", self.name()),
                        bold,
                    );
                }
                if let Screen::Confirm { lines, .. } = &self.screen {
                    for line in lines {
                        text(&mut out, line, bold);
                    }
                }
                if let Screen::Refused { lines, unread, .. } = &self.screen {
                    for line in lines {
                        text(&mut out, line, plain);
                    }
                    if !unread.is_empty() {
                        out.push((Vec::new(), None));
                        text(&mut out, "only unread reports hold it:", bold);
                        for node in unread {
                            text(&mut out, &format!("  {}: {}", node.name, node.line), plain);
                        }
                    }
                }
                if !matches!(self.screen, Screen::Menu) {
                    out.push((Vec::new(), None));
                }
                for (index, (key, label, _)) in self.choices().into_iter().enumerate() {
                    let style = if index == self.selected {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        plain
                    };
                    let line = cut(&format!(" {key}  {label}"), width);
                    let pad = width.saturating_sub(line.width());
                    out.push((
                        vec![(format!("{line}{}", " ".repeat(pad)), style)],
                        Some(index),
                    ));
                }
            }
            Screen::Report { .. } => {
                if let Subject::Agent {
                    report: Some(report),
                    ..
                } = &self.subject
                {
                    let text = report::text(&report.summary, report.body.as_deref());
                    out.extend(report::render(&text, width).into_iter().map(|l| (l, None)));
                }
            }
            Screen::Prompt { kind, input } => {
                let label = match kind {
                    Prompt::Reply => format!("reply {}", self.name()),
                    Prompt::Tell => format!("tell {}", self.name()),
                    Prompt::Rename => "rename".into(),
                    Prompt::Worktree => "branch".into(),
                    Prompt::Why => "struggled, why".into(),
                };
                text(&mut out, &format!("{label}> {input}▏"), bold);
            }
            Screen::Working(what) => text(&mut out, &format!("{what}…"), dim),
        }
        out
    }

    fn hint(&self) -> &'static str {
        match self.screen {
            Screen::Menu => "key or enter runs · esc closes",
            Screen::Report { .. } => "j/k scroll · esc closes and acks",
            Screen::Prompt { .. } => "enter sends · esc back",
            Screen::Working(_) => "",
            _ => "key or enter runs · esc back",
        }
    }

    pub fn draw(&mut self, buf: &mut Buffer) {
        let area = buf.area;
        let width = area.width.saturating_sub(2) as usize;
        let height = area.height as usize;
        buf.reset();
        self.rows = vec![None; height];
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let dim = Style::default().add_modifier(Modifier::DIM);
        let mut top: Vec<Spans> = if let Screen::Report { .. } = self.screen {
            vec![self.report_header()]
        } else {
            let (title, line) = self.header();
            let mut top = vec![vec![(cut(&title, width), bold)]];
            if let Some((line, unread)) = line {
                top.push(vec![(cut(&line, width), if unread { bold } else { dim })]);
            }
            top
        };
        top.push(vec![("─".repeat(width), dim)]);
        // a status wraps to two lines, so an error is read whole; the hint is cut to one
        let footer: Vec<String> = match &self.status {
            Some(status) => {
                let mut lines = wrap(status, width);
                lines.truncate(2);
                lines
            }
            None => vec![cut(self.hint(), width)],
        };
        let room = height.saturating_sub(top.len() + footer.len());
        let mut body = self.body(width);
        if let Screen::Report { scroll } = &mut self.screen {
            *scroll = (*scroll).min(body.len().saturating_sub(room));
            body.drain(..*scroll);
        }
        let x = area.x + 1;
        let put = |buf: &mut Buffer, row: usize, spans: &Spans| {
            let mut at = x;
            for (text, style) in spans {
                let left = (x as usize + width).saturating_sub(at as usize);
                at = buf
                    .set_stringn(at, area.y + row as u16, text, left, *style)
                    .0;
            }
        };
        for (row, spans) in top.iter().enumerate() {
            if row < height {
                put(buf, row, spans);
            }
        }
        for (offset, (spans, choice)) in body.into_iter().take(room).enumerate() {
            let row = top.len() + offset;
            put(buf, row, &spans);
            self.rows[row] = choice;
        }
        let style = if self.status.is_some() { bold } else { dim };
        for (offset, line) in footer.iter().enumerate() {
            let row = height.saturating_sub(footer.len()) + offset;
            if row >= top.len() {
                buf.set_stringn(x, area.y + row as u16, cut(line, width), width, style);
            }
        }
    }
}

fn first_line(stderr: &str, stdout: &str) -> String {
    stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("failed")
        .to_string()
}

pub fn closed_names(result: &Value) -> Vec<String> {
    result
        .get("agents")
        .and_then(|agents| agents.as_array())
        .map(|agents| {
            agents
                .iter()
                .map(|agent| {
                    agent["name"]
                        .as_str()
                        .or(agent["agent_id"].as_str())
                        .or(agent["pane_id"].as_str())
                        .unwrap_or("?")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Word-wraps to `width` columns, keeping blank lines; a word wider than a line is cut.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for paragraph in text.lines() {
        // a paragraph's own indent is kept on its first line
        let indent = &paragraph[..paragraph.len() - paragraph.trim_start().len()];
        let indent = if indent.width() < width / 2 {
            indent
        } else {
            ""
        };
        let mut line = indent.to_string();
        let mut used = indent.width();
        for word in paragraph.split_whitespace() {
            let w = word.width();
            if line.trim().is_empty() {
                // nothing but the indent yet: the word starts here
            } else if used + 1 + w > width {
                out.push(std::mem::take(&mut line));
                used = 0;
            }
            if w > width {
                let mut chunk = String::new();
                let mut cw = 0;
                for ch in word.chars() {
                    let c = ch.width().unwrap_or(0);
                    if cw + c > width {
                        out.push(std::mem::take(&mut chunk));
                        cw = 0;
                    }
                    chunk.push(ch);
                    cw += c;
                }
                line = chunk;
                used = cw;
                continue;
            }
            if !line.trim().is_empty() {
                line.push(' ');
                used += 1;
            }
            line.push_str(word);
            used += w;
        }
        out.push(line);
    }
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::config::from_table;
    use crate::model::fixtures::attention_snapshot;

    /// Records what the menu asked for and answers from a script.
    #[derive(Default)]
    pub struct Stub {
        pub calls: Vec<(String, Value)>,
        pub runs: Vec<Vec<String>>,
        pub spawned: Vec<Vec<String>>,
        pub ctls: Vec<(String, String)>,
        /// Answers to `run`, in order; past the end every run succeeds.
        pub outputs: Vec<Output>,
        pub close_error: Option<ApiError>,
    }

    impl Io for Stub {
        fn call(&mut self, method: &str, params: Value) -> Result<Value, ApiError> {
            self.calls.push((method.to_string(), params.clone()));
            match method {
                "session.snapshot" => Ok(json!({ "snapshot": snapshot_json() })),
                "message.list" => Ok(json!({ "messages": [
                    { "message_id": 7, "kind": "report", "to_agent_id": "root", "summary": format!("done: {} finished", params["from"].as_str().unwrap_or("?")), "sent_at_ms": 1 }
                ]})),
                "agent.close" if params.get("dry_run").is_some() => {
                    Ok(json!({ "agents": [{ "name": "alpha-one" }, { "name": "alpha" }] }))
                }
                "agent.close" => match self.close_error.take() {
                    Some(err) => Err(err),
                    None => Ok(json!({ "agents": [] })),
                },
                _ => Ok(json!({})),
            }
        }

        fn run(&mut self, argv: &[String]) -> Output {
            self.runs.push(argv.to_vec());
            if self.outputs.is_empty() {
                return Output {
                    ok: true,
                    stdout: String::new(),
                    stderr: String::new(),
                };
            }
            self.outputs.remove(0)
        }

        fn spawn(&mut self, argv: &[String]) -> Result<(), String> {
            self.spawned.push(argv.to_vec());
            Ok(())
        }

        fn ctl(&mut self, verb: &str, arg: &str) -> Result<(), String> {
            self.ctls.push((verb.to_string(), arg.to_string()));
            Ok(())
        }
    }

    fn snapshot_json() -> Value {
        let snap = attention_snapshot();
        json!({
            "workspaces": snap.workspaces.iter().map(|w| json!({ "workspace_id": w.workspace_id, "label": w.label, "focused": false })).collect::<Vec<_>>(),
            "tabs": [],
            "panes": snap.panes.iter().map(|p| json!({
                "pane_id": p.pane_id, "workspace_id": p.workspace_id, "tab_id": p.tab_id,
                "focused": false, "agent_status": "idle", "agent_id": p.agent_id,
            })).collect::<Vec<_>>(),
        })
    }

    pub const LOOM: &str = r#"
        [actions]
        close = { command = ["loom", "close", "{agent_id}"], popup = true }
        ack = { command = ["loom", "ack", "{agent_id}"] }
    "#;

    fn report(summary: &str) -> Report {
        Report {
            message_id: 7,
            summary: summary.into(),
            body: Some("the long part\nsecond line".into()),
            sent_at_ms: Some(NOW_MS - 4 * 60_000),
        }
    }

    const NOW_MS: u64 = 1_762_000_000_000;

    pub fn agent(unread: Option<&str>) -> Subject {
        Subject::Agent {
            pane_id: "w1:p3".into(),
            agent_id: Some("a".into()),
            name: "alpha".into(),
            unread: unread.map(Into::into),
            report: Some(report("done: built it")),
        }
    }

    pub fn menu(subject: Subject, config: &str) -> Menu {
        let mut menu = Menu::new(subject, from_table(&config.parse().unwrap()));
        menu.now_ms = NOW_MS;
        menu
    }

    pub fn press(menu: &mut Menu, code: KeyCode) {
        menu.key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    pub fn typed(menu: &mut Menu, text: &str) {
        for c in text.chars() {
            press(menu, KeyCode::Char(c));
        }
    }

    fn draw(menu: &mut Menu, width: u16, height: u16) -> Vec<String> {
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, width, height));
        menu.draw(&mut buf);
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

    fn keys(menu: &Menu) -> String {
        menu.choices().iter().map(|(k, _, _)| *k).collect()
    }

    fn failed(stderr: &str) -> Output {
        Output {
            ok: false,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }

    const HELD_UNSEEN: &str = "loom: close: the user still has it, nothing closed (leave it to them; --force to close anyway):\n  alpha [a] unseen: its audience is the user, who has not seen its done report (loom ack once they have)\n  alpha-one [a1] unseen: its audience is the user, who has not seen its failed report (loom ack once they have)\n";
    const HELD_HANDOFF: &str = "loom: close: the user still has it, nothing closed (leave it to them; --force to close anyway):\n  alpha [a] unseen: its audience is the user, who has not seen its done report (loom ack once they have)\n  alpha-one [a1] handoff: steps are left for the user in w1:p5\n";
    const BUSY: &str = "loom: close: still working or blocked, nothing closed (--force to close anyway):\n  alpha-one [a1] running, herdr: working\n";

    #[test]
    fn entries_follow_the_row_kind() {
        assert_eq!(keys(&menu(agent(Some("done")), "")), "oartcfp");
        // nothing unread: no ack; no report: nothing to open
        assert_eq!(keys(&menu(agent(None), "")), "ortcfp");
        let bare = Subject::Agent {
            pane_id: "w1:p2".into(),
            agent_id: None,
            name: "claude".into(),
            unread: None,
            report: None,
        };
        assert_eq!(keys(&menu(bare, "")), "cp");
        let ws = Subject::Workspace {
            workspace_id: "w2".into(),
            label: "elsewhere".into(),
        };
        assert_eq!(keys(&menu(ws, "")), "rcwo");
    }

    #[test]
    fn the_header_carries_the_name_and_latest_report_line() {
        let mut m = menu(agent(Some("done")), "");
        let lines = draw(&mut m, 40, 14);
        assert_eq!(lines[0], " alpha");
        assert_eq!(lines[1], " unread · done: built it");
        assert_eq!(lines[3], "  o  open report");
        assert_eq!(lines[4], "  a  ack");
        assert_eq!(lines[13], " key or enter runs · esc closes");
    }

    #[test]
    fn keys_jk_enter_and_clicks_pick_entries_and_esc_quits() {
        let mut m = menu(agent(None), "");
        typed(&mut m, "jj");
        assert_eq!(m.selected, 2);
        press(&mut m, KeyCode::Enter);
        assert!(matches!(
            &m.screen,
            Screen::Prompt {
                kind: Prompt::Tell,
                ..
            }
        ));
        press(&mut m, KeyCode::Esc);
        assert_eq!(m.screen, Screen::Menu);
        let lines = draw(&mut m, 40, 14);
        let scope = lines
            .iter()
            .position(|l| l.contains("scope to subtree"))
            .unwrap();
        m.mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 4,
            row: scope as u16,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(m.job, Some(Job::Ctl("scope", "w1:p3".into())));
        let mut stub = Stub::default();
        m.work(&mut stub);
        assert_eq!(stub.ctls, [("scope".to_string(), "w1:p3".to_string())]);
        assert_eq!(m.exit, Some(Exit::Quit));
        let mut m = menu(agent(None), "");
        press(&mut m, KeyCode::Esc);
        assert_eq!(m.exit, Some(Exit::Quit));
    }

    #[test]
    fn closing_the_report_acks_it_and_returns_to_the_menu() {
        let mut m = menu(agent(Some("done")), LOOM);
        press(&mut m, KeyCode::Char('o'));
        let lines = draw(&mut m, 40, 12);
        assert_eq!(lines[2], " built it");
        assert_eq!(lines[4], " the long part");
        press(&mut m, KeyCode::Esc);
        assert_eq!(m.job, Some(Job::Ack { then_quit: false }));
        let mut stub = Stub::default();
        m.work(&mut stub);
        assert_eq!(stub.runs, [["loom", "ack", "a"]]);
        assert_eq!(m.screen, Screen::Menu);
        assert_eq!(m.status.as_deref(), Some("acked"));
        // read now: no ack entry, and reopening never acks again
        assert_eq!(keys(&m), "ortcfp");
        press(&mut m, KeyCode::Char('o'));
        press(&mut m, KeyCode::Esc);
        assert_eq!(m.job, None);
    }

    #[test]
    fn the_report_screen_shows_the_report_under_name_kind_and_age() {
        let message: Value =
            serde_json::from_str(include_str!("../tests/fixtures/reports/message-119.json"))
                .unwrap();
        let sent = message["sent_at_ms"].as_u64().unwrap();
        let subject = Subject::Agent {
            pane_id: "w1:p3".into(),
            agent_id: Some("004127b1".into()),
            name: "test-dirs-055".into(),
            unread: Some("done".into()),
            report: latest_report(api::messages_from_result(&json!({ "messages": [message] }))),
        };
        let mut m = menu(subject, &format!("glyphs = \"unicode\"\n{LOOM}"));
        m.now_ms = sent + 4 * 60_000 + 59_000;
        press(&mut m, KeyCode::Char('o'));
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 50, 30));
        m.draw(&mut buf);
        let line = |y: u16| {
            (0..50)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        };
        assert_eq!(line(0), " test-dirs-055 ✓ done · 4m");
        // the glyph in its kind's colour, the name bold, the body normal weight
        assert_eq!(buf[(15, 0)].fg, ratatui::style::Color::Cyan);
        assert!(buf[(1, 0)].modifier.contains(Modifier::BOLD));
        assert_eq!(line(2), " 055 test user dirs: test-only temp defaults");
        assert!(!buf[(1, 2)].modifier.contains(Modifier::BOLD));
        let lines: Vec<String> = (0..30).map(line).collect();
        assert!(lines.iter().all(|l| !l.contains('‖')), "{lines:#?}");
        let heading = lines
            .iter()
            .position(|l| l == " 055 test user dirs — implementation report")
            .unwrap();
        assert!(buf[(1, heading as u16)].modifier.contains(Modifier::BOLD));
        assert_eq!(lines[29], " j/k scroll · esc closes and acks");
        // scrolling still works and is clamped to the end
        for _ in 0..500 {
            press(&mut m, KeyCode::Char('j'));
        }
        m.draw(&mut buf);
        let Screen::Report { scroll } = m.screen else {
            panic!()
        };
        assert!(scroll > 0);
        press(&mut m, KeyCode::Char('g'));
        assert_eq!(m.screen, Screen::Report { scroll: 0 });
    }

    #[test]
    fn a_report_without_a_body_is_its_summary_without_the_parents_rows() {
        let mut subject = agent(None);
        if let Subject::Agent { report, .. } = &mut subject {
            *report = Some(Report {
                body: None,
                sent_at_ms: None,
                ..super::tests::report(
                    "failed: tests red ‖ child b [b1] running, no report yet ‖ [loom] 1 ask open",
                )
            });
        }
        let mut m = menu(subject, "glyphs = \"unicode\"");
        // the menu's line keeps the kind but drops the rows too
        assert_eq!(draw(&mut m, 60, 10)[1], " failed: tests red");
        press(&mut m, KeyCode::Char('o'));
        let lines = draw(&mut m, 60, 10);
        // no age without a send time
        assert_eq!(lines[0], " alpha ✗ failed");
        assert_eq!(lines[2], " tests red");
        assert_eq!(lines[3], "");
    }

    #[test]
    fn without_an_ack_action_ack_and_open_report_say_so() {
        let mut m = menu(agent(Some("done")), "");
        press(&mut m, KeyCode::Char('a'));
        assert_eq!(m.job, None);
        assert_eq!(m.status.as_deref(), Some(NO_ACK));
        // read whole even in a narrow popup
        let lines = draw(&mut m, 30, 14);
        assert_eq!(
            lines[12..],
            [" ack: no [sidebar.actions]", " ack configured"]
        );
        press(&mut m, KeyCode::Char('o'));
        press(&mut m, KeyCode::Esc);
        assert_eq!(m.job, None);
        assert_eq!(m.screen, Screen::Menu);
        assert_eq!(m.status.as_deref(), Some(NO_ACK));
    }

    #[test]
    fn ack_runs_the_ack_action_and_quits() {
        let mut m = menu(agent(Some("done")), LOOM);
        press(&mut m, KeyCode::Char('a'));
        let mut stub = Stub::default();
        m.work(&mut stub);
        assert_eq!(stub.runs, [["loom", "ack", "a"]]);
        assert_eq!(m.exit, Some(Exit::Quit));
        // a failing ack stays and shows why
        let mut m = menu(agent(Some("done")), LOOM);
        press(&mut m, KeyCode::Char('a'));
        let mut stub = Stub {
            outputs: vec![failed("loom: ack: no node a\n")],
            ..Default::default()
        };
        m.work(&mut stub);
        assert_eq!(m.exit, None);
        assert_eq!(m.status.as_deref(), Some("ack: loom: ack: no node a"));
    }

    #[test]
    fn close_picks_an_outcome_and_struggled_asks_why() {
        let mut m = menu(agent(None), LOOM);
        press(&mut m, KeyCode::Char('c'));
        assert_eq!(m.screen, Screen::Outcome);
        press(&mut m, KeyCode::Char('o'));
        let mut stub = Stub::default();
        m.work(&mut stub);
        assert_eq!(stub.runs, [["loom", "close", "a", "--outcome", "ok"]]);
        assert_eq!(m.exit, Some(Exit::Quit));

        let mut m = menu(agent(None), LOOM);
        typed(&mut m, "cs");
        typed(&mut m, "flaky ci");
        press(&mut m, KeyCode::Enter);
        let mut stub = Stub::default();
        m.work(&mut stub);
        assert_eq!(
            stub.runs,
            [[
                "loom",
                "close",
                "a",
                "--outcome",
                "struggled",
                "--why",
                "flaky ci"
            ]]
        );
    }

    #[test]
    fn a_close_held_only_by_unread_reports_offers_ack_and_close() {
        let mut m = menu(agent(Some("done")), LOOM);
        typed(&mut m, "co");
        let mut stub = Stub {
            outputs: vec![failed(HELD_UNSEEN)],
            ..Default::default()
        };
        m.work(&mut stub);
        let Screen::Refused { unread, .. } = &m.screen else {
            panic!("{:?}", m.screen)
        };
        assert_eq!(
            unread
                .iter()
                .map(|u| (u.agent_id.as_str(), u.pane_id.as_str(), u.line.as_str()))
                .collect::<Vec<_>>(),
            [
                ("a", "w1:p3", "done: a finished"),
                ("a1", "w1:p5", "done: a1 finished")
            ]
        );
        assert_eq!(keys(&m), "af");
        let lines = draw(&mut m, 60, 20);
        assert!(
            lines.iter().any(|l| l == "   alpha-one: done: a1 finished"),
            "{lines:#?}"
        );
        press(&mut m, KeyCode::Char('a'));
        let mut stub = Stub::default();
        m.work(&mut stub);
        assert_eq!(
            stub.runs,
            [
                vec!["loom", "ack", "a"],
                vec!["loom", "ack", "a1"],
                vec!["loom", "close", "a", "--outcome", "ok"]
            ]
        );
        assert_eq!(m.exit, Some(Exit::Quit));
    }

    #[test]
    fn any_other_refusal_offers_only_force() {
        for stderr in [HELD_HANDOFF, BUSY] {
            let mut m = menu(agent(Some("done")), LOOM);
            typed(&mut m, "co");
            let mut stub = Stub {
                outputs: vec![failed(stderr)],
                ..Default::default()
            };
            m.work(&mut stub);
            assert!(matches!(&m.screen, Screen::Refused { unread, .. } if unread.is_empty()));
            assert_eq!(keys(&m), "f");
            press(&mut m, KeyCode::Char('f'));
            let mut stub = Stub::default();
            m.work(&mut stub);
            assert_eq!(
                stub.runs,
                [["loom", "close", "a", "--outcome", "ok", "--force"]]
            );
            assert_eq!(m.exit, Some(Exit::Quit));
        }
    }

    #[test]
    fn ack_and_close_refused_again_for_being_busy_offers_force() {
        let mut m = menu(agent(Some("done")), LOOM);
        typed(&mut m, "co");
        let mut stub = Stub {
            outputs: vec![failed(HELD_UNSEEN)],
            ..Default::default()
        };
        m.work(&mut stub);
        press(&mut m, KeyCode::Char('a'));
        let mut stub = Stub {
            outputs: vec![
                Output {
                    ok: true,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                Output {
                    ok: true,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                failed(BUSY),
            ],
            ..Default::default()
        };
        m.work(&mut stub);
        assert_eq!(keys(&m), "f");
        // the header no longer calls the acked report unread
        assert_eq!(draw(&mut m, 40, 12)[1], " done: built it");
    }

    #[test]
    fn a_loom_error_is_not_a_refusal() {
        let mut m = menu(agent(None), LOOM);
        typed(&mut m, "co");
        let mut stub = Stub {
            outputs: vec![failed("loom: close: no node a\n")],
            ..Default::default()
        };
        m.work(&mut stub);
        assert_eq!(m.screen, Screen::Menu);
        assert_eq!(m.status.as_deref(), Some("close: loom: close: no node a"));
    }

    #[test]
    fn refusals_read_looms_held_lines() {
        assert!(refusal(HELD_UNSEEN).unread_only());
        assert!(!refusal(HELD_HANDOFF).unread_only());
        assert_eq!(refusal(BUSY), Refusal::Busy);
        assert_eq!(refusal("loom: close: no node x"), Refusal::Error);
        // a held line loom words differently counts as not unread: force only
        let odd = "loom: close: the user still has it, nothing closed:\n  alpha [a] has a thing\n";
        assert!(!refusal(odd).unread_only());
    }

    #[test]
    fn without_a_close_action_close_is_herdrs_with_preview_and_force() {
        let mut m = menu(agent(None), "");
        press(&mut m, KeyCode::Char('c'));
        assert_eq!(m.job, Some(Job::ClosePreview));
        let mut stub = Stub {
            close_error: Some(ApiError {
                code: "agent_subtree_busy".into(),
                message: "alpha-one working".into(),
            }),
            ..Default::default()
        };
        m.work(&mut stub);
        let lines = draw(&mut m, 40, 14);
        assert_eq!(lines[3], " close alpha: 2 pane(s)");
        press(&mut m, KeyCode::Char('y'));
        m.work(&mut stub);
        assert!(matches!(
            &m.screen,
            Screen::Confirm {
                yes: Job::Close { force: true },
                ..
            }
        ));
        press(&mut m, KeyCode::Enter);
        m.work(&mut stub);
        assert_eq!(m.exit, Some(Exit::Quit));
        let closes: Vec<&Value> = stub
            .calls
            .iter()
            .filter(|(m, _)| m == "agent.close")
            .map(|(_, p)| p)
            .collect();
        assert_eq!(closes[2]["force"], json!(true));
    }

    #[test]
    fn reply_answers_the_latest_report_and_tell_sends_a_tell() {
        let mut m = menu(agent(None), "");
        press(&mut m, KeyCode::Char('r'));
        typed(&mut m, "go on");
        let lines = draw(&mut m, 40, 10);
        assert_eq!(lines[3], " reply alpha> go on▏");
        press(&mut m, KeyCode::Enter);
        let mut stub = Stub::default();
        m.work(&mut stub);
        assert_eq!(
            stub.calls.last().unwrap(),
            &(
                "message.send".to_string(),
                json!({ "to": "a", "summary": "go on", "kind": "reply", "reply_to": 7 })
            )
        );
        let mut m = menu(agent(None), "");
        typed(&mut m, "thi");
        press(&mut m, KeyCode::Enter);
        assert_eq!(
            m.job,
            Some(Job::Call(
                "message.send",
                json!({ "to": "a", "summary": "hi", "kind": "tell" })
            ))
        );
    }

    #[test]
    fn overrides_exec_in_the_popup_or_run_detached() {
        let mut m = menu(
            agent(None),
            r#"actions = { reply = { command = ["loom", "reply", "{agent_id}"], popup = true }, tell = { command = ["t", "{name}"] } }"#,
        );
        press(&mut m, KeyCode::Char('r'));
        assert_eq!(
            m.exit,
            Some(Exit::Exec(vec!["loom".into(), "reply".into(), "a".into()]))
        );
        let mut m = menu(
            agent(None),
            r#"actions = { tell = { command = ["t", "{name}"] } }"#,
        );
        press(&mut m, KeyCode::Char('t'));
        let mut stub = Stub::default();
        m.work(&mut stub);
        assert_eq!(stub.spawned, [["t", "alpha"]]);
        assert_eq!(m.exit, Some(Exit::Quit));
    }

    #[test]
    fn loom_ui_opens_after_the_popup_closes() {
        let mut m = menu(agent(None), "");
        press(&mut m, KeyCode::Char('p'));
        assert!(
            matches!(&m.exit, Some(Exit::After("plugin.pane.open", p)) if p["plugin_id"] == "loom")
        );
    }

    #[test]
    fn workspace_entries_rename_close_branch_and_collapse() {
        let ws = || Subject::Workspace {
            workspace_id: "w2".into(),
            label: "elsewhere".into(),
        };
        let mut m = menu(ws(), "");
        press(&mut m, KeyCode::Char('r'));
        assert!(matches!(&m.screen, Screen::Prompt { input, .. } if input == "elsewhere"));
        press(&mut m, KeyCode::Backspace);
        press(&mut m, KeyCode::Enter);
        assert_eq!(
            m.job,
            Some(Job::Call(
                "workspace.rename",
                json!({ "workspace_id": "w2", "label": "elsewher" })
            ))
        );
        let mut m = menu(ws(), "");
        typed(&mut m, "cy");
        assert_eq!(
            m.job,
            Some(Job::Call(
                "workspace.close",
                json!({ "workspace_id": "w2" })
            ))
        );
        let mut m = menu(ws(), "");
        typed(&mut m, "cn");
        assert_eq!((m.job.clone(), m.screen.clone()), (None, Screen::Menu));
        let mut m = menu(ws(), "");
        typed(&mut m, "wfeat");
        press(&mut m, KeyCode::Enter);
        assert_eq!(
            m.job,
            Some(Job::Call(
                "worktree.create",
                json!({ "workspace_id": "w2", "branch": "feat", "focus": true })
            ))
        );
        let mut m = menu(ws(), "");
        press(&mut m, KeyCode::Char('o'));
        assert_eq!(m.job, Some(Job::Ctl("collapse-others", "w2".into())));
    }

    #[test]
    fn the_subject_is_read_from_the_snapshot() {
        let snap = attention_snapshot();
        let s = subject(&snap, &Target::Pane("w1:p5".into()), |id| {
            Some(report(&format!("done: {id}")))
        })
        .unwrap();
        assert!(
            matches!(&s, Subject::Agent { agent_id: Some(id), unread: Some(kind), report: Some(r), .. }
            if id == "a1" && kind == "done" && r.summary == "done: a1")
        );
        let s = subject(&snap, &Target::Workspace("w2".into()), |_| None).unwrap();
        assert_eq!(
            s,
            Subject::Workspace {
                workspace_id: "w2".into(),
                label: "elsewhere".into()
            }
        );
        assert!(subject(&snap, &Target::Pane("w9:p1".into()), |_| None).is_err());
    }

    #[test]
    fn wrap_breaks_on_words_and_cuts_long_ones() {
        assert_eq!(wrap("one two three", 7), ["one two", "three"]);
        assert_eq!(wrap("a\n\nb", 5), ["a", "", "b"]);
        assert_eq!(wrap("abcdefgh", 3), ["abc", "def", "gh"]);
        assert_eq!(wrap("  ab cd ef", 8), ["  ab cd", "ef"]);
    }
}
