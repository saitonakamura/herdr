use serde::{Deserialize, Serialize};

use crate::cli::projection::{cli_params, Field, Input};
use crate::cli::workspace_bindings as bindings;

pub(crate) fn workspace() -> Input<Option<String>> {
    // With `cwd` too, the worktree comes from `cwd` and its tab opens in the workspace.
    super::tabs::workspace_option("ID")
}

fn json_compatibility() -> Input<bool> {
    Input::flag("json", |_, _| Ok(())).hidden()
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
    pub struct WorktreeListParams => WorktreeListProjection {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub workspace_id: Option<String> => Field::new(None).input(workspace().order(0)),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cwd: Option<String> => Field::new(None).input(crate::cli::worktree::bindings::path("cwd").order(1)),
        #[serde(default, skip_serializing_if = "super::is_false")]
        pub trust_repository: bool => Field::new(false)
            .input(bindings::boolean("trust-repository", true).order(2))
            .input(json_compatibility()),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
    pub struct WorktreeCreateParams => WorktreeCreateProjection {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub workspace_id: Option<String> => Field::new(None).input(workspace().order(0)),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cwd: Option<String> => Field::new(None).input(crate::cli::worktree::bindings::path("cwd").order(1)),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub branch: Option<String> => Field::new(None).input(bindings::optional_text("branch", "NAME").order(2)),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub base: Option<String> => Field::new(None).input(bindings::optional_text("base", "REF").order(3)),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String> => Field::new(None).input(crate::cli::worktree::bindings::path("path").order(4)),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub label: Option<String> => Field::new(None).input(bindings::optional_text("label", "TEXT").order(5)),
        #[serde(default)]
        pub focus: bool => Field::new(false)
            .input(bindings::boolean("focus", true).order(6))
            .input(bindings::boolean("no-focus", false).order(7)),
        #[serde(default, skip_serializing_if = "super::is_false")]
        pub trust_repository: bool => Field::new(false)
            .input(bindings::boolean("trust-repository", true).order(8))
            .input(json_compatibility()),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
    pub struct WorktreeOpenParams => WorktreeOpenProjection {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub workspace_id: Option<String> => Field::new(None).input(workspace().order(0)),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cwd: Option<String> => Field::new(None).input(crate::cli::worktree::bindings::path("cwd").order(1)),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String> => Field::new(None).input(crate::cli::worktree::bindings::path("path").order(2)),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub branch: Option<String> => Field::new(None).input(bindings::optional_text("branch", "NAME").order(3)),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub label: Option<String> => Field::new(None).input(bindings::optional_text("label", "TEXT").order(4)),
        #[serde(default)]
        pub focus: bool => Field::new(false)
            .input(bindings::boolean("focus", true).order(5))
            .input(bindings::boolean("no-focus", false).order(6)),
        #[serde(default, skip_serializing_if = "super::is_false")]
        pub trust_repository: bool => Field::new(false)
            .input(bindings::boolean("trust-repository", true).order(7))
            .input(json_compatibility()),
    }
}

// The workspace-addressed remove older clients send; the CLI uses
// `WorktreeRemoveTabParams`. No doc comments: they would change the frozen shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorktreeRemoveParams {
    pub workspace_id: String,
    #[serde(default)]
    pub force: bool,
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub trust_repository: bool,
}

cli_params! {
    /// Removes the checkout behind a worktree tab and closes the tab: the tab
    /// named, or the one worktree tab of the workspace named.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
    pub struct WorktreeRemoveTabParams => WorktreeRemoveTabProjection {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub tab_id: Option<String> => Field::new(None).input(Input::value("tab", "ID", |slot, value| {
            *slot = Some(crate::cli::normalize_tab_id(value));
            Ok(())
        }).order(0)),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub workspace_id: Option<String> => Field::new(None).input(super::tabs::workspace_option("ID").order(1)),
        #[serde(default)]
        pub force: bool => Field::new(false).input(bindings::boolean("force", true).order(2)),
        #[serde(default, skip_serializing_if = "super::is_false")]
        pub trust_repository: bool => Field::new(false)
            .input(bindings::boolean("trust-repository", true).order(3))
            .input(json_compatibility()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorktreeSourceInfo {
    pub repo_key: String,
    pub repo_name: String,
    pub repo_root: String,
    pub source_checkout_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_workspace_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorktreeInfo {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub is_bare: bool,
    pub is_detached: bool,
    pub is_prunable: bool,
    pub is_linked_worktree: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_workspace_id: Option<String>,
    /// The tab open on this checkout, if any; `open_workspace_id` is its workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_tab_id: Option<String>,
    pub label: String,
}
