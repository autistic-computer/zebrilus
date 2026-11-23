use ala::{AnyElement, App, Div, ElementId, Window, prelude::*, px};
use penna::{Size, Space};

use crate::{
    Icon, Sizable as _,
    agent::controls::SIGN_IN,
    button::{Button, ButtonVariants as _},
    feedback::{alert_actions, alert_ground, inline_alert},
    v_flex,
};

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

pub fn phone_error_card(error: &str, stopped: bool, ground: penna::Ground, cx: &App) -> AnyElement {
    penna::blocks::chat::phone_error_card(
        stopped.then(|| "The agent stopped".into()),
        rendered_error(error),
        ground,
        cx,
    )
}

pub fn phone_error_card_sign_in(
    id: impl Into<ElementId>,
    error: &str,
    stopped: bool,
    ground: penna::Ground,
    on_sign_in: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    v_flex()
        .w_full()
        .gap(Space::S8)
        .child(phone_error_card(error, stopped, ground, cx))
        .child(
            Button::new(id)
                .label(SIGN_IN)
                .primary()
                .with_size(Size::TouchLarge)
                .w_full()
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    on_sign_in(window, cx);
                }),
        )
        .into_any_element()
}

pub fn error_card(error: &str, stopped: bool, cx: &App) -> Div {
    inline_alert(
        stopped.then(|| "The agent stopped".into()),
        rendered_error(error),
        cx,
    )
}

pub fn error_card_sign_in(
    id: impl Into<ElementId>,
    error: &str,
    stopped: bool,
    on_sign_in: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    error_card(error, stopped, cx).child(
        alert_actions(cx).child(
            Button::new(id)
                .label(SIGN_IN)
                .compact()
                .primary()
                .on(alert_ground(cx))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    on_sign_in(window, cx);
                }),
        ),
    )
}
