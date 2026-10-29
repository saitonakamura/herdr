//! Agent waits: what an agent waits for, with a deadline, persisted with the session.
//!
//! Pure data. Firing, which resolves deciders and queues messages, lives in `api::waits`.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::api::schema::{
    TaskStatus, WaitDecider, WaitFiredInfo, WaitInfo, WaitNeedsUserInfo, WaitSourceInfo,
    WaitState as ApiState,
};
use crate::terminal::AgentId;

/// Deadline extensions a wait (and the waits that re-arm it) may use before it escalates.
pub(crate) const MAX_EXTENSIONS: u8 = 2;
pub(crate) const LIVE_PER_WAITER_LIMIT: u32 = 32;
pub(crate) const LIVE_LIMIT: usize = 1024;
/// Finished waits kept for `wait list`; live ones are never evicted.
pub(crate) const FINISHED_HISTORY_LIMIT: usize = 500;
/// The source kind of a wait on another agent; its target is that agent's id.
pub(crate) const NODE_SOURCE: &str = "node";
/// Kinds herdr keeps for itself; a plugin reader cannot claim them.
pub(crate) const RESERVED_SOURCES: &[&str] = &[NODE_SOURCE, "run"];
/// What a node wait waits for when `until` is left out.
pub(crate) const NODE_UNTIL_DEFAULT: TaskStatus = TaskStatus::Complete;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WaitSource {
    pub kind: String,
    pub target: String,
}

impl WaitSource {
    /// The agent a node wait watches.
    pub(crate) fn node(&self) -> Option<AgentId> {
        (self.kind == NODE_SOURCE)
            .then(|| AgentId::parse(&self.target))
            .flatten()
    }
}

/// What a node's new task status (or its close, as `closed`) does to a wait on it, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NodeOutcome {
    Satisfied(String),
    Fired(String),
}

/// The task statuses a node wait may wait for: a terminal one, or a call for help.
pub(crate) fn node_until(until: Option<&str>) -> Option<TaskStatus> {
    let Some(until) = until else {
        return Some(NODE_UNTIL_DEFAULT);
    };
    let status: TaskStatus =
        serde_json::from_value(serde_json::Value::String(until.into())).ok()?;
    matches!(
        status,
        TaskStatus::Complete
            | TaskStatus::Failed
            | TaskStatus::Closed
            | TaskStatus::NeedsParent
            | TaskStatus::NeedsUser
    )
    .then_some(status)
}

/// Reaching `until` satisfies the wait; ending any other way fires it; anything else waits on.
pub(crate) fn node_outcome(
    node: &AgentId,
    until: TaskStatus,
    status: TaskStatus,
) -> Option<NodeOutcome> {
    let reason = match status {
        TaskStatus::Closed => format!("node {node} closed"),
        status => format!("node {node} reported {}", task_status_name(status)),
    };
    if status == until {
        return Some(NodeOutcome::Satisfied(reason));
    }
    matches!(
        status,
        TaskStatus::Closed | TaskStatus::Complete | TaskStatus::Failed
    )
    .then_some(NodeOutcome::Fired(reason))
}

pub(crate) fn task_status_name(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Starting => "starting",
        TaskStatus::Running => "running",
        TaskStatus::NeedsParent => "needs_parent",
        TaskStatus::NeedsUser => "needs_user",
        TaskStatus::Complete => "complete",
        TaskStatus::Failed => "failed",
        TaskStatus::Closed => "closed",
    }
}

/// Who a fire went to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WaitRecipient {
    Agent(AgentId),
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum WaitState {
    Live,
    Fired {
        at_ms: u64,
        reason: String,
        to: Vec<WaitRecipient>,
        #[serde(default, skip_serializing_if = "is_false")]
        escalated: bool,
    },
    Satisfied {
        at_ms: u64,
    },
    Cancelled {
        at_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Wait {
    pub id: u64,
    pub waiter: AgentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<WaitSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    pub summary: String,
    pub deadline_at_ms: u64,
    pub then: WaitDecider,
    #[serde(default)]
    pub extensions: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rearm_of: Option<u64>,
    pub created_at_ms: u64,
    pub state: WaitState,
    /// Fired to the user, who has not dealt with it yet.
    #[serde(default, skip_serializing_if = "is_false")]
    pub needs_user: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ok_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_state: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub errors: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl Wait {
    pub(crate) fn is_live(&self) -> bool {
        self.state == WaitState::Live
    }

    pub(crate) fn info(&self) -> WaitInfo {
        let (state, finished_at_ms, fired) = match &self.state {
            WaitState::Live => (ApiState::Live, None, None),
            WaitState::Fired {
                at_ms,
                reason,
                to,
                escalated,
            } => (
                ApiState::Fired,
                Some(*at_ms),
                Some(WaitFiredInfo {
                    reason: reason.clone(),
                    to: to
                        .iter()
                        .map(|recipient| match recipient {
                            WaitRecipient::Agent(agent_id) => agent_id.to_string(),
                            WaitRecipient::User => "user".to_owned(),
                        })
                        .collect(),
                    escalated: *escalated,
                }),
            ),
            WaitState::Satisfied { at_ms } => (ApiState::Satisfied, Some(*at_ms), None),
            WaitState::Cancelled { at_ms } => (ApiState::Cancelled, Some(*at_ms), None),
        };
        WaitInfo {
            wait_id: self.id,
            waiter_agent_id: self.waiter.to_string(),
            state,
            summary: self.summary.clone(),
            on: self.on.as_ref().map(|on| WaitSourceInfo {
                kind: on.kind.clone(),
                target: on.target.clone(),
            }),
            until: self.until.clone(),
            deadline_at_ms: self.deadline_at_ms,
            then: self.then,
            extensions: self.extensions,
            rearm_of: self.rearm_of,
            created_at_ms: self.created_at_ms,
            finished_at_ms,
            fired,
            needs_user: self.needs_user,
            last_ok_at_ms: self.last_ok_at_ms,
            last_state: self.last_state.clone(),
            errors: self.errors,
            last_error: self.last_error.clone(),
        }
    }

    pub(crate) fn needs_user_info(&self) -> Option<WaitNeedsUserInfo> {
        match &self.state {
            WaitState::Fired { reason, .. } if self.needs_user => Some(WaitNeedsUserInfo {
                wait_id: self.id,
                reason: reason.clone(),
            }),
            _ => None,
        }
    }
}

/// What a declare asks for; the store assigns the id and checks the limits.
pub(crate) struct NewWait {
    pub waiter: AgentId,
    pub on: Option<WaitSource>,
    pub until: Option<String>,
    pub summary: String,
    pub deadline_at_ms: u64,
    /// Required unless `rearm` names a wait to inherit it from.
    pub then: Option<WaitDecider>,
    pub rearm: Option<u64>,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeclareError {
    WaiterLimit,
    SessionLimit,
    MissingDecider,
    RearmNotFound,
    /// Only a fired wait of the same waiter can be re-armed.
    RearmNotFired,
    RearmOtherWaiter,
    /// A re-arm inherits `then`; asking for another one is refused rather than ignored.
    RearmDeciderMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeError {
    NotFound,
    NotLive,
    ExtensionLimit,
}

/// The persisted part of the store; everything else is an index rebuilt from it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
struct WaitRecords {
    #[serde(default)]
    next_id: u64,
    /// Oldest first; ids only grow.
    #[serde(default)]
    waits: Vec<Wait>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct WaitIndex {
    /// `(deadline_at_ms, id)` of every live wait, so the next deadline is the first entry.
    deadlines: BTreeSet<(u64, u64)>,
    live_per_waiter: HashMap<AgentId, u32>,
    needs_user_per_waiter: HashMap<AgentId, u32>,
    /// Live node waits by the agent they watch, so a status change touches only its own.
    live_on_node: HashMap<AgentId, BTreeSet<u64>>,
}

impl WaitIndex {
    fn add(&mut self, wait: &Wait) {
        if wait.is_live() {
            self.deadlines.insert((wait.deadline_at_ms, wait.id));
            *self.live_per_waiter.entry(wait.waiter.clone()).or_default() += 1;
            if let Some(node) = wait.on.as_ref().and_then(WaitSource::node) {
                self.live_on_node.entry(node).or_default().insert(wait.id);
            }
        }
        if wait.needs_user {
            *self
                .needs_user_per_waiter
                .entry(wait.waiter.clone())
                .or_default() += 1;
        }
    }

    fn remove(&mut self, wait: &Wait) {
        if wait.is_live() {
            self.deadlines.remove(&(wait.deadline_at_ms, wait.id));
            decrement(&mut self.live_per_waiter, &wait.waiter);
            if let Some(node) = wait.on.as_ref().and_then(WaitSource::node) {
                if let Some(ids) = self.live_on_node.get_mut(&node) {
                    ids.remove(&wait.id);
                    if ids.is_empty() {
                        self.live_on_node.remove(&node);
                    }
                }
            }
        }
        if wait.needs_user {
            decrement(&mut self.needs_user_per_waiter, &wait.waiter);
        }
    }
}

fn decrement(counts: &mut HashMap<AgentId, u32>, key: &AgentId) {
    if let Some(count) = counts.get_mut(key) {
        *count -= 1;
        if *count == 0 {
            counts.remove(key);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "WaitRecords", into = "WaitRecords")]
pub(crate) struct WaitStore {
    records: WaitRecords,
    index: WaitIndex,
}

impl From<WaitRecords> for WaitStore {
    fn from(mut records: WaitRecords) -> Self {
        records.waits.sort_by_key(|wait| wait.id);
        records.waits.dedup_by_key(|wait| wait.id);
        let mut index = WaitIndex::default();
        for wait in &records.waits {
            index.add(wait);
        }
        Self { records, index }
    }
}

impl From<WaitStore> for WaitRecords {
    fn from(store: WaitStore) -> Self {
        store.records
    }
}

impl WaitStore {
    pub(crate) fn is_empty(&self) -> bool {
        self.records.waits.is_empty()
    }

    pub(crate) fn get(&self, id: u64) -> Option<&Wait> {
        self.position(id).map(|index| &self.records.waits[index])
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Wait> {
        self.records.waits.iter()
    }

    pub(crate) fn live_count(&self, waiter: &AgentId) -> u32 {
        self.index.live_per_waiter.get(waiter).copied().unwrap_or(0)
    }

    /// The waiter's newest wait that fired to the user and still needs them.
    pub(crate) fn needs_user(&self, waiter: &AgentId) -> Option<&Wait> {
        if !self.index.needs_user_per_waiter.contains_key(waiter) {
            return None;
        }
        self.records
            .waits
            .iter()
            .rev()
            .find(|wait| wait.needs_user && &wait.waiter == waiter)
    }

    pub(crate) fn declare(&mut self, new: NewWait) -> Result<&Wait, DeclareError> {
        if self.index.deadlines.len() >= LIVE_LIMIT {
            return Err(DeclareError::SessionLimit);
        }
        if self.live_count(&new.waiter) >= LIVE_PER_WAITER_LIMIT {
            return Err(DeclareError::WaiterLimit);
        }
        let (then, extensions) = match new.rearm {
            Some(rearm) => {
                let old = self.get(rearm).ok_or(DeclareError::RearmNotFound)?;
                if old.waiter != new.waiter {
                    return Err(DeclareError::RearmOtherWaiter);
                }
                if !matches!(old.state, WaitState::Fired { .. }) {
                    return Err(DeclareError::RearmNotFired);
                }
                if new.then.is_some_and(|then| then != old.then) {
                    return Err(DeclareError::RearmDeciderMismatch);
                }
                (old.then, old.extensions)
            }
            None => (new.then.ok_or(DeclareError::MissingDecider)?, 0),
        };
        if let Some(rearm) = new.rearm {
            self.modify(rearm, |old| old.needs_user = false);
        }
        let id = self.records.next_id.max(self.last_id()) + 1;
        self.records.next_id = id;
        let wait = Wait {
            id,
            waiter: new.waiter,
            on: new.on,
            until: new.until,
            summary: new.summary,
            deadline_at_ms: new.deadline_at_ms,
            then,
            extensions,
            rearm_of: new.rearm,
            created_at_ms: new.created_at_ms,
            state: WaitState::Live,
            needs_user: false,
            last_ok_at_ms: None,
            last_state: None,
            errors: 0,
            last_error: None,
        };
        self.index.add(&wait);
        self.records.waits.push(wait);
        Ok(&self.records.waits[self.records.waits.len() - 1])
    }

    pub(crate) fn extend(&mut self, id: u64, deadline_at_ms: u64) -> Result<&Wait, ChangeError> {
        let wait = self.get(id).ok_or(ChangeError::NotFound)?;
        if !wait.is_live() {
            return Err(ChangeError::NotLive);
        }
        if wait.extensions >= MAX_EXTENSIONS {
            return Err(ChangeError::ExtensionLimit);
        }
        self.modify(id, |wait| {
            wait.deadline_at_ms = deadline_at_ms;
            wait.extensions += 1;
        });
        self.get(id).ok_or(ChangeError::NotFound)
    }

    /// Cancels a live wait. A fired one that still needs the user is cleared instead, and stays fired.
    pub(crate) fn cancel(&mut self, id: u64, at_ms: u64) -> Result<&Wait, ChangeError> {
        let wait = self.get(id).ok_or(ChangeError::NotFound)?;
        if wait.is_live() {
            self.modify(id, |wait| wait.state = WaitState::Cancelled { at_ms });
            self.evict_finished();
        } else if wait.needs_user {
            self.modify(id, |wait| wait.needs_user = false);
        } else {
            return Err(ChangeError::NotLive);
        }
        self.get(id).ok_or(ChangeError::NotFound)
    }

    pub(crate) fn next_deadline_ms(&self) -> Option<u64> {
        self.index.deadlines.first().map(|&(deadline, _)| deadline)
    }

    /// Live waits whose deadline is at or before `now_ms`, earliest first.
    pub(crate) fn due(&self, now_ms: u64) -> Vec<u64> {
        self.index
            .deadlines
            .range(..=(now_ms, u64::MAX))
            .map(|&(_, id)| id)
            .collect()
    }

    /// Marks a live wait fired; a fire that reached the user leaves it needing them.
    pub(crate) fn fire(
        &mut self,
        id: u64,
        at_ms: u64,
        reason: String,
        to: Vec<WaitRecipient>,
        escalated: bool,
    ) -> Option<&Wait> {
        if !self.get(id)?.is_live() {
            return None;
        }
        self.modify(id, |wait| {
            wait.needs_user = to.contains(&WaitRecipient::User);
            wait.state = WaitState::Fired {
                at_ms,
                reason,
                to,
                escalated,
            };
        });
        self.evict_finished();
        self.get(id)
    }

    /// Records a reader's answer on a live wait; returns the errors now in a row.
    pub(crate) fn record_read(
        &mut self,
        id: u64,
        at_ms: u64,
        read: Result<String, String>,
    ) -> Option<u32> {
        if !self.get(id)?.is_live() {
            return None;
        }
        self.modify(id, |wait| match read {
            Ok(state) => {
                wait.last_ok_at_ms = Some(at_ms);
                wait.last_state = Some(state);
                wait.errors = 0;
                wait.last_error = None;
            }
            Err(error) => {
                wait.errors = wait.errors.saturating_add(1);
                wait.last_error = Some(error);
            }
        });
        self.get(id).map(|wait| wait.errors)
    }

    /// Marks a live wait satisfied: its source reached the state it waited for.
    pub(crate) fn satisfy(&mut self, id: u64, at_ms: u64) -> Option<&Wait> {
        if !self.get(id)?.is_live() {
            return None;
        }
        self.modify(id, |wait| wait.state = WaitState::Satisfied { at_ms });
        self.evict_finished();
        self.get(id)
    }

    /// The live waits on `node`, oldest first.
    pub(crate) fn live_on_node(&self, node: &AgentId) -> Vec<u64> {
        self.index
            .live_on_node
            .get(node)
            .map(|ids| ids.iter().copied().collect())
            .unwrap_or_default()
    }

    /// The live node waits whose node is not in `live`, oldest first.
    pub(crate) fn live_on_nodes_not_in(&self, live: &HashSet<AgentId>) -> Vec<u64> {
        let mut ids: Vec<u64> = self
            .index
            .live_on_node
            .iter()
            .filter(|(node, _)| !live.contains(*node))
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect();
        ids.sort_unstable();
        ids
    }

    /// Cancels the live waits of waiters that are gone, and drops what they needed from the user.
    /// Returns the cancelled ids, and whether anything changed.
    pub(crate) fn cancel_waiters_not_in(
        &mut self,
        live: &HashSet<AgentId>,
        at_ms: u64,
    ) -> (Vec<u64>, bool) {
        let gone =
            |counts: &HashMap<AgentId, u32>| counts.keys().any(|waiter| !live.contains(waiter));
        if !gone(&self.index.live_per_waiter) && !gone(&self.index.needs_user_per_waiter) {
            return (Vec::new(), false);
        }
        let ids: Vec<u64> = self
            .records
            .waits
            .iter()
            .filter(|wait| (wait.is_live() || wait.needs_user) && !live.contains(&wait.waiter))
            .map(|wait| wait.id)
            .collect();
        let mut cancelled = Vec::new();
        for &id in &ids {
            self.modify(id, |wait| {
                wait.needs_user = false;
                if wait.is_live() {
                    wait.state = WaitState::Cancelled { at_ms };
                    cancelled.push(id);
                }
            });
        }
        self.evict_finished();
        (cancelled, true)
    }

    /// Clears what the waiter's fired waits needed from the user; returns whether any did.
    pub(crate) fn clear_needs_user(&mut self, waiter: &AgentId) -> bool {
        if !self.index.needs_user_per_waiter.contains_key(waiter) {
            return false;
        }
        let ids: Vec<u64> = self
            .records
            .waits
            .iter()
            .filter(|wait| wait.needs_user && &wait.waiter == waiter)
            .map(|wait| wait.id)
            .collect();
        for id in ids {
            self.modify(id, |wait| wait.needs_user = false);
        }
        true
    }

    fn modify(&mut self, id: u64, change: impl FnOnce(&mut Wait)) {
        let Some(position) = self.position(id) else {
            return;
        };
        let wait = &mut self.records.waits[position];
        self.index.remove(wait);
        change(wait);
        self.index.add(wait);
    }

    fn position(&self, id: u64) -> Option<usize> {
        self.records
            .waits
            .binary_search_by_key(&id, |wait| wait.id)
            .ok()
    }

    fn evict_finished(&mut self) {
        let finished = self.records.waits.len() - self.index.deadlines.len();
        let mut excess = finished.saturating_sub(FINISHED_HISTORY_LIMIT);
        if excess == 0 {
            return;
        }
        let index = &mut self.index;
        self.records.waits.retain(|wait| {
            if excess > 0 && !wait.is_live() {
                excess -= 1;
                index.remove(wait);
                false
            } else {
                true
            }
        });
    }

    fn last_id(&self) -> u64 {
        self.records.waits.last().map_or(0, |wait| wait.id)
    }

    /// Moves a live wait's deadline without counting an extension, as time passing would.
    #[cfg(test)]
    pub(crate) fn set_deadline_for_test(&mut self, id: u64, deadline_at_ms: u64) {
        self.modify(id, |wait| wait.deadline_at_ms = deadline_at_ms);
    }

    #[cfg(test)]
    pub(crate) fn assert_invariants_for_test(&self) {
        let waits = &self.records.waits;
        assert!(
            waits.windows(2).all(|pair| pair[0].id < pair[1].id),
            "wait ids must strictly grow"
        );
        assert!(
            self.records.next_id >= self.last_id(),
            "next wait id {} is behind the newest wait {}",
            self.records.next_id,
            self.last_id()
        );
        let rebuilt = WaitStore::from(self.records.clone());
        assert_eq!(
            rebuilt.index, self.index,
            "wait index drifted from the records"
        );
        assert!(self.index.deadlines.len() <= LIVE_LIMIT);
        assert!(waits.len() - self.index.deadlines.len() <= FINISHED_HISTORY_LIMIT);
        assert!(waits.iter().all(|wait| wait.extensions <= MAX_EXTENSIONS));
        assert!(
            waits
                .iter()
                .all(|wait| !wait.needs_user || matches!(wait.state, WaitState::Fired { .. })),
            "only a fired wait can need the user"
        );
    }
}

/// `90s`, `30m`, `2h`, `1d` or `250ms`, in milliseconds.
pub(crate) fn parse_duration_ms(value: &str) -> Option<u64> {
    let unit_at = value.find(|ch: char| !ch.is_ascii_digit())?;
    let (count, unit) = value.split_at(unit_at);
    let count: u64 = count.parse().ok()?;
    let unit_ms = match unit {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => return None,
    };
    count.checked_mul(unit_ms)
}

/// Unix epoch ms now; deadlines are kept in wall-clock time so they survive a restart.
pub(crate) fn epoch_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str) -> AgentId {
        AgentId::parse(id).unwrap()
    }

    fn new(waiter: &str, deadline_at_ms: u64) -> NewWait {
        NewWait {
            waiter: agent(waiter),
            on: None,
            until: None,
            summary: "ci".into(),
            deadline_at_ms,
            then: Some(WaitDecider::Parent),
            rearm: None,
            created_at_ms: 1,
        }
    }

    fn fire(store: &mut WaitStore, id: u64) {
        store
            .fire(
                id,
                50,
                "deadline passed".into(),
                vec![WaitRecipient::User],
                false,
            )
            .unwrap();
    }

    #[test]
    fn ids_grow_and_are_not_reused() {
        let mut store = WaitStore::default();
        let first = store.declare(new("1111aaaa", 10)).unwrap().id;
        let second = store.declare(new("1111aaaa", 20)).unwrap().id;
        assert_eq!((first, second), (1, 2));
        store.cancel(second, 5).unwrap();
        let mut restored: WaitStore =
            serde_json::from_str(&serde_json::to_string(&store).unwrap()).unwrap();
        assert_eq!(restored.declare(new("1111aaaa", 30)).unwrap().id, 3);
        restored.assert_invariants_for_test();
    }

    #[test]
    fn the_index_follows_declare_extend_cancel_and_fire() {
        let mut store = WaitStore::default();
        let late = store.declare(new("1111aaaa", 300)).unwrap().id;
        let early = store.declare(new("2222bbbb", 100)).unwrap().id;
        let middle = store.declare(new("1111aaaa", 200)).unwrap().id;
        store.assert_invariants_for_test();
        assert_eq!(store.next_deadline_ms(), Some(100));
        assert_eq!(store.due(99), Vec::<u64>::new());
        assert_eq!(store.due(200), vec![early, middle]);
        assert_eq!(store.live_count(&agent("1111aaaa")), 2);

        store.extend(early, 400).unwrap();
        store.assert_invariants_for_test();
        assert_eq!(store.next_deadline_ms(), Some(200));
        assert_eq!(store.due(400), vec![middle, late, early]);

        store.cancel(middle, 5).unwrap();
        fire(&mut store, late);
        store.assert_invariants_for_test();
        assert_eq!(store.due(u64::MAX), vec![early]);
        assert_eq!(store.live_count(&agent("1111aaaa")), 0);
        assert_eq!(
            store.needs_user(&agent("1111aaaa")).map(|wait| wait.id),
            Some(late)
        );
        assert!(store
            .fire(late, 60, "again".into(), Vec::new(), false)
            .is_none());
    }

    #[test]
    fn extensions_stop_at_two() {
        let mut store = WaitStore::default();
        let id = store.declare(new("1111aaaa", 10)).unwrap().id;
        assert_eq!(store.extend(id, 20).unwrap().extensions, 1);
        assert_eq!(store.extend(id, 30).unwrap().extensions, 2);
        assert_eq!(store.extend(id, 40), Err(ChangeError::ExtensionLimit));
        assert_eq!(store.get(id).unwrap().deadline_at_ms, 30);
        assert_eq!(store.extend(99, 40), Err(ChangeError::NotFound));
        store.cancel(id, 5).unwrap();
        assert_eq!(store.extend(id, 40), Err(ChangeError::NotLive));
        assert_eq!(store.cancel(id, 6), Err(ChangeError::NotLive));
    }

    #[test]
    fn live_waits_are_limited_per_waiter_and_per_session() {
        let mut store = WaitStore::default();
        for _ in 0..LIVE_PER_WAITER_LIMIT {
            store.declare(new("1111aaaa", 10)).unwrap();
        }
        assert_eq!(
            store.declare(new("1111aaaa", 10)).err(),
            Some(DeclareError::WaiterLimit)
        );
        let first = store.iter().next().unwrap().id;
        store.cancel(first, 5).unwrap();
        assert!(store.declare(new("1111aaaa", 10)).is_ok());

        let mut store = WaitStore::default();
        for index in 0..LIVE_LIMIT {
            store
                .declare(new(&format!("{:08x}", 0x1000_0000 + index / 32), 10))
                .unwrap();
        }
        assert_eq!(
            store.declare(new("9999ffff", 10)).err(),
            Some(DeclareError::SessionLimit)
        );
        store.assert_invariants_for_test();
    }

    #[test]
    fn finished_history_is_capped_and_live_waits_are_never_evicted() {
        let mut store = WaitStore::default();
        let live = store.declare(new("1111aaaa", u64::MAX)).unwrap().id;
        for _ in 0..FINISHED_HISTORY_LIMIT + 5 {
            let id = store.declare(new("2222bbbb", 10)).unwrap().id;
            store.cancel(id, 5).unwrap();
        }
        store.assert_invariants_for_test();
        assert!(store.get(live).is_some_and(Wait::is_live));
        assert_eq!(
            store.iter().filter(|wait| !wait.is_live()).count(),
            FINISHED_HISTORY_LIMIT
        );
        assert_eq!(
            store.iter().find(|wait| !wait.is_live()).unwrap().id,
            live + 6,
            "the oldest finished go first"
        );
    }

    #[test]
    fn a_rearm_inherits_extensions_and_decider_and_clears_the_old_flag() {
        let mut store = WaitStore::default();
        let first = store.declare(new("1111aaaa", 10)).unwrap().id;
        store.extend(first, 20).unwrap();
        store.extend(first, 30).unwrap();
        fire(&mut store, first);

        let mut rearm = new("1111aaaa", 100);
        rearm.then = None;
        rearm.rearm = Some(first);
        let second = store.declare(rearm).unwrap();
        assert_eq!(
            (second.extensions, second.then, second.rearm_of),
            (MAX_EXTENSIONS, WaitDecider::Parent, Some(first))
        );
        let second = second.id;
        assert_ne!(second, first);
        assert!(!store.get(first).unwrap().needs_user);
        assert_eq!(store.extend(second, 200), Err(ChangeError::ExtensionLimit));
        store.assert_invariants_for_test();
    }

    #[test]
    fn a_rearm_must_name_a_fired_wait_of_the_same_waiter() {
        let mut store = WaitStore::default();
        let live = store.declare(new("1111aaaa", 10)).unwrap().id;
        let fired = store.declare(new("1111aaaa", 10)).unwrap().id;
        fire(&mut store, fired);
        let rearm = |id, waiter: &str, then| {
            let mut wait = new(waiter, 100);
            wait.rearm = Some(id);
            wait.then = then;
            wait
        };
        let mut missing = new("1111aaaa", 100);
        missing.then = None;
        for (wait, expected) in [
            (rearm(99, "1111aaaa", None), DeclareError::RearmNotFound),
            (rearm(live, "1111aaaa", None), DeclareError::RearmNotFired),
            (
                rearm(fired, "2222bbbb", None),
                DeclareError::RearmOtherWaiter,
            ),
            (
                rearm(fired, "1111aaaa", Some(WaitDecider::User)),
                DeclareError::RearmDeciderMismatch,
            ),
            (missing, DeclareError::MissingDecider),
        ] {
            assert_eq!(store.declare(wait).err(), Some(expected));
        }
        assert!(store
            .declare(rearm(fired, "1111aaaa", Some(WaitDecider::Parent)))
            .is_ok());
    }

    #[test]
    fn cancelling_a_fired_wait_clears_what_it_needed_from_the_user() {
        let mut store = WaitStore::default();
        let id = store.declare(new("1111aaaa", 10)).unwrap().id;
        fire(&mut store, id);
        let wait = store.cancel(id, 60).unwrap();
        assert!(matches!(wait.state, WaitState::Fired { .. }));
        assert!(!wait.needs_user);
        assert_eq!(store.cancel(id, 61), Err(ChangeError::NotLive));
        store.assert_invariants_for_test();
    }

    #[test]
    fn a_gone_waiter_has_its_live_waits_cancelled_and_flags_cleared() {
        let mut store = WaitStore::default();
        let gone_live = store.declare(new("1111aaaa", 10)).unwrap().id;
        let gone_fired = store.declare(new("1111aaaa", 10)).unwrap().id;
        let kept = store.declare(new("2222bbbb", 10)).unwrap().id;
        fire(&mut store, gone_fired);
        let live = HashSet::from([agent("2222bbbb")]);

        assert_eq!(
            store.cancel_waiters_not_in(&live, 70),
            (vec![gone_live], true)
        );
        assert_eq!(
            store.get(gone_live).unwrap().state,
            WaitState::Cancelled { at_ms: 70 }
        );
        assert!(store.needs_user(&agent("1111aaaa")).is_none());
        assert!(store.get(kept).unwrap().is_live());
        assert_eq!(store.cancel_waiters_not_in(&live, 80), (Vec::new(), false));
        store.assert_invariants_for_test();
    }

    #[test]
    fn a_report_clears_what_the_waiter_needed_from_the_user() {
        let mut store = WaitStore::default();
        let id = store.declare(new("1111aaaa", 10)).unwrap().id;
        assert!(!store.clear_needs_user(&agent("1111aaaa")));
        fire(&mut store, id);
        assert!(store.clear_needs_user(&agent("1111aaaa")));
        assert!(store.needs_user(&agent("1111aaaa")).is_none());
        store.assert_invariants_for_test();
    }

    #[test]
    fn the_store_round_trips_through_json_and_rebuilds_its_index() {
        let mut store = WaitStore::default();
        let fired = store.declare(new("1111aaaa", 10)).unwrap().id;
        let mut on = new("2222bbbb", 20);
        on.on = Some(WaitSource {
            kind: "node".into(),
            target: "3333cccc".into(),
        });
        on.until = Some("done".into());
        store.declare(on).unwrap();
        fire(&mut store, fired);

        let json = serde_json::to_string(&store).unwrap();
        assert!(
            !json.contains("deadlines"),
            "the index is not persisted: {json}"
        );
        let restored: WaitStore = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, store);
        restored.assert_invariants_for_test();
        assert_eq!(
            serde_json::from_str::<WaitStore>("{}").unwrap(),
            WaitStore::default()
        );
    }

    fn on_node(waiter: &str, node: &str) -> NewWait {
        let mut wait = new(waiter, 10);
        wait.on = Some(WaitSource {
            kind: NODE_SOURCE.into(),
            target: node.into(),
        });
        wait.until = Some("complete".into());
        wait
    }

    #[test]
    fn the_node_index_holds_live_node_waits_only() {
        let mut store = WaitStore::default();
        let (node, other) = (agent("3333cccc"), agent("4444dddd"));
        let satisfied = store.declare(on_node("1111aaaa", "3333cccc")).unwrap().id;
        let fired = store.declare(on_node("2222bbbb", "3333cccc")).unwrap().id;
        let elsewhere = store.declare(on_node("1111aaaa", "4444dddd")).unwrap().id;
        store.declare(new("1111aaaa", 10)).unwrap();
        assert_eq!(store.live_on_node(&node), vec![satisfied, fired]);

        store.satisfy(satisfied, 30).unwrap();
        fire(&mut store, fired);
        assert!(store.live_on_node(&node).is_empty());
        assert!(
            store.satisfy(satisfied, 40).is_none(),
            "a finished wait stays finished"
        );
        assert!(matches!(
            store.get(satisfied).unwrap().state,
            WaitState::Satisfied { at_ms: 30 }
        ));
        assert_eq!(
            store.live_on_nodes_not_in(&HashSet::from([node.clone()])),
            vec![elsewhere]
        );
        assert!(store
            .live_on_nodes_not_in(&HashSet::from([node, other]))
            .is_empty());
        store.assert_invariants_for_test();
    }

    #[test]
    fn a_node_wait_waits_for_a_terminal_status_or_a_call_for_help() {
        assert_eq!(node_until(None), Some(TaskStatus::Complete));
        for until in ["complete", "failed", "closed", "needs_parent", "needs_user"] {
            assert!(node_until(Some(until)).is_some(), "{until}");
        }
        for until in ["running", "starting", "done", "merged", ""] {
            assert_eq!(node_until(Some(until)), None, "{until}");
        }
    }

    #[test]
    fn reaching_until_satisfies_and_ending_otherwise_fires() {
        let node = agent("3333cccc");
        let outcome = |until, status| node_outcome(&node, until, status);
        assert_eq!(
            outcome(TaskStatus::Complete, TaskStatus::Complete),
            Some(NodeOutcome::Satisfied(
                "node 3333cccc reported complete".into()
            ))
        );
        assert_eq!(
            outcome(TaskStatus::Complete, TaskStatus::Failed),
            Some(NodeOutcome::Fired("node 3333cccc reported failed".into()))
        );
        assert_eq!(
            outcome(TaskStatus::Complete, TaskStatus::Closed),
            Some(NodeOutcome::Fired("node 3333cccc closed".into()))
        );
        assert_eq!(
            outcome(TaskStatus::Closed, TaskStatus::Closed),
            Some(NodeOutcome::Satisfied("node 3333cccc closed".into()))
        );
        assert_eq!(
            outcome(TaskStatus::NeedsParent, TaskStatus::Complete),
            Some(NodeOutcome::Fired("node 3333cccc reported complete".into()))
        );
        for status in [
            TaskStatus::Starting,
            TaskStatus::Running,
            TaskStatus::NeedsParent,
            TaskStatus::NeedsUser,
        ] {
            assert_eq!(outcome(TaskStatus::Complete, status), None, "{status:?}");
        }
    }
}
