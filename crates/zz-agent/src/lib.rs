#![cfg_attr(
    test,
    allow(
        clippy::disallowed_methods,
        reason = "tests spawn helper processes from threads with an empty signal mask"
    )
)]

pub mod accounts;
pub mod acp_chat;
pub mod agy;
pub mod capture;
pub mod catalog;
#[cfg_attr(not(test), allow(dead_code))]
pub mod chat;
pub mod chat_bridge;
#[cfg(test)]
mod chat_bridge_tests;
#[cfg(test)]
mod chat_tests;
pub mod child;
pub mod claude;
#[cfg(unix)]
pub mod claude_peers;
#[cfg(unix)]
pub mod cli_session;
pub mod codex;
#[cfg(unix)]
pub mod codex_queue;
pub mod cursor;
#[cfg(test)]
pub mod cut_points;
pub mod dialect;
pub mod environment;
pub mod fanout;
#[cfg(any(test, feature = "test-support"))]
pub mod fixture;
pub mod git_summary;
pub mod grok;
pub mod host;
pub mod journal;
#[cfg(unix)]
pub mod login;
#[cfg(unix)]
pub mod logins;
pub mod paths;
pub mod runtime;
pub mod session_files;
pub mod stream;
pub mod tape;
pub mod turn;
pub mod visualize;
pub mod zz_command;

pub const fn sandboxed() -> bool {
    cfg!(any(test, feature = "test-support"))
}
