//! The agent lens over terminal panes, behind the `agent-pane` cargo feature.

pub(crate) mod attachment;

#[cfg(feature = "agent-pane")]
pub(crate) mod controller;
pub(crate) mod notify;
/// Unconditional: the workspace chrome renders agent badges in every build,
/// and without the feature the status map is simply always empty.
pub(crate) mod sound;
#[cfg(feature = "agent-pane")]
mod view;

#[cfg(feature = "agent-pane")]
pub use controller::AgentController;
#[cfg(feature = "agent-pane")]
pub(crate) use view::AgentView;

#[cfg(not(feature = "agent-pane"))]
mod stub;
#[cfg(not(feature = "agent-pane"))]
pub use stub::AgentController;
#[cfg(not(feature = "agent-pane"))]
pub(crate) use stub::AgentView;
