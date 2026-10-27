//! Messages between agents: one queue per recipient, persisted with the session.
//!
//! Pure data. Delivery, which needs panes and runtimes, lives in `agent_messages`.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::api::schema::{MessageInfo, MessageKind, MessageState};
use crate::terminal::AgentId;

/// Delivered messages kept for `message list` and `message show`; pending ones are never evicted.
pub(crate) const DELIVERED_HISTORY_LIMIT: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Message {
    pub id: u64,
    pub kind: MessageKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<AgentId>,
    pub from_name: String,
    pub to: AgentId,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<u64>,
    pub sent_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_at_ms: Option<u64>,
}

impl Message {
    pub(crate) fn is_pending(&self) -> bool {
        self.delivered_at_ms.is_none()
    }

    pub(crate) fn info(&self) -> MessageInfo {
        MessageInfo {
            message_id: self.id,
            kind: self.kind,
            state: if self.is_pending() {
                MessageState::Pending
            } else {
                MessageState::Delivered
            },
            from_agent_id: self.from.as_ref().map(ToString::to_string),
            from_name: self.from_name.clone(),
            to_agent_id: self.to.to_string(),
            summary: self.summary.clone(),
            body: self.body.clone(),
            reply_to: self.reply_to,
            sent_at_ms: self.sent_at_ms,
            delivered_at_ms: self.delivered_at_ms,
        }
    }
}

/// What a sender asks for; the store assigns the id.
pub(crate) struct NewMessage {
    pub kind: MessageKind,
    pub from: Option<AgentId>,
    pub from_name: String,
    pub to: AgentId,
    pub summary: String,
    pub body: Option<String>,
    pub reply_to: Option<u64>,
    pub sent_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct MessageStore {
    #[serde(default)]
    next_id: u64,
    /// Oldest first; ids only grow.
    #[serde(default)]
    messages: Vec<Message>,
}

impl MessageStore {
    pub(crate) fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.messages.iter().any(Message::is_pending)
    }

    pub(crate) fn push(&mut self, new: NewMessage) -> &Message {
        let id = self.next_id.max(self.last_id()) + 1;
        self.next_id = id;
        self.messages.push(Message {
            id,
            kind: new.kind,
            from: new.from,
            from_name: new.from_name,
            to: new.to,
            summary: new.summary,
            body: new.body,
            reply_to: new.reply_to,
            sent_at_ms: new.sent_at_ms,
            delivered_at_ms: None,
        });
        &self.messages[self.messages.len() - 1]
    }

    pub(crate) fn get(&self, id: u64) -> Option<&Message> {
        self.messages
            .binary_search_by_key(&id, |message| message.id)
            .ok()
            .map(|index| &self.messages[index])
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Message> {
        self.messages.iter()
    }

    /// Recipients with something pending, each once, in the order their oldest message arrived.
    pub(crate) fn pending_recipients(&self) -> Vec<AgentId> {
        let mut seen = HashSet::new();
        self.messages
            .iter()
            .filter(|message| message.is_pending() && seen.insert(message.to.clone()))
            .map(|message| message.to.clone())
            .collect()
    }

    /// The recipient's pending messages, oldest first.
    pub(crate) fn pending_for(&self, to: &AgentId) -> Vec<&Message> {
        self.messages
            .iter()
            .filter(|message| message.is_pending() && &message.to == to)
            .collect()
    }

    pub(crate) fn mark_delivered(&mut self, id: u64, at_ms: u64) -> Option<&Message> {
        let index = self
            .messages
            .binary_search_by_key(&id, |message| message.id)
            .ok()?;
        self.messages[index].delivered_at_ms = Some(at_ms);
        self.evict_delivered();
        self.get(id)
    }

    /// Removes a pending message. A delivered one cannot be taken back.
    pub(crate) fn withdraw(&mut self, id: u64) -> Result<Message, WithdrawError> {
        let index = self
            .messages
            .binary_search_by_key(&id, |message| message.id)
            .map_err(|_| WithdrawError::NotFound)?;
        if !self.messages[index].is_pending() {
            return Err(WithdrawError::Delivered);
        }
        Ok(self.messages.remove(index))
    }

    /// Drops messages to agents that no longer exist: nobody can receive or read them.
    pub(crate) fn retain_recipients(&mut self, live: &HashSet<AgentId>) -> usize {
        let before = self.messages.len();
        self.messages.retain(|message| live.contains(&message.to));
        before - self.messages.len()
    }

    fn evict_delivered(&mut self) {
        let delivered = self
            .messages
            .iter()
            .filter(|message| !message.is_pending())
            .count();
        let mut excess = delivered.saturating_sub(DELIVERED_HISTORY_LIMIT);
        if excess == 0 {
            return;
        }
        self.messages.retain(|message| {
            if excess > 0 && !message.is_pending() {
                excess -= 1;
                false
            } else {
                true
            }
        });
    }

    fn last_id(&self) -> u64 {
        self.messages.last().map_or(0, |message| message.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WithdrawError {
    NotFound,
    Delivered,
}

/// Past this many pending messages, a delivery is one line pointing at the list.
pub(crate) const DELIVERY_BLOCK_LIMIT: usize = 5;

/// The lines typed into the recipient's pane for its pending messages, oldest first: one line
/// each, under a header when there are several, or past `DELIVERY_BLOCK_LIMIT` a single pointer.
pub(crate) fn delivery_block(messages: &[&Message], to: &AgentId, now_ms: u64) -> Vec<String> {
    match messages {
        [] => Vec::new(),
        [message] => vec![delivery_line(message, now_ms)],
        _ if messages.len() > DELIVERY_BLOCK_LIMIT => vec![pointer_line(messages, to, now_ms)],
        _ => std::iter::once(format!(
            "[herdr] {} messages, oldest first:",
            messages.len()
        ))
        .chain(
            messages
                .iter()
                .map(|message| delivery_line(message, now_ms)),
        )
        .collect(),
    }
}

/// One message's line: sender, summary, kind, queue age and where the rest is.
fn delivery_line(message: &Message, now_ms: u64) -> String {
    const SUMMARY_LIMIT: usize = 240;
    let summary = one_line(&message.summary);
    let summary = if summary.chars().count() > SUMMARY_LIMIT {
        let cut: String = summary.chars().take(SUMMARY_LIMIT - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        summary
    };
    let kind = match (message.kind, message.reply_to) {
        (MessageKind::Reply, Some(reply_to)) => format!("reply to {reply_to}"),
        (MessageKind::Reply, None) => "reply".to_owned(),
        (MessageKind::Report, _) => "report".to_owned(),
        (MessageKind::Tell, _) => "tell".to_owned(),
    };
    format!(
        "[from {}] {summary} ({kind}, {}, herdr message show {})",
        one_line(&message.from_name),
        age(message.sent_at_ms, now_ms),
        message.id
    )
}

fn pointer_line(messages: &[&Message], to: &AgentId, now_ms: u64) -> String {
    const NAMED_SENDERS: usize = 4;
    let mut senders: Vec<String> = Vec::new();
    for message in messages {
        let name = one_line(&message.from_name);
        if !senders.contains(&name) {
            senders.push(name);
        }
    }
    let mut from = senders
        .iter()
        .take(NAMED_SENDERS)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if senders.len() > NAMED_SENDERS {
        from.push_str(&format!(" and {} more", senders.len() - NAMED_SENDERS));
    }
    let oldest = messages
        .first()
        .map_or(now_ms, |message| message.sent_at_ms);
    format!(
        "[herdr] {} messages from {from}, the oldest sent {}: `loom inbox` lists them, or `herdr message list --to {to}`",
        messages.len(),
        age(oldest, now_ms)
    )
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How long a message has waited, in the units loom's footer uses.
fn age(sent_at_ms: u64, now_ms: u64) -> String {
    let minutes = now_ms.saturating_sub(sent_at_ms) / 60_000;
    match minutes {
        0 => "just now".to_owned(),
        1..60 => format!("{minutes}m ago"),
        60..2880 => format!("{}h ago", minutes / 60),
        _ => format!("{}d ago", minutes / 1440),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str) -> AgentId {
        AgentId::parse(id).unwrap()
    }

    fn new(to: &str, summary: &str) -> NewMessage {
        NewMessage {
            kind: MessageKind::Report,
            from: Some(agent("1111aaaa")),
            from_name: "worker".into(),
            to: agent(to),
            summary: summary.into(),
            body: None,
            reply_to: None,
            sent_at_ms: 10,
        }
    }

    #[test]
    fn ids_grow_and_pending_goes_oldest_first_per_recipient() {
        let mut store = MessageStore::default();
        let first = store.push(new("2222bbbb", "one")).id;
        let other = store.push(new("3333cccc", "elsewhere")).id;
        let second = store.push(new("2222bbbb", "two")).id;
        assert_eq!((first, other, second), (1, 2, 3));
        assert_eq!(
            store.pending_recipients(),
            vec![agent("2222bbbb"), agent("3333cccc")]
        );

        let to = agent("2222bbbb");
        let ids = |store: &MessageStore| -> Vec<u64> {
            store.pending_for(&to).iter().map(|m| m.id).collect()
        };
        assert_eq!(ids(&store), vec![first, second]);
        store.mark_delivered(first, 20);
        assert_eq!(ids(&store), vec![second]);
        assert_eq!(store.get(first).unwrap().delivered_at_ms, Some(20));
        store.mark_delivered(second, 30);
        assert!(ids(&store).is_empty());
        assert!(store.has_pending(), "the other recipient still waits");
    }

    #[test]
    fn ids_are_not_reused_after_the_newest_message_goes() {
        let mut store = MessageStore::default();
        let id = store.push(new("2222bbbb", "one")).id;
        store.withdraw(id).unwrap();
        assert_eq!(store.push(new("2222bbbb", "two")).id, id + 1);
    }

    #[test]
    fn only_a_pending_message_can_be_withdrawn() {
        let mut store = MessageStore::default();
        let delivered = store.push(new("2222bbbb", "one")).id;
        let pending = store.push(new("2222bbbb", "two")).id;
        store.mark_delivered(delivered, 20);

        assert_eq!(store.withdraw(delivered), Err(WithdrawError::Delivered));
        assert_eq!(store.withdraw(99), Err(WithdrawError::NotFound));
        assert_eq!(store.withdraw(pending).unwrap().summary, "two");
        assert!(store.get(pending).is_none());
    }

    #[test]
    fn messages_to_a_gone_agent_are_dropped() {
        let mut store = MessageStore::default();
        store.push(new("2222bbbb", "kept"));
        store.push(new("3333cccc", "dropped"));
        let live = HashSet::from([agent("2222bbbb"), agent("1111aaaa")]);

        assert_eq!(store.retain_recipients(&live), 1);
        assert_eq!(store.iter().count(), 1);
        assert_eq!(store.iter().next().unwrap().summary, "kept");
    }

    #[test]
    fn delivered_history_is_capped_and_pending_messages_are_never_evicted() {
        let mut store = MessageStore::default();
        let pending = store.push(new("2222bbbb", "waiting")).id;
        for index in 0..DELIVERED_HISTORY_LIMIT + 5 {
            let id = store.push(new("2222bbbb", &format!("m{index}"))).id;
            store.mark_delivered(id, 20);
        }

        let delivered = store.iter().filter(|message| !message.is_pending()).count();
        assert_eq!(delivered, DELIVERED_HISTORY_LIMIT);
        assert!(store.get(pending).is_some());
        assert_eq!(
            store
                .iter()
                .find(|message| !message.is_pending())
                .unwrap()
                .summary,
            "m5",
            "the oldest delivered go first"
        );
    }

    #[test]
    fn the_delivery_line_is_one_line_with_the_pointer_kept() {
        let mut store = MessageStore::default();
        let id = store.push(new("2222bbbb", "done:\n  tests\tpass")).id;
        assert_eq!(
            delivery_line(store.get(id).unwrap(), 10),
            format!("[from worker] done: tests pass (report, just now, herdr message show {id})")
        );

        let long = "x".repeat(400);
        let id = store.push(new("2222bbbb", &long)).id;
        let line = delivery_line(store.get(id).unwrap(), 10);
        assert!(line.ends_with(&format!("… (report, just now, herdr message show {id})")));
        assert!(line.chars().count() < 300);
    }

    #[test]
    fn each_line_names_the_kind_and_how_long_the_message_waited() {
        const MINUTE: u64 = 60_000;
        let mut store = MessageStore::default();
        let mut tell = new("2222bbbb", "heads up");
        tell.kind = MessageKind::Tell;
        let tell = store.push(tell).id;
        let mut reply = new("2222bbbb", "yes");
        reply.kind = MessageKind::Reply;
        reply.reply_to = Some(tell);
        let reply = store.push(reply).id;
        let line = |id, waited| delivery_line(store.get(id).unwrap(), 10 + waited);

        assert!(line(tell, 59_999).ends_with("(tell, just now, herdr message show 1)"));
        assert!(line(tell, 12 * MINUTE).contains("(tell, 12m ago,"));
        assert!(line(tell, 47 * 60 * MINUTE).contains("(tell, 47h ago,"));
        assert!(line(tell, 3 * 1440 * MINUTE).contains("(tell, 3d ago,"));
        assert!(line(reply, 0).contains(&format!("(reply to {tell}, just now,")));
    }

    #[test]
    fn a_backlog_is_one_block_and_past_the_limit_one_pointer() {
        let to = agent("2222bbbb");
        let mut store = MessageStore::default();
        for (index, from) in ["a", "b", "a", "c", "d", "e"].into_iter().enumerate() {
            let mut message = new("2222bbbb", &format!("m{index}"));
            message.from_name = from.into();
            message.sent_at_ms = index as u64;
            store.push(message);
        }
        let pending = store.pending_for(&to);
        let at = 2 * 60 * 60_000;

        assert!(delivery_block(&[], &to, at).is_empty());
        assert_eq!(
            delivery_block(&pending[..1], &to, at),
            vec!["[from a] m0 (report, 2h ago, herdr message show 1)"]
        );
        let block = delivery_block(&pending[..DELIVERY_BLOCK_LIMIT], &to, at);
        assert_eq!(block.len(), DELIVERY_BLOCK_LIMIT + 1);
        assert_eq!(block[0], "[herdr] 5 messages, oldest first:");
        assert!(block[1].starts_with("[from a] m0 "));
        assert!(block[5].starts_with("[from d] m4 "));
        assert_eq!(
            delivery_block(&pending, &to, at),
            vec![
                "[herdr] 6 messages from a, b, c, d and 1 more, the oldest sent 2h ago: \
                 `loom inbox` lists them, or `herdr message list --to 2222bbbb`"
            ]
        );
    }

    #[test]
    fn the_store_round_trips_through_json() {
        let mut store = MessageStore::default();
        let id = store.push(new("2222bbbb", "one")).id;
        store.mark_delivered(id, 20);
        store.push(new("2222bbbb", "two"));

        let json = serde_json::to_string(&store).unwrap();
        let restored: MessageStore = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, store);
        assert_eq!(
            serde_json::from_str::<MessageStore>("{}").unwrap(),
            MessageStore::default()
        );
    }
}
