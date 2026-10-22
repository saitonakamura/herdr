//! The row model: a snapshot and the sidebar's own state in, the list of items
//! to draw out. Pure, so it is tested against synthetic snapshots.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::api::{AgentStatus, PaneInfo, SessionSnapshot, TaskStatus};
use crate::glyphs::Meaning;

/// The one effort or subtree the sidebar is narrowed to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum Scope {
    Workspace(String),
    /// An agent id.
    Subtree(String),
}

/// What the sidebar remembers per session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiState {
    /// Workspace ids whose section is collapsed.
    pub collapsed: BTreeSet<String>,
    /// Node keys (agent ids) whose children are hidden.
    pub folded: BTreeSet<String>,
    pub scope: Option<Scope>,
}

/// Facts the sidebar gathers beside the snapshot.
#[derive(Debug, Clone, Default)]
pub struct Extras {
    /// Pending messages per recipient agent id.
    pub pending: HashMap<String, usize>,
    /// Panes whose process exited.
    pub exited: HashSet<String>,
    /// Seconds each pane has been in its current state.
    pub ages: HashMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoLine {
    pub branch: Option<String>,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    pub dirty: Option<usize>,
    /// The checkout's directory name, for a linked worktree.
    pub worktree: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderItem {
    pub workspace_id: String,
    pub label: String,
    pub effort: Option<String>,
    pub collapsed: bool,
    pub focused: bool,
    /// Agents per state, in glyph-table order, zeros left out.
    pub counts: Vec<(Meaning, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentItem {
    pub pane_id: String,
    pub workspace_id: String,
    pub agent_id: Option<String>,
    /// For each ancestor below the root: whether its column still has siblings to come.
    pub guides: Vec<bool>,
    pub depth: usize,
    pub last: bool,
    pub meaning: Meaning,
    pub harness: Option<String>,
    pub name: String,
    /// The effort id, on root rows only.
    pub effort: Option<String>,
    pub ticket: Option<String>,
    pub task: Option<&'static str>,
    pub failed: bool,
    pub pending: usize,
    /// The live Source: its agent id and name.
    pub source: Option<(String, String)>,
    pub age: Option<u64>,
    pub repo: Option<RepoLine>,
    pub folded: bool,
    pub hidden_below: usize,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneItem {
    pub pane_id: String,
    pub workspace_id: String,
    pub name: String,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Scope(String),
    Search(String),
    Header(HeaderItem),
    Agent(Box<AgentItem>),
    Pane(PaneItem),
    New,
}

impl Item {
    /// A key that survives redraws, for the selection.
    pub fn key(&self) -> String {
        match self {
            Item::Scope(_) => "scope".into(),
            Item::Search(_) => "search".into(),
            Item::Header(h) => format!("ws:{}", h.workspace_id),
            Item::Agent(a) => format!("pane:{}", a.pane_id),
            Item::Pane(p) => format!("pane:{}", p.pane_id),
            Item::New => "new".into(),
        }
    }

    pub fn selectable(&self) -> bool {
        !matches!(self, Item::Search(_))
    }
}

/// Creation order within a workspace: herdr numbers panes monotonically per
/// workspace (`w1:p3` is the third), so the id's suffix orders them.
pub fn creation_number(pane_id: &str) -> usize {
    const ALPHABET: &[u8; 32] = b"123456789ABCDEFGHJKMNPQRSTVWXYZ0";
    let Some((_, suffix)) = pane_id.split_once(":p") else {
        return usize::MAX;
    };
    let mut value = 0usize;
    for ch in suffix.bytes() {
        let Some(digit) = ALPHABET.iter().position(|c| *c == ch) else {
            return usize::MAX;
        };
        value = value.saturating_mul(32).saturating_add(digit + 1);
    }
    value
}

pub fn is_agent(pane: &PaneInfo) -> bool {
    pane.agent_id.is_some() || pane.agent.is_some()
}

fn agent_key(pane: &PaneInfo) -> String {
    pane.agent_id
        .clone()
        .unwrap_or_else(|| pane.pane_id.clone())
}

/// What a row calls the agent: its tab's name, else the pane's label or the harness.
pub fn agent_name(pane: &PaneInfo, tab_labels: &HashMap<&str, &str>) -> String {
    tab_labels
        .get(pane.tab_id.as_str())
        .map(|label| label.to_string())
        .filter(|label| !label.is_empty())
        .or_else(|| pane.label.clone())
        .or_else(|| pane.agent.clone())
        .unwrap_or_else(|| pane.pane_id.clone())
}

fn pane_name(pane: &PaneInfo, exited: bool) -> String {
    if exited {
        return "exited".into();
    }
    let title = pane
        .label
        .clone()
        .or_else(|| pane.terminal_title_stripped.clone())
        .or_else(|| pane.title.clone())
        .filter(|title| !title.trim().is_empty());
    let cwd = pane
        .foreground_cwd
        .as_deref()
        .or(pane.cwd.as_deref())
        .and_then(|cwd| std::path::Path::new(cwd).file_name())
        .map(|name| name.to_string_lossy().into_owned());
    match (title, cwd) {
        (Some(title), Some(cwd)) if title != cwd => format!("{title} · {cwd}"),
        (Some(title), _) => title,
        (None, Some(cwd)) => cwd,
        (None, None) => pane.pane_id.clone(),
    }
}

fn task_word(task: Option<TaskStatus>) -> Option<&'static str> {
    match task? {
        TaskStatus::NeedsParent => Some("needs_parent"),
        TaskStatus::NeedsUser => Some("needs_user"),
        TaskStatus::Failed => Some("failed"),
        TaskStatus::Complete => Some("complete"),
        _ => None,
    }
}

fn repo_line(pane: &PaneInfo) -> Option<RepoLine> {
    let repo = pane.repo.as_ref()?;
    Some(RepoLine {
        branch: repo.branch.clone(),
        ahead: repo.ahead,
        behind: repo.behind,
        dirty: repo.dirty,
        worktree: repo.is_linked_worktree.then(|| {
            std::path::Path::new(&repo.root)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| repo.root.clone())
        }),
    })
}

fn matches_search(pane: &PaneInfo, name: &str, query: &str) -> bool {
    let query = query.to_lowercase();
    [
        Some(name),
        pane.annotations.get("ticket").map(String::as_str),
        pane.annotations.get("effort").map(String::as_str),
        pane.agent_id.as_deref(),
        Some(pane.pane_id.as_str()),
    ]
    .into_iter()
    .flatten()
    .any(|field| field.to_lowercase().contains(&query))
}

struct Section<'a> {
    agents: Vec<&'a PaneInfo>,
    others: Vec<&'a PaneInfo>,
    /// Agent key -> children, in creation order.
    children: BTreeMap<String, Vec<&'a PaneInfo>>,
    roots: Vec<&'a PaneInfo>,
}

fn section<'a>(panes: &[&'a PaneInfo]) -> Section<'a> {
    let mut agents: Vec<&PaneInfo> = panes.iter().copied().filter(|p| is_agent(p)).collect();
    let mut others: Vec<&PaneInfo> = panes.iter().copied().filter(|p| !is_agent(p)).collect();
    agents.sort_by_key(|p| creation_number(&p.pane_id));
    others.sort_by_key(|p| creation_number(&p.pane_id));
    let ids: HashSet<&str> = agents
        .iter()
        .filter_map(|p| p.agent_id.as_deref())
        .collect();
    let mut children: BTreeMap<String, Vec<&PaneInfo>> = BTreeMap::new();
    let mut roots = Vec::new();
    for pane in &agents {
        match pane.parent_agent_id.as_deref() {
            Some(parent) if ids.contains(parent) && Some(parent) != pane.agent_id.as_deref() => {
                children.entry(parent.to_string()).or_default().push(pane)
            }
            _ => roots.push(*pane),
        }
    }
    // A Parent cycle has no root; its members are drawn as roots rather than lost.
    let reachable = reachable_keys(&roots, &children);
    for pane in &agents {
        if !reachable.contains(&agent_key(pane)) {
            roots.push(pane);
        }
    }
    roots.sort_by_key(|p| creation_number(&p.pane_id));
    Section {
        agents,
        others,
        children,
        roots,
    }
}

fn reachable_keys(
    roots: &[&PaneInfo],
    children: &BTreeMap<String, Vec<&PaneInfo>>,
) -> HashSet<String> {
    let mut seen = HashSet::new();
    let mut stack: Vec<&PaneInfo> = roots.to_vec();
    while let Some(pane) = stack.pop() {
        if seen.insert(agent_key(pane)) {
            if let Some(kids) = children.get(&agent_key(pane)) {
                stack.extend(kids.iter().copied());
            }
        }
    }
    seen
}

fn count_below(
    key: &str,
    children: &BTreeMap<String, Vec<&PaneInfo>>,
    seen: &mut HashSet<String>,
) -> usize {
    if !seen.insert(key.to_string()) {
        return 0;
    }
    children
        .get(key)
        .map(|kids| {
            kids.iter()
                .map(|kid| 1 + count_below(&agent_key(kid), children, seen))
                .sum()
        })
        .unwrap_or(0)
}

pub struct Inputs<'a> {
    pub snapshot: &'a SessionSnapshot,
    pub extras: &'a Extras,
    pub ui: &'a UiState,
    pub search: Option<&'a str>,
}

/// Whether the scope still names something live.
pub fn scope_is_live(snapshot: &SessionSnapshot, scope: &Scope) -> bool {
    match scope {
        Scope::Workspace(id) => snapshot.workspaces.iter().any(|w| &w.workspace_id == id),
        Scope::Subtree(id) => snapshot
            .panes
            .iter()
            .any(|p| p.agent_id.as_deref() == Some(id.as_str())),
    }
}

pub fn build(inputs: &Inputs) -> Vec<Item> {
    let snapshot = inputs.snapshot;
    let tab_labels: HashMap<&str, &str> = snapshot
        .tabs
        .iter()
        .map(|tab| (tab.tab_id.as_str(), tab.label.as_str()))
        .collect();
    let live_agents: HashMap<&str, &PaneInfo> = snapshot
        .panes
        .iter()
        .filter_map(|p| Some((p.agent_id.as_deref()?, p)))
        .collect();
    let search = inputs.search.filter(|query| !query.is_empty());
    let scope = inputs
        .ui
        .scope
        .as_ref()
        .filter(|scope| scope_is_live(snapshot, scope));

    let mut items = Vec::new();
    if let Some(scope) = scope {
        let label = match scope {
            Scope::Workspace(id) => snapshot
                .workspaces
                .iter()
                .find(|w| &w.workspace_id == id)
                .map(|w| w.label.clone())
                .unwrap_or_else(|| id.clone()),
            Scope::Subtree(id) => live_agents
                .get(id.as_str())
                .map(|p| agent_name(p, &tab_labels))
                .unwrap_or_else(|| id.clone()),
        };
        items.push(Item::Scope(label));
    }
    if let Some(query) = inputs.search {
        items.push(Item::Search(query.to_string()));
    }

    for workspace in &snapshot.workspaces {
        let panes: Vec<&PaneInfo> = snapshot
            .panes
            .iter()
            .filter(|p| p.workspace_id == workspace.workspace_id)
            .collect();
        let subtree_root = match scope {
            Some(Scope::Workspace(id)) if id != &workspace.workspace_id => continue,
            Some(Scope::Subtree(agent)) => {
                match panes
                    .iter()
                    .find(|p| p.agent_id.as_deref() == Some(agent.as_str()))
                {
                    Some(root) => Some(*root),
                    None => continue,
                }
            }
            _ => None,
        };
        let section = section(&panes);
        let name_of = |pane: &PaneInfo| agent_name(pane, &tab_labels);

        // Under a search, a node stays while it or something below it matches.
        let mut keep: Option<HashSet<String>> = None;
        if let Some(query) = search {
            let mut kept = HashSet::new();
            fn mark<'a>(
                pane: &'a PaneInfo,
                section: &Section<'a>,
                query: &str,
                name_of: &dyn Fn(&PaneInfo) -> String,
                kept: &mut HashSet<String>,
                seen: &mut HashSet<String>,
            ) -> bool {
                let key = agent_key(pane);
                if !seen.insert(key.clone()) {
                    return false;
                }
                let mut any = matches_search(pane, &name_of(pane), query);
                for kid in section.children.get(&key).into_iter().flatten() {
                    any |= mark(kid, section, query, name_of, kept, seen);
                }
                if any {
                    kept.insert(key);
                }
                any
            }
            let mut seen = HashSet::new();
            for root in &section.roots {
                mark(root, &section, query, &name_of, &mut kept, &mut seen);
            }
            for pane in &section.others {
                if matches_search(pane, &pane_name(pane, false), query) {
                    kept.insert(pane.pane_id.clone());
                }
            }
            if kept.is_empty() {
                continue;
            }
            keep = Some(kept);
        }

        let effort = workspace.tokens.get("effort").cloned().or_else(|| {
            section
                .roots
                .iter()
                .find_map(|root| root.annotations.get("effort").cloned())
        });
        let mut counts: BTreeMap<Meaning, usize> = BTreeMap::new();
        for agent in &section.agents {
            *counts
                .entry(Meaning::for_state(agent.agent_status, agent.blocked_reason))
                .or_default() += 1;
        }
        let collapsed = search.is_none() && inputs.ui.collapsed.contains(&workspace.workspace_id);
        items.push(Item::Header(HeaderItem {
            workspace_id: workspace.workspace_id.clone(),
            label: workspace.label.clone(),
            effort,
            collapsed,
            focused: workspace.focused,
            counts: counts.into_iter().filter(|(_, n)| *n > 0).collect(),
        }));
        if collapsed {
            continue;
        }

        let roots: Vec<&PaneInfo> = match subtree_root {
            Some(root) => vec![root],
            None => section.roots.clone(),
        };
        let roots: Vec<&PaneInfo> = roots
            .into_iter()
            .filter(|p| keep.as_ref().is_none_or(|k| k.contains(&agent_key(p))))
            .collect();
        let mut seen = HashSet::new();
        let count = roots.len();
        for (index, root) in roots.into_iter().enumerate() {
            push_agent(
                &mut items,
                Node {
                    pane: root,
                    depth: 0,
                    guides: Vec::new(),
                    last: index + 1 == count,
                },
                &Ctx {
                    section: &section,
                    inputs,
                    tab_labels: &tab_labels,
                    live_agents: &live_agents,
                    keep: keep.as_ref(),
                    folds_apply: search.is_none(),
                },
                &mut seen,
            );
        }
        if subtree_root.is_none() {
            for pane in &section.others {
                if keep.as_ref().is_some_and(|k| !k.contains(&pane.pane_id)) {
                    continue;
                }
                items.push(Item::Pane(PaneItem {
                    pane_id: pane.pane_id.clone(),
                    workspace_id: pane.workspace_id.clone(),
                    name: pane_name(pane, inputs.extras.exited.contains(&pane.pane_id)),
                    focused: pane.focused,
                }));
            }
        }
    }
    if scope.is_none() && search.is_none() {
        items.push(Item::New);
    }
    items
}

struct Node<'a> {
    pane: &'a PaneInfo,
    depth: usize,
    guides: Vec<bool>,
    last: bool,
}

struct Ctx<'a, 'b> {
    section: &'b Section<'a>,
    inputs: &'b Inputs<'b>,
    tab_labels: &'b HashMap<&'b str, &'b str>,
    live_agents: &'b HashMap<&'b str, &'b PaneInfo>,
    keep: Option<&'b HashSet<String>>,
    folds_apply: bool,
}

fn push_agent(items: &mut Vec<Item>, node: Node, ctx: &Ctx, seen: &mut HashSet<String>) {
    let pane = node.pane;
    let key = agent_key(pane);
    if !seen.insert(key.clone()) {
        return;
    }
    let kids: Vec<&PaneInfo> = ctx
        .section
        .children
        .get(&key)
        .map(|kids| {
            kids.iter()
                .copied()
                .filter(|kid| ctx.keep.is_none_or(|k| k.contains(&agent_key(kid))))
                .collect()
        })
        .unwrap_or_default();
    let folded = ctx.folds_apply && !kids.is_empty() && ctx.inputs.ui.folded.contains(&key);
    let hidden_below = if folded {
        count_below(&key, &ctx.section.children, &mut HashSet::new())
    } else {
        0
    };
    let source = pane
        .annotations
        .get("source")
        .filter(|id| pane.agent_id.as_ref() != Some(*id))
        .and_then(|id| {
            let source = ctx.live_agents.get(id.as_str())?;
            Some((id.clone(), agent_name(source, ctx.tab_labels)))
        });
    let pending = pane
        .agent_id
        .as_ref()
        .and_then(|id| ctx.inputs.extras.pending.get(id))
        .copied()
        .unwrap_or(0);
    items.push(Item::Agent(Box::new(AgentItem {
        pane_id: pane.pane_id.clone(),
        workspace_id: pane.workspace_id.clone(),
        agent_id: pane.agent_id.clone(),
        guides: node.guides.clone(),
        depth: node.depth,
        last: node.last,
        meaning: Meaning::for_state(pane.agent_status, pane.blocked_reason),
        harness: pane.agent.clone(),
        name: agent_name(pane, ctx.tab_labels),
        effort: (node.depth == 0)
            .then(|| pane.annotations.get("effort").cloned())
            .flatten(),
        ticket: pane
            .annotations
            .get("ticket")
            .or_else(|| pane.tokens.get("ticket"))
            .cloned(),
        task: task_word(pane.task_status),
        failed: pane.task_status == Some(TaskStatus::Failed),
        pending,
        source,
        age: ctx.inputs.extras.ages.get(&pane.pane_id).copied(),
        repo: repo_line(pane),
        folded,
        hidden_below,
        focused: pane.focused,
    })));
    if folded {
        return;
    }
    let count = kids.len();
    for (index, kid) in kids.into_iter().enumerate() {
        let mut guides = node.guides.clone();
        if node.depth > 0 {
            guides.push(!node.last);
        }
        push_agent(
            items,
            Node {
                pane: kid,
                depth: node.depth + 1,
                guides,
                last: index + 1 == count,
            },
            ctx,
            seen,
        );
    }
}

/// The agent ids in `agent`'s subtree, itself included, within one workspace.
pub fn subtree_ids(snapshot: &SessionSnapshot, agent: &str) -> Vec<String> {
    let Some(root) = snapshot
        .panes
        .iter()
        .find(|p| p.agent_id.as_deref() == Some(agent))
    else {
        return Vec::new();
    };
    let panes: Vec<&PaneInfo> = snapshot
        .panes
        .iter()
        .filter(|p| p.workspace_id == root.workspace_id)
        .collect();
    let section = section(&panes);
    let mut out = Vec::new();
    let mut stack = vec![root];
    let mut seen = HashSet::new();
    while let Some(pane) = stack.pop() {
        let key = agent_key(pane);
        if !seen.insert(key.clone()) {
            continue;
        }
        out.push(key.clone());
        stack.extend(section.children.get(&key).into_iter().flatten().copied());
    }
    out
}

/// Tracks how long each pane has been in its current state, as the sidebar saw it.
#[derive(Debug, Default)]
pub struct AgeClock {
    since: HashMap<String, (AgentStatus, u64)>,
}

impl AgeClock {
    /// Records `now` (seconds) as the start of any state the clock hasn't seen yet.
    pub fn observe(&mut self, snapshot: &SessionSnapshot, now: u64) {
        let live: HashSet<&str> = snapshot.panes.iter().map(|p| p.pane_id.as_str()).collect();
        self.since.retain(|pane, _| live.contains(pane.as_str()));
        for pane in &snapshot.panes {
            match self.since.get(&pane.pane_id) {
                Some((status, _)) if *status == pane.agent_status => {}
                _ => {
                    self.since
                        .insert(pane.pane_id.clone(), (pane.agent_status, now));
                }
            }
        }
    }

    pub fn ages(&self, now: u64) -> HashMap<String, u64> {
        self.since
            .iter()
            .map(|(pane, (_, at))| (pane.clone(), now.saturating_sub(*at)))
            .collect()
    }
}

#[cfg(test)]
pub mod fixtures {
    use super::*;
    use crate::api::{PaneRepoInfo, TabInfo, WorkspaceInfo};

    pub fn workspace(id: &str, label: &str) -> WorkspaceInfo {
        WorkspaceInfo {
            workspace_id: id.into(),
            label: label.into(),
            ..Default::default()
        }
    }

    pub fn tab(id: &str, label: &str) -> TabInfo {
        TabInfo {
            tab_id: id.into(),
            workspace_id: id.split(':').next().unwrap().into(),
            label: label.into(),
            focused: false,
        }
    }

    /// An agent pane `w:pN`, in its own tab `w:tN` named `name`.
    pub fn agent(pane: &str, id: &str, parent: Option<&str>, status: AgentStatus) -> PaneInfo {
        let (ws, n) = pane.split_once(":p").unwrap();
        PaneInfo {
            pane_id: pane.into(),
            workspace_id: ws.into(),
            tab_id: format!("{ws}:t{n}"),
            agent: Some("claude".into()),
            agent_id: Some(id.into()),
            parent_agent_id: parent.map(Into::into),
            agent_status: status,
            ..Default::default()
        }
    }

    pub fn shell(pane: &str, cwd: &str) -> PaneInfo {
        let (ws, n) = pane.split_once(":p").unwrap();
        PaneInfo {
            pane_id: pane.into(),
            workspace_id: ws.into(),
            tab_id: format!("{ws}:t{n}"),
            cwd: Some(cwd.into()),
            ..Default::default()
        }
    }

    pub fn repo(branch: &str) -> PaneRepoInfo {
        PaneRepoInfo {
            root: "/code/herdr".into(),
            repo_name: "herdr".into(),
            branch: Some(branch.into()),
            ahead: Some(1),
            behind: Some(0),
            dirty: Some(2),
            is_linked_worktree: false,
        }
    }

    /// Two workspaces: `w1` holds root -> (a -> a1, b) and a shell, `w2` one agent.
    pub fn snapshot() -> SessionSnapshot {
        let mut root = agent("w1:p1", "root", None, AgentStatus::Done);
        root.annotations
            .insert("effort".into(), "pt:herdr-fork".into());
        root.repo = Some(repo("master"));
        let mut a = agent("w1:p3", "a", Some("root"), AgentStatus::Working);
        a.annotations.insert("ticket".into(), "fork-030".into());
        let b = agent("w1:p4", "b", Some("root"), AgentStatus::Blocked);
        let a1 = agent("w1:p5", "a1", Some("a"), AgentStatus::Idle);
        let sh = shell("w1:p2", "/code/herdr");
        let mut other = agent("w2:p1", "other", None, AgentStatus::Idle);
        other.annotations.insert("source".into(), "a".into());
        SessionSnapshot {
            focused_workspace_id: Some("w1".into()),
            focused_tab_id: None,
            focused_pane_id: Some("w1:p1".into()),
            workspaces: vec![workspace("w1", "dogfood"), workspace("w2", "elsewhere")],
            tabs: vec![
                tab("w1:t1", "cockpit"),
                tab("w1:t2", "shell"),
                tab("w1:t3", "alpha"),
                tab("w1:t4", "beta"),
                tab("w1:t5", "alpha-one"),
                tab("w2:t1", "remote"),
            ],
            // Deliberately out of creation order: the model sorts by pane number.
            panes: vec![b, sh, a1, root, a, other],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn names(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                Item::Scope(label) => format!("scope {label}"),
                Item::Search(query) => format!("/{query}"),
                Item::Header(h) => format!("# {}", h.label),
                Item::Agent(a) => format!("{}{}", "  ".repeat(a.depth), a.name),
                Item::Pane(p) => format!("- {}", p.name),
                Item::New => "+".into(),
            })
            .collect()
    }

    fn build_with(ui: &UiState, search: Option<&str>) -> Vec<Item> {
        let snapshot = snapshot();
        build(&Inputs {
            snapshot: &snapshot,
            extras: &Extras::default(),
            ui,
            search,
        })
    }

    #[test]
    fn sections_hold_the_parent_tree_in_creation_order_then_other_panes() {
        let items = build_with(&UiState::default(), None);
        assert_eq!(
            names(&items),
            [
                "# dogfood",
                "cockpit",
                "  alpha",
                "    alpha-one",
                "  beta",
                "- herdr",
                "# elsewhere",
                "remote",
                "+"
            ]
        );
    }

    #[test]
    fn creation_numbers_follow_herdrs_public_id_alphabet() {
        assert_eq!(creation_number("w1:p1"), 1);
        assert_eq!(creation_number("w1:p9"), 9);
        assert_eq!(creation_number("w1:pA"), 10);
        assert!(creation_number("w1:pX") > creation_number("w1:pQ"));
        assert!(creation_number("w1:p11") > creation_number("w1:p0"));
    }

    #[test]
    fn rows_carry_root_effort_ticket_repo_and_live_source() {
        let items = build_with(&UiState::default(), None);
        let agent = |name: &str| {
            items
                .iter()
                .find_map(|item| match item {
                    Item::Agent(a) if a.name == name => Some(a.clone()),
                    _ => None,
                })
                .unwrap()
        };
        let root = agent("cockpit");
        assert_eq!(root.effort.as_deref(), Some("pt:herdr-fork"));
        assert_eq!(
            root.repo.as_ref().unwrap().branch.as_deref(),
            Some("master")
        );
        assert_eq!(agent("alpha").ticket.as_deref(), Some("fork-030"));
        assert_eq!(agent("alpha").effort, None);
        assert_eq!(
            agent("remote").source,
            Some(("a".to_string(), "alpha".to_string()))
        );
        // nothing on the source's own row
        assert_eq!(agent("alpha").source, None);
        let header = items
            .iter()
            .find_map(|item| match item {
                Item::Header(h) if h.workspace_id == "w1" => Some(h.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(header.effort.as_deref(), Some("pt:herdr-fork"));
        assert_eq!(header.counts.iter().map(|(_, n)| n).sum::<usize>(), 4);
    }

    #[test]
    fn a_source_that_is_gone_draws_nothing() {
        let mut snapshot = snapshot();
        snapshot
            .panes
            .retain(|p| p.agent_id.as_deref() != Some("a"));
        let items = build(&Inputs {
            snapshot: &snapshot,
            extras: &Extras::default(),
            ui: &UiState::default(),
            search: None,
        });
        assert!(items
            .iter()
            .all(|item| !matches!(item, Item::Agent(a) if a.source.is_some())));
    }

    #[test]
    fn collapse_folds_and_scope_narrow_the_list() {
        let mut ui = UiState::default();
        ui.collapsed.insert("w2".into());
        ui.folded.insert("a".into());
        let items = build_with(&ui, None);
        assert_eq!(
            names(&items),
            [
                "# dogfood",
                "cockpit",
                "  alpha",
                "  beta",
                "- herdr",
                "# elsewhere",
                "+"
            ]
        );
        let folded = items
            .iter()
            .find_map(|item| match item {
                Item::Agent(a) if a.name == "alpha" => Some(a.clone()),
                _ => None,
            })
            .unwrap();
        assert!(folded.folded);
        assert_eq!(folded.hidden_below, 1);

        let ui = UiState {
            scope: Some(Scope::Subtree("a".into())),
            ..Default::default()
        };
        assert_eq!(
            names(&build_with(&ui, None)),
            ["scope alpha", "# dogfood", "alpha", "  alpha-one"]
        );

        let ui = UiState {
            scope: Some(Scope::Workspace("w2".into())),
            ..Default::default()
        };
        assert_eq!(
            names(&build_with(&ui, None)),
            ["scope elsewhere", "# elsewhere", "remote"]
        );
    }

    #[test]
    fn a_scope_on_something_gone_is_ignored() {
        let ui = UiState {
            scope: Some(Scope::Subtree("gone".into())),
            ..Default::default()
        };
        assert_eq!(
            build_with(&ui, None).len(),
            build_with(&UiState::default(), None).len()
        );
    }

    #[test]
    fn search_keeps_matches_and_the_way_down_to_them() {
        let mut ui = UiState::default();
        ui.collapsed.insert("w1".into());
        assert_eq!(
            names(&build_with(&ui, Some("one"))),
            ["/one", "# dogfood", "cockpit", "  alpha", "    alpha-one"]
        );
        assert_eq!(
            names(&build_with(&ui, Some("030"))),
            ["/030", "# dogfood", "cockpit", "  alpha"]
        );
    }

    #[test]
    fn parent_cycles_and_foreign_parents_still_draw_every_agent() {
        let mut snapshot = snapshot();
        for pane in &mut snapshot.panes {
            match pane.agent_id.as_deref() {
                Some("root") => pane.parent_agent_id = Some("b".into()),
                Some("other") => pane.parent_agent_id = Some("root".into()),
                _ => {}
            }
        }
        let items = build(&Inputs {
            snapshot: &snapshot,
            extras: &Extras::default(),
            ui: &UiState::default(),
            search: None,
        });
        let agents = items.iter().filter(|i| matches!(i, Item::Agent(_))).count();
        assert_eq!(agents, 5);
    }

    #[test]
    fn pending_counts_task_words_and_exited_panes() {
        let mut snapshot = snapshot();
        for pane in &mut snapshot.panes {
            if pane.agent_id.as_deref() == Some("b") {
                pane.task_status = Some(TaskStatus::Failed);
            }
            if pane.agent_id.as_deref() == Some("a") {
                pane.task_status = Some(TaskStatus::Running);
            }
        }
        let mut extras = Extras::default();
        extras.pending.insert("b".into(), 2);
        extras.exited.insert("w1:p2".into());
        let items = build(&Inputs {
            snapshot: &snapshot,
            extras: &extras,
            ui: &UiState::default(),
            search: None,
        });
        let find = |name: &str| {
            items
                .iter()
                .find_map(|item| match item {
                    Item::Agent(a) if a.name == name => Some(a.clone()),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(find("beta").pending, 2);
        assert_eq!(find("beta").task, Some("failed"));
        assert!(find("beta").failed);
        assert_eq!(find("alpha").task, None);
        assert!(names(&items).contains(&"- exited".to_string()));
    }

    #[test]
    fn subtree_ids_follow_parent_within_the_workspace() {
        let mut ids = subtree_ids(&snapshot(), "a");
        ids.sort();
        assert_eq!(ids, ["a", "a1"]);
        assert_eq!(subtree_ids(&snapshot(), "root").len(), 4);
    }

    #[test]
    fn the_age_clock_restarts_on_a_state_change() {
        let mut snapshot = snapshot();
        let mut clock = AgeClock::default();
        clock.observe(&snapshot, 100);
        clock.observe(&snapshot, 150);
        assert_eq!(clock.ages(160)["w1:p1"], 60);
        snapshot.panes[3].agent_status = AgentStatus::Working;
        clock.observe(&snapshot, 170);
        assert_eq!(clock.ages(175)[&snapshot.panes[3].pane_id], 5);
    }
}
