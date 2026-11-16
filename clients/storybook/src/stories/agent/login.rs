use ala::{
    AnyElement, AnyView, App, AppContext as _, Entity, IntoElement as _, ParentElement as _,
    SharedString, Styled as _, Window, div, px,
};
use penna::{
    blocks::chat::{COMPOSER_MAX_WIDTH, COMPOSER_OUTER_PADDING},
    components::input::InputState,
};
use zz_client::agent_login::card;
use zz_protocol::{
    AgentLogin, AgentLoginBrowser, AgentLoginFailure, AgentLoginPane, AgentLoginStep,
    AgentProvider, PaneId,
};
use zz_ui::agent::login::card_view;

use super::composer::{Draft, claude_settings, composer, composer_input};
use crate::stories::workspace::fixtures::stateful;
use crate::story::states;

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

pub struct Cards {
    code: Entity<InputState>,
    composers: Vec<Entity<InputState>>,
}

fn new_cards(window: &mut Window, cx: &mut App) -> Cards {
    Cards {
        code: pasted(window, cx),
        composers: steps()
            .iter()
            .map(|_| composer_input("", window, cx))
            .collect(),
    }
}

pub fn cards(window: &mut Window, cx: &mut App) -> AnyView {
    stateful(window, cx, new_cards, render_cards)
}

fn render_cards(cards: &Cards, _: &mut Window, cx: &mut App) -> AnyElement {
    let mut shown = states();
    for (index, ((label, login), input)) in steps().into_iter().zip(&cards.composers).enumerate() {
        let (Some(pane), Some(card)) = (login.pane, card(&login)) else {
            continue;
        };
        let element = composer(
            Draft {
                id: SharedString::from(format!("sign-in-{index}")),
                input: input.clone(),
                running: false,
                writable: true,
                images: Vec::new(),
                prefix: vec![card_view(pane, &card, Some(&cards.code), |_, _, _| {}, cx)],
                mention: None,
                settings: claude_settings(),
            },
            cx,
        );
        shown = shown.state(
            label,
            div()
                .w_full()
                .max_w(px(COMPOSER_MAX_WIDTH + 2.0 * COMPOSER_OUTER_PADDING))
                .pt(px(COMPOSER_OUTER_PADDING))
                .child(element),
        );
    }
    shown.into_any_element()
}
