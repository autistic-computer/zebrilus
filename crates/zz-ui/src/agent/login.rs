use std::rc::Rc;

use ala::{
    AnyElement, App, ClipboardItem, Context, Entity, IntoElement, SharedString, Subscription,
    Window, div, prelude::*, px,
};
use penna::{Chip, ChipLook, Label, TextStyle, Well, components::spinner::Spinner};
use zz_client::agent_login::{self, CardAction, CardClose, CardGlyph, LoginCard};
use zz_protocol::{AgentLogin, AgentLoginStep, PaneId, ProtocolMessage};

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Icon, IconName, Sizable as _, StyledExt as _,
    button::{Button, ButtonLook, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex,
};

const PAD: f32 = 10.0;
const GAP: f32 = 10.0;
const LINE: f32 = 16.0;
const GLYPH: f32 = 13.0;
const GLYPH_DROP: f32 = 1.5;
const HEAD_GAP: f32 = 7.0;
const LINK_HEIGHT: f32 = 32.0;
const LINK_GLYPH: f32 = 12.0;
const CODE_HEIGHT: f32 = 64.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginEvent {
    Close,
    Action(CardAction),
}

pub struct CodeField {
    login: u64,
    pub input: Entity<InputState>,
    _enter: Subscription,
}

impl CodeField {
    pub fn sync<V: 'static>(
        slot: &mut Option<Self>,
        login: Option<&AgentLogin>,
        window: &mut Window,
        cx: &mut Context<V>,
        on_enter: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    ) {
        let pasting = login.filter(|login| matches!(login.step, AgentLoginStep::PasteCode { .. }));
        let Some(login) = pasting else {
            *slot = None;
            return;
        };
        if slot.as_ref().is_some_and(|field| field.login == login.id) {
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Code"));
        let enter = cx.subscribe_in(&input, window, move |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { shift: false }) {
                on_enter(this, window, cx);
            }
        });
        *slot = Some(Self {
            login: login.id,
            input,
            _enter: enter,
        });
    }

    pub fn take(
        &self,
        login: &AgentLogin,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<ProtocolMessage> {
        if self.login != login.id {
            return None;
        }
        let message = agent_login::submit_code(login, &self.input.read(cx).value())?;
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        Some(message)
    }
}

type OnLogin = Rc<dyn Fn(LoginEvent, &mut Window, &mut App)>;

fn glyph(glyph: CardGlyph, pane: PaneId, cx: &App) -> AnyElement {
    let muted = cx.theme().foreground.muted();
    let inner = match glyph {
        CardGlyph::Working => Spinner::new()
            .id(("agent-login-spinner", pane.0))
            .with_size(px(GLYPH))
            .color(muted)
            .into_any_element(),
        CardGlyph::Failed => Icon::new(IconName::TriangleAlert)
            .size(px(GLYPH))
            .text_color(cx.theme().danger)
            .into_any_element(),
        CardGlyph::Waiting => Icon::new(IconName::Clock)
            .size(px(GLYPH))
            .text_color(muted)
            .into_any_element(),
    };
    div()
        .flex_none()
        .relative()
        .top(px(GLYPH_DROP))
        .child(inner)
        .into_any_element()
}

fn row_icon(
    id: impl Into<ala::ElementId>,
    icon: IconName,
    tooltip: &'static str,
    cx: &App,
) -> Button {
    Button::compact_icon(id, icon)
        .look(ButtonLook::RowIcon)
        .on(cx.theme().ground_of(ChipLook::Floating))
        .text_color(cx.theme().foreground.muted())
        .tooltip(tooltip)
}

fn copy(text: String) -> impl Fn(&ala::ClickEvent, &mut Window, &mut App) + 'static {
    move |_, _, cx| {
        cx.stop_propagation();
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
    }
}

pub fn link_row(pane: PaneId, url: &str, cx: &App) -> AnyElement {
    let muted = cx.theme().foreground.muted();
    let open = url.to_owned();
    Well::new()
        .flex()
        .items_center()
        .gap(px(4.0))
        .h(px(LINK_HEIGHT))
        .pl(px(10.0))
        .pr(px(4.0))
        .child(
            Icon::new(IconName::Globe)
                .size(px(LINK_GLYPH))
                .flex_none()
                .text_color(muted),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .pl(px(4.0))
                .type_style(TextStyle::Small, cx)
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(muted)
                .truncate()
                .child(SharedString::from(agent_login::shown_url(url).to_owned())),
        )
        .child(
            row_icon(
                ("agent-login-copy-link", pane.0),
                IconName::Copy,
                "Copy link",
                cx,
            )
            .on_click(copy(url.to_owned())),
        )
        .child(
            row_icon(
                ("agent-login-open-link", pane.0),
                IconName::ExternalLink,
                "Open again",
                cx,
            )
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                cx.open_url(&open);
            }),
        )
        .into_any_element()
}

fn code_tile(pane: PaneId, code: &str, cx: &App) -> AnyElement {
    Well::new()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .h(px(CODE_HEIGHT))
        .type_style(TextStyle::Display, cx)
        .font_family(cx.theme().mono_font_family.clone())
        .child(SharedString::from(code.to_owned()))
        .child(
            div().absolute().right(px(8.0)).child(
                row_icon(
                    ("agent-login-copy-code", pane.0),
                    IconName::Copy,
                    "Copy code",
                    cx,
                )
                .on_click(copy(code.to_owned())),
            ),
        )
        .into_any_element()
}

fn action_button(pane: PaneId, action: CardAction, on: &OnLogin, cx: &App) -> Button {
    let on = Rc::clone(on);
    Button::new(SharedString::from(format!(
        "agent-login-{}-{}",
        pane.0,
        action.label()
    )))
    .label(action.label())
    .compact()
    .on(cx.theme().ground_of(ChipLook::Floating))
    .when(action == CardAction::Terminal, |button| {
        button.icon(IconName::SquareTerminal)
    })
    .map(|button| {
        if action.primary() {
            button.primary()
        } else {
            button.emphasis(Emphasis::Ghost)
        }
    })
    .on_click(move |_, window, cx| {
        cx.stop_propagation();
        on(LoginEvent::Action(action), window, cx);
    })
}

pub fn login_card(
    login: &AgentLogin,
    field: Option<&CodeField>,
    on_event: impl Fn(LoginEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Option<AnyElement> {
    let pane = login.pane?;
    let card = agent_login::card(login)?;
    Some(card_view(
        pane,
        &card,
        field.map(|field| &field.input),
        on_event,
        cx,
    ))
}

pub fn card_view(
    pane: PaneId,
    card: &LoginCard,
    field: Option<&Entity<InputState>>,
    on_event: impl Fn(LoginEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let on: OnLogin = Rc::new(on_event);
    let close = {
        let on = Rc::clone(&on);
        let tooltip = match card.close {
            CardClose::Cancel => "Cancel",
            CardClose::Dismiss => "Close",
        };
        row_icon(("agent-login-close", pane.0), IconName::Xmark, tooltip, cx)
            .mt(px(-4.0))
            .mr(px(-4.0))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                on(LoginEvent::Close, window, cx);
            })
    };
    let head = h_flex()
        .w_full()
        .items_start()
        .gap(px(HEAD_GAP))
        .child(glyph(card.glyph, pane, cx))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    Label::new(TextStyle::Body, card.title.clone())
                        .strong()
                        .line_height(px(LINE)),
                )
                .children(card.detail.clone().map(|detail| {
                    div()
                        .text_color(cx.theme().foreground.muted())
                        .child(detail)
                })),
        )
        .child(close);
    let (lead, trail): (Vec<CardAction>, Vec<CardAction>) =
        card.actions.iter().partition(|action| action.leads());
    let actions = (!card.actions.is_empty()).then(|| {
        h_flex()
            .w_full()
            .items_center()
            .gap(px(6.0))
            .children(
                lead.into_iter()
                    .map(|action| action_button(pane, action, &on, cx)),
            )
            .child(div().flex_1())
            .children(
                trail
                    .into_iter()
                    .map(|action| action_button(pane, action, &on, cx)),
            )
    });
    let field = card.paste.then_some(field).flatten().map(|input| {
        Input::new(input)
            .w_full()
            .font_family(cx.theme().mono_font_family.clone())
            .on(cx.theme().ground_of(ChipLook::Floating))
    });
    Chip::empty(("agent-login", pane.0))
        .look(ChipLook::Floating)
        .multiline(true)
        .debug_selector(|| "agent-login-card".into())
        .flex()
        .flex_col()
        .items_stretch()
        .gap(px(GAP))
        .w_full()
        .h_auto()
        .p(px(PAD))
        .font_family(cx.theme().font_family.clone())
        .type_style(TextStyle::Body, cx)
        .line_height(px(LINE))
        .on_mouse_down(ala::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(head)
        .children(card.url.as_deref().map(|url| link_row(pane, url, cx)))
        .children(card.code.as_deref().map(|code| code_tile(pane, code, cx)))
        .children(field)
        .children(actions)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ala::{Render, TestAppContext};
    use zz_protocol::{AgentLoginBrowser, AgentLoginCode, AgentLoginRequest, AgentProvider};

    struct Host {
        code: Option<CodeField>,
        sent: Vec<ProtocolMessage>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn login(step: AgentLoginStep) -> AgentLogin {
        AgentLogin {
            id: 3,
            provider: AgentProvider::ClaudeCode,
            account: None,
            pane: Some(PaneId(2)),
            browser: AgentLoginBrowser::Elsewhere,
            step,
        }
    }

    #[ala::test]
    fn a_pasted_code_goes_out_once_in_its_own_message_and_leaves_the_field(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init);
        let paste = login(AgentLoginStep::PasteCode {
            url: "https://claude.ai/oauth/authorize".to_owned(),
        });
        let (view, cx) = cx.add_window_view(|_, _| Host {
            code: None,
            sent: Vec::new(),
        });
        cx.update(|window, cx| {
            view.update(cx, |host, cx| {
                CodeField::sync(&mut host.code, Some(&paste), window, cx, |_, _, _| {});
                let input = host.code.as_ref().expect("a field").input.clone();
                input.update(cx, |input, cx| input.set_value(" aZ3k#c1f7 ", window, cx));
                let field = host.code.take().expect("a field");
                host.sent.extend(field.take(&paste, window, cx));
                host.sent.extend(field.take(&paste, window, cx));
                assert_eq!(field.input.read(cx).value(), "");
                host.code = Some(field);
                CodeField::sync(
                    &mut host.code,
                    Some(&login(AgentLoginStep::Verifying)),
                    window,
                    cx,
                    |_, _, _| {},
                );
            });
        });
        let host = view.read_with(cx, |host, _| (host.sent.clone(), host.code.is_none()));
        assert_eq!(
            host.0,
            [ProtocolMessage::AgentLogin(AgentLoginRequest::Code {
                login: 3,
                code: AgentLoginCode::new("aZ3k#c1f7").expect("a code"),
            })]
        );
        assert!(host.1, "the field goes when the step moves on");
    }
}
