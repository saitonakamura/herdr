//! Moving a workspace in herdr's order (062), which herdr keeps and every client
//! shows. A drag drops a header above another header or after the last tree; the
//! workspace menu steps one up or down. A repo's worktree workspaces move as one
//! block by upstream's rules (`8843bbb0`): a linked worktree never moves alone,
//! and nothing lands inside a group. Fork servers report no workspace `worktree`
//! (worktrees open as tabs), so there every workspace moves on its own.

use serde_json::{json, Value};

use crate::api::WorkspaceInfo;
use crate::layout::Line;

/// A herdr request: its method and params.
pub type Request = (&'static str, Value);

fn linked(workspace: &WorkspaceInfo) -> bool {
    workspace
        .worktree
        .as_ref()
        .is_some_and(|w| w.is_linked_worktree)
}

/// The workspaces a move can start from or land before, in herdr's order.
fn roots(workspaces: &[WorkspaceInfo]) -> Vec<&WorkspaceInfo> {
    workspaces.iter().filter(|w| !linked(w)).collect()
}

/// Whether a workspace moves on its own: a linked worktree goes with its repo's.
pub fn movable(workspaces: &[WorkspaceInfo], id: &str) -> bool {
    workspaces
        .iter()
        .any(|w| w.workspace_id == id && !linked(w))
}

/// The request that puts `source` just before `before` (`None`: after the last),
/// or `None` when it would stay put. Upstream's client `workspace_move_method`:
/// a grouped source takes its group along through `workspace.move_block`, a
/// plain one goes through `workspace.move`.
pub fn request(
    workspaces: &[WorkspaceInfo],
    source: &str,
    before: Option<&str>,
) -> Option<Request> {
    let workspace = workspaces.iter().find(|w| w.workspace_id == source)?;
    if linked(workspace) || before == Some(source) {
        return None;
    }
    let roots = roots(workspaces);
    let from = roots.iter().position(|w| w.workspace_id == source)?;
    let remaining: Vec<_> = roots.iter().filter(|w| w.workspace_id != source).collect();
    let to = match before {
        Some(target) => remaining.iter().position(|w| w.workspace_id == target)?,
        None => remaining.len(),
    };
    match &workspace.worktree {
        None if to == from => None,
        None => {
            // herdr's insert_index counts the source in, as the list stands
            let insert_index = before
                .and_then(|target| workspaces.iter().position(|w| w.workspace_id == target))
                .unwrap_or(workspaces.len());
            Some((
                "workspace.move",
                json!({ "workspace_id": source, "insert_index": insert_index }),
            ))
        }
        Some(group) => {
            let ids: Vec<&str> = std::iter::once(source)
                .chain(
                    workspaces
                        .iter()
                        .filter(|w| w.workspace_id != source)
                        .filter(|w| {
                            w.worktree
                                .as_ref()
                                .is_some_and(|g| g.repo_key == group.repo_key)
                        })
                        .map(|w| w.workspace_id.as_str()),
                )
                .collect();
            if before.is_some_and(|target| ids.contains(&target)) {
                return None;
            }
            let mut params = json!({ "workspace_ids": ids });
            if let Some(before) = before {
                params["before_workspace_id"] = json!(before);
            }
            Some(("workspace.move_block", params))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Up,
    Down,
}

/// The request that moves `source` one place up or down, or why there is none.
pub fn step(workspaces: &[WorkspaceInfo], source: &str, step: Step) -> Result<Request, String> {
    if !workspaces.iter().any(|w| w.workspace_id == source) {
        return Err(format!("no workspace {source}"));
    }
    if !movable(workspaces, source) {
        return Err("a linked worktree moves with its repo's workspace".into());
    }
    let roots = roots(workspaces);
    let at = roots
        .iter()
        .position(|w| w.workspace_id == source)
        .ok_or_else(|| format!("no workspace {source}"))?;
    let before = match step {
        Step::Up if at == 0 => return Err("already first".into()),
        Step::Up => Some(roots[at - 1].workspace_id.as_str()),
        Step::Down if at + 1 == roots.len() => return Err("already last".into()),
        Step::Down => roots.get(at + 2).map(|w| w.workspace_id.as_str()),
    };
    request(workspaces, source, before).ok_or_else(|| "already there".into())
}

/// Where a dragged header lands: before `before` (`None`: after the last
/// workspace). `line` is the list line it sits on: the header it goes above, or
/// the line under the last tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub before: Option<String>,
    pub line: usize,
}

fn header_id(line: &Line) -> Option<&str> {
    line.key.strip_prefix("ws:")
}

/// A line that belongs to no item: the gaps between trees and above `+ new`.
fn dead(line: &Line) -> bool {
    line.target.is_none() && line.spans.iter().all(|s| s.target.is_none())
}

/// The slots of a drawn list: above each header a move can land before, and
/// under the last tree.
pub fn slots(lines: &[Line], workspaces: &[WorkspaceInfo]) -> Vec<Slot> {
    let mut slots: Vec<Slot> = lines
        .iter()
        .enumerate()
        .filter_map(|(line, l)| {
            let id = header_id(l)?;
            movable(workspaces, id).then(|| Slot {
                before: Some(id.to_string()),
                line,
            })
        })
        .collect();
    let Some(first) = lines.iter().position(|l| header_id(l).is_some()) else {
        return slots;
    };
    let last_tree_line = lines[first..]
        .iter()
        .rposition(|l| !dead(l) && l.key != "new")
        .map_or(first, |at| first + at);
    slots.push(Slot {
        before: None,
        line: last_tree_line + 1,
    });
    slots
}

/// The slot whose edge is nearest list line `line`, the earlier on a tie; a
/// pointer below the list passes the list's length.
pub fn slot_at(slots: &[Slot], line: usize) -> Option<&Slot> {
    // in half lines: a slot is the edge above its line, the pointer the middle of its own
    slots
        .iter()
        .enumerate()
        .min_by_key(|(index, slot)| ((2 * slot.line).abs_diff(2 * line + 1), *index))
        .map(|(_, slot)| slot)
}

/// How a slot is shown: a rule over the dead line above its header (or the one
/// under the last tree), else, with no such line, its header highlighted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Rule(usize),
    Header(usize),
}

pub fn marker(lines: &[Line], slot: &Slot) -> Marker {
    let rule_line = match slot.before {
        Some(_) => slot.line.checked_sub(1),
        None => Some(slot.line),
    };
    match rule_line {
        Some(at) if lines.get(at).is_some_and(dead) => Marker::Rule(at),
        _ if slot.before.is_some() => Marker::Header(slot.line),
        _ => Marker::Rule(slot.line),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::WorkspaceWorktreeInfo;
    use crate::model::fixtures::workspace;

    fn grouped(id: &str, key: &str, linked: bool) -> WorkspaceInfo {
        WorkspaceInfo {
            worktree: Some(WorkspaceWorktreeInfo {
                repo_key: key.into(),
                is_linked_worktree: linked,
            }),
            ..workspace(id, id)
        }
    }

    fn plain(ids: &[&str]) -> Vec<WorkspaceInfo> {
        ids.iter().map(|id| workspace(id, id)).collect()
    }

    #[test]
    fn a_plain_workspace_moves_through_workspace_move() {
        let ws = plain(&["a", "b", "c"]);
        assert_eq!(
            request(&ws, "c", Some("a")),
            Some((
                "workspace.move",
                json!({ "workspace_id": "c", "insert_index": 0 })
            ))
        );
        // herdr counts the source in: before c, from a, is index 2
        assert_eq!(
            request(&ws, "a", Some("c")),
            Some((
                "workspace.move",
                json!({ "workspace_id": "a", "insert_index": 2 })
            ))
        );
        assert_eq!(
            request(&ws, "a", None),
            Some((
                "workspace.move",
                json!({ "workspace_id": "a", "insert_index": 3 })
            ))
        );
        // above itself or above its own successor, it stays put
        assert_eq!(request(&ws, "b", Some("b")), None);
        assert_eq!(request(&ws, "b", Some("c")), None);
        assert_eq!(request(&ws, "c", None), None);
        assert_eq!(request(&ws, "x", None), None);
    }

    #[test]
    fn a_repo_takes_its_worktrees_along_and_nothing_lands_inside_a_group() {
        let ws = vec![
            grouped("repo", "k", false),
            grouped("wt", "k", true),
            workspace("other", "other"),
            grouped("wt2", "k", true),
        ];
        assert_eq!(
            request(&ws, "repo", None),
            Some((
                "workspace.move_block",
                json!({ "workspace_ids": ["repo", "wt", "wt2"] })
            ))
        );
        assert_eq!(
            request(&ws, "other", Some("repo")),
            Some((
                "workspace.move",
                json!({ "workspace_id": "other", "insert_index": 0 })
            ))
        );
        // a linked worktree is no place to land, and never moves alone
        assert_eq!(request(&ws, "other", Some("wt")), None);
        assert_eq!(request(&ws, "wt", None), None);
        assert!(!movable(&ws, "wt"));
        assert_eq!(
            step(&ws, "wt", Step::Up).unwrap_err(),
            "a linked worktree moves with its repo's workspace"
        );
    }

    #[test]
    fn steps_walk_past_one_neighbour_and_stop_at_the_ends() {
        let ws = plain(&["a", "b", "c"]);
        assert_eq!(
            step(&ws, "b", Step::Up).unwrap().1,
            json!({ "workspace_id": "b", "insert_index": 0 })
        );
        assert_eq!(
            step(&ws, "b", Step::Down).unwrap().1,
            json!({ "workspace_id": "b", "insert_index": 3 })
        );
        assert_eq!(
            step(&ws, "a", Step::Down).unwrap().1,
            json!({ "workspace_id": "a", "insert_index": 2 })
        );
        assert_eq!(step(&ws, "a", Step::Up).unwrap_err(), "already first");
        assert_eq!(step(&ws, "c", Step::Down).unwrap_err(), "already last");
        // a group is one step, whatever its size
        let ws = vec![
            workspace("a", "a"),
            grouped("repo", "k", false),
            grouped("wt", "k", true),
            workspace("c", "c"),
        ];
        assert_eq!(
            step(&ws, "c", Step::Up).unwrap().1,
            json!({ "workspace_id": "c", "insert_index": 1 })
        );
        assert_eq!(
            step(&ws, "repo", Step::Down).unwrap().1,
            json!({ "workspace_ids": ["repo", "wt"] })
        );
    }

    fn lines(keys: &[&str]) -> Vec<Line> {
        use crate::layout::Target;
        keys.iter()
            .map(|key| Line {
                spans: Vec::new(),
                key: key.to_string(),
                target: match *key {
                    "gap" => None,
                    "new" => Some(Target::NewWorkspace),
                    key => Some(Target::FocusPane(key.to_string())),
                },
            })
            .collect()
    }

    const LIST: &[&str] = &[
        "ws:a", "pane:a1", "pane:a2", "gap", "ws:b", "gap", "ws:c", "pane:c1", "gap", "new",
    ];

    fn before(line: usize, ws: &[WorkspaceInfo]) -> Option<String> {
        let lines = lines(LIST);
        slot_at(&slots(&lines, ws), line).unwrap().before.clone()
    }

    #[test]
    fn a_drop_lands_above_the_header_under_it_or_after_the_last_tree() {
        let ws = plain(&["a", "b", "c"]);
        let lines = lines(LIST);
        assert_eq!(
            slots(&lines, &ws)
                .iter()
                .map(|s| s.line)
                .collect::<Vec<_>>(),
            [0, 4, 6, 8]
        );
        // onto a header: above it
        assert_eq!(before(0, &ws).as_deref(), Some("a"));
        assert_eq!(before(4, &ws).as_deref(), Some("b"));
        assert_eq!(before(6, &ws).as_deref(), Some("c"));
        // the gap above a header belongs to it
        assert_eq!(before(3, &ws).as_deref(), Some("b"));
        assert_eq!(before(5, &ws).as_deref(), Some("c"));
        // inside a tree: the nearer edge, so a tree's lower rows mean "after it"
        assert_eq!(before(1, &ws).as_deref(), Some("a"));
        assert_eq!(before(2, &ws).as_deref(), Some("b"));
        assert_eq!(before(7, &ws), None);
        // below the last tree, on `+ new` or past the list: after the last
        assert_eq!(before(8, &ws), None);
        assert_eq!(before(9, &ws), None);
        assert_eq!(before(40, &ws), None);
    }

    #[test]
    fn a_linked_worktrees_header_is_no_slot() {
        let ws = vec![
            workspace("a", "a"),
            grouped("b", "k", true),
            grouped("c", "k", false),
        ];
        let lines = lines(LIST);
        assert_eq!(
            slots(&lines, &ws)
                .iter()
                .map(|s| s.line)
                .collect::<Vec<_>>(),
            [0, 6, 8]
        );
        // onto the linked header: the nearest edge a move may use
        assert_eq!(before(4, &ws).as_deref(), Some("c"));
        assert_eq!(before(1, &ws).as_deref(), Some("a"));
    }

    #[test]
    fn the_marker_rules_the_gap_above_or_lights_a_header_with_none() {
        let ws = plain(&["a", "b", "c"]);
        let lines = lines(LIST);
        let slots = slots(&lines, &ws);
        assert_eq!(marker(&lines, &slots[0]), Marker::Header(0));
        assert_eq!(marker(&lines, &slots[1]), Marker::Rule(3));
        assert_eq!(marker(&lines, &slots[3]), Marker::Rule(8));
    }
}
