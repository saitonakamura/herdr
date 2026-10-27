use serde::{Deserialize, Serialize};

use crate::cli::message::bindings;
use crate::cli::projection::{cli_params, Field};

/// What a message is to its recipient: a `report` upward, a `reply` to a report,
/// or a `tell` downward. Herdr does not enforce the direction.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Report,
    Reply,
    #[default]
    Tell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessageState {
    /// Waiting for the recipient to be idle, or blocked on a question with an empty composer.
    Pending,
    /// Typed into the recipient's pane, on its own line or counted in a pointer to the list.
    Delivered,
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct MessageSendParams => MessageSendProjection {
        /// Recipient: agent id, agent name, or pane id. A pane without an agent gets an agent id.
        pub to: String => Field::new(String::new()).input(bindings::to_position()),
        /// One line, typed into the recipient's pane on delivery.
        pub summary: String => Field::new(String::new()).input(bindings::summary_position()),
        #[serde(default)]
        pub kind: MessageKind => Field::new(MessageKind::Tell).input(bindings::kind()),
        /// The rest of the message, shown by `message.get`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub body: Option<String> => Field::new(None).input(bindings::body()),
        /// Sender: agent id, agent name, or pane id. Omit to send as the user.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub from: Option<String> => Field::new(None).input(bindings::from()).input(bindings::user()),
        /// The message this one answers.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub reply_to: Option<u64> => Field::new(None).input(bindings::reply_to()),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct MessageListParams => MessageListProjection {
        /// Only messages to this agent id, agent name, or pane id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub to: Option<String> => Field::new(None).input(bindings::to_filter()),
        /// Only messages from this agent id, agent name, or pane id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub from: Option<String> => Field::new(None).input(bindings::from_filter()),
        /// Only messages not yet delivered.
        #[serde(default, skip_serializing_if = "super::is_false")]
        pub pending: bool => Field::new(false).input(bindings::pending()),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct MessageTarget => MessageTargetProjection {
        pub message_id: u64 => Field::new(0).input(bindings::message_id()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MessageInfo {
    pub message_id: u64,
    pub kind: MessageKind,
    pub state: MessageState,
    /// Absent when the user sent it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_agent_id: Option<String>,
    /// The sender's agent name when it was sent, else its agent id, or `user`.
    pub from_name: String,
    pub to_agent_id: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<u64>,
    /// Unix time in milliseconds.
    pub sent_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_at_ms: Option<u64>,
}
