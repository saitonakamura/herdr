mod agent_id;
mod history_read;
mod id;
mod runtime;
mod runtime_registry;
pub mod state;
mod title;

pub use agent_id::AgentId;
pub(crate) use history_read::{merge_scrolled_up, snapshot_text, ScreenSnapshot, UpwardMerge};
pub use id::TerminalId;
pub use runtime::TerminalRuntime;
pub(crate) use runtime_registry::TerminalRuntimeRegistry;
pub use state::{
    AgentMetadataReport, EffectivePresentation, EffectiveStateChange, HookReportDetail,
    TerminalState, TerminalStateMutation,
};
pub(crate) use title::stripped_terminal_title;
