use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::popup_size::PopupSize;

/// Fork-only: runs `argv` in the session's popup, the way a `type = "popup"`
/// custom command does. Fails with `popup_already_open` while one is open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PopupOpenParams {
    pub argv: Vec<String>,
    /// Defaults to the focused pane's cwd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<PopupSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<PopupSize>,
}
