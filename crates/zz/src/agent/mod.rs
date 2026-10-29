pub(crate) mod attachment;

#[cfg(feature = "agent-pane")]
pub(crate) mod controller;
#[cfg(feature = "agent-pane")]
pub(crate) mod notify;
pub(crate) mod sound;
#[cfg(feature = "agent-pane")]
mod view;

#[cfg(feature = "agent-pane")]
pub use controller::AgentController;
#[cfg(feature = "agent-pane")]
pub(crate) use view::{AgentView, ShowTerminal};

#[cfg(not(feature = "agent-pane"))]
mod stub;
#[cfg(not(feature = "agent-pane"))]
pub use stub::AgentController;
#[cfg(not(feature = "agent-pane"))]
pub(crate) use stub::{AgentView, ShowTerminal};
