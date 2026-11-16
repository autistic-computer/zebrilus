use ala::{
    AnyElement, AnyView, App, AppContext as _, Entity, IntoElement as _, ParentElement as _,
    Styled as _, Window, div, px,
};
use penna::components::input::InputState;
use zz_client::agent_login::card;
use zz_protocol::{
    AgentLogin, AgentLoginBrowser, AgentLoginFailure, AgentLoginPane, AgentLoginStep,
    AgentProvider, PaneId,
};
use zz_ui::agent::login::card_view;

use crate::stories::workspace::fixtures::stateful;
use crate::story::states;

const CARD_WIDTH: f32 = 456.0;
pub const CLAUDE_URL: &str = "https://claude.ai/oauth/authorize?code=true&client_id=9d1c25&response_type=code&scope=user%3Ainference";
pub const OPENAI_URL: &str = "https://auth.openai.com/codex/device";
pub const PASTED: &str = "aZ3kQ9vL2xR8mT4p#c1f7e2b9";

#[must_use]
pub fn login(
    provider: AgentProvider,
    browser: AgentLoginBrowser,
    step: AgentLoginStep,
) -> AgentLogin {
    AgentLogin {
        id: 1,
        provider,
        account: Some("work".to_owned()),
        pane: Some(PaneId(10)),
        browser,
        step,
    }
}

#[must_use]
pub fn steps() -> Vec<(&'static str, AgentLogin)> {
    use AgentLoginBrowser::{Elsewhere, Local};
    use AgentProvider::{ClaudeCode, Codex};
    vec![
        (
            "Starting",
            login(ClaudeCode, Local, AgentLoginStep::Starting),
        ),
        (
            "browser on this machine: the page opened, open it again, copy it, or use a code",
            login(
                ClaudeCode,
                Local,
                AgentLoginStep::OpenUrl {
                    url: CLAUDE_URL.to_owned(),
                },
            ),
        ),
        (
            "Claude, browser elsewhere: open the page, paste the code it shows, Continue or Enter",
            login(
                ClaudeCode,
                Elsewhere,
                AgentLoginStep::PasteCode {
                    url: CLAUDE_URL.to_owned(),
                },
            ),
        ),
        (
            "ChatGPT, browser elsewhere: open the page and type the code there",
            login(
                Codex,
                Elsewhere,
                AgentLoginStep::DeviceCode {
                    url: OPENAI_URL.to_owned(),
                    code: "7KQM-4XWD".to_owned(),
                },
            ),
        ),
        (
            "Verifying",
            login(Codex, Elsewhere, AgentLoginStep::Verifying),
        ),
        (
            "failed: device code sign-in is off for the account",
            login(
                Codex,
                Elsewhere,
                AgentLoginStep::Failed(AgentLoginFailure::DeviceCodeDisabled),
            ),
        ),
        (
            "failed: the CLI is not installed",
            login(
                ClaudeCode,
                Local,
                AgentLoginStep::Failed(AgentLoginFailure::NotInstalled),
            ),
        ),
        (
            "failed: timed out",
            login(
                ClaudeCode,
                Elsewhere,
                AgentLoginStep::Failed(AgentLoginFailure::TimedOut),
            ),
        ),
        (
            "failed: the CLI's own text",
            login(
                ClaudeCode,
                Elsewhere,
                AgentLoginStep::Failed(AgentLoginFailure::Cli(
                    "OAuth error: invalid code".to_owned(),
                )),
            ),
        ),
        (
            "added while the chat is busy: the busy card for the new account",
            login(
                Codex,
                Elsewhere,
                AgentLoginStep::Done {
                    email: Some("me@example.com".to_owned()),
                    organization: None,
                    plan: Some("plus".to_owned()),
                    pane: Some(AgentLoginPane::Busy),
                },
            ),
        ),
    ]
}

#[must_use]
pub fn pasted(window: &mut Window, cx: &mut App) -> Entity<InputState> {
    cx.new(|cx| {
        let mut input = InputState::new(window, cx).placeholder("Code");
        input.set_value(PASTED, window, cx);
        input
    })
}

pub fn cards(window: &mut Window, cx: &mut App) -> AnyView {
    stateful(window, cx, pasted, render_cards)
}

fn render_cards(code: &Entity<InputState>, _: &mut Window, cx: &mut App) -> AnyElement {
    steps()
        .into_iter()
        .filter_map(|(label, login)| Some((label, login.pane?, card(&login)?)))
        .fold(states(), |states, (label, pane, card)| {
            states.state(
                label,
                div().w_full().max_w(px(CARD_WIDTH)).child(card_view(
                    pane,
                    &card,
                    Some(code),
                    |_, _, _| {},
                    cx,
                )),
            )
        })
        .into_any_element()
}
