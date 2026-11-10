use ala::{App, Div, prelude::*, px};

use crate::{Icon, feedback::inline_alert};

pub use zz_client::agent_completion::{MAX_RENDERED_ERROR_BYTES, rendered_error};

pub fn agent_welcome(provider: zz_protocol::AgentProvider, cx: &App) -> Div {
    penna::blocks::chat::welcome_state(
        Icon::new(crate::pane::agent_provider_icon(provider)).size(px(40.0)),
        zz_client::agent_lens::agent_welcome_title(provider),
        zz_client::agent_lens::AGENT_WELCOME_DETAIL,
        cx,
    )
}

pub fn agent_resume_welcome(provider: zz_protocol::AgentProvider, cx: &App) -> Div {
    penna::blocks::chat::welcome_state(
        Icon::new(crate::pane::agent_provider_icon(provider)).size(px(40.0)),
        zz_client::agent_lens::agent_welcome_title(provider),
        zz_client::agent_lens::AGENT_RESUME_DETAIL,
        cx,
    )
    .h_auto()
    .flex_none()
    .pb(px(18.0))
}

pub fn error_card(error: &str, stopped: bool, cx: &App) -> Div {
    inline_alert(
        stopped.then(|| "The agent stopped".into()),
        rendered_error(error),
        cx,
    )
}
