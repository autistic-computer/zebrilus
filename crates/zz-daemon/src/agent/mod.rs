pub(crate) mod catalog;
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) mod chat;
pub(crate) mod chat_bridge;
#[cfg(test)]
mod chat_bridge_tests;
#[cfg(test)]
mod chat_tests;
pub(crate) mod child;
pub(crate) mod claude;
#[cfg(unix)]
pub(crate) mod claude_peers;
pub(crate) mod codex;
#[cfg(unix)]
pub(crate) mod codex_queue;
pub(crate) mod environment;
pub(crate) mod fanout;
#[cfg(test)]
pub(crate) mod fixture;
pub(crate) mod git_summary;
pub(crate) mod host;
pub(crate) mod journal;
#[cfg(unix)]
pub(crate) mod lens;
pub(crate) mod paths;
pub(crate) mod runtime;
pub(crate) mod stream;
pub(crate) mod turn;
pub(crate) mod visualize;
pub(crate) mod zz_command;
